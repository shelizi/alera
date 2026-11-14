//! Revision-checked, no-start Host administration of AGY account labels.
//! This does not access Credential Manager, tokens or an AGY process.
//! A Google account label is NOT an independently authenticated identity.

use alera_core::runtime::{RuntimeAgentQuotaSettings, RuntimeAgyQuotaProfile};
use serde_json::{json, Value};

use crate::cli::{RuntimeAgyAccountsAction, RuntimeDirArgs};
use crate::runtime_host_client::RuntimeHostRpcClient;
use crate::runtime_os_identity::HostOsIdentity;
use crate::{print_error, print_value, runtime_dir};

fn decode_host_snapshot(value: &Value) -> Result<(i64, RuntimeAgentQuotaSettings), &'static str> {
    let revision = value
        .get("revision")
        .and_then(Value::as_i64)
        .filter(|revision| *revision >= 0)
        .ok_or("Host did not supply a settings revision")?;
    let quotas = value
        .get("agentQuotas")
        .ok_or("Host quota settings unavailable")?;
    // Old Hosts silently discard unknown fields, so never pretend a save
    // worked unless this version explicitly advertises AGY registrations.
    if quotas.get("agyProfiles").is_none() {
        return Err("Runtime Host does not support AGY accounts; update the Host");
    }
    let settings: RuntimeAgentQuotaSettings =
        serde_json::from_value(quotas.clone()).map_err(|_| "Host quota settings are invalid")?;
    Ok((revision, settings))
}

fn validate_account_id(account: &str) -> Result<(), &'static str> {
    if account == "default"
        || account.is_empty()
        || account.len() > 64
        || !account
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
    {
        return Err("Use 1-64 letters, digits, '-' or '_' as account ID (not 'default')");
    }
    Ok(())
}

fn apply_account_change(
    settings: &mut RuntimeAgentQuotaSettings,
    action: &RuntimeAgyAccountsAction,
) -> Result<(), &'static str> {
    match action {
        RuntimeAgyAccountsAction::Get => Ok(()),
        RuntimeAgyAccountsAction::Upsert {
            account_id, alias, ..
        } => {
            validate_account_id(account_id)?;
            let name = alias.trim();
            if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
                return Err("Account name must be 1-80 characters without control characters");
            }
            if settings
                .agy_profiles
                .iter()
                .any(|entry| entry.alias == name && entry.account_id != *account_id)
            {
                return Err("An Antigravity registration with this name already exists");
            }
            if let Some(existing) = settings
                .agy_profiles
                .iter_mut()
                .find(|entry| entry.account_id == *account_id)
            {
                existing.alias = name.to_owned();
            } else {
                if settings.agy_profiles.len() >= 32 {
                    return Err("Maximum 32 additional Antigravity accounts per Host");
                }
                settings.agy_profiles.push(RuntimeAgyQuotaProfile {
                    alias: name.to_owned(),
                    account_id: account_id.clone(),
                });
            }
            Ok(())
        }
        RuntimeAgyAccountsAction::Remove { account_id, .. } => {
            validate_account_id(account_id)?;
            let before = settings.agy_profiles.len();
            settings
                .agy_profiles
                .retain(|entry| entry.account_id != *account_id);
            if settings.agy_profiles.len() == before {
                return Err("Unknown Antigravity registration; refresh and retry");
            }
            // Group policy is only metadata: never delete native conversations,
            // active sessions, backups or credentials.
            settings
                .history_sync
                .account_groups
                .remove(&format!("agy:{account_id}"));
            Ok(())
        }
    }
}

/// The real keyring is scoped to the Host OS user, not to a HOME directory
/// or an arbitrary account alias. Expose capability limitations explicitly.
fn account_response(
    revision: i64,
    settings: &RuntimeAgentQuotaSettings,
    host_os_identity: Option<&HostOsIdentity>,
) -> Value {
    json!({
        "revision": revision,
        "agyAccounts": settings.agy_profiles,
        "hostOsIdentity": host_os_identity,
        "identityCapability": {
            "credentialScope": "host-os-user",
            "accountAuthentication": "unverified",
            "hostOsUserProof": if host_os_identity.is_some() { "os-token-digest" } else { "unavailable" },
            "supportsIndependentLocalAccounts": false,
            "supportsAccountSwitching": false,
            "recommendedIsolation": "separate-os-user-runtime-host",
            "canRegisterNames": true
        }
    })
}

pub(crate) async fn run(
    runtime: &RuntimeDirArgs,
    json_output: bool,
    action: RuntimeAgyAccountsAction,
) -> i32 {
    // This connect path never starts or replaces the real Runtime Host.
    let mut client = match RuntimeHostRpcClient::connect(&runtime_dir(runtime)).await {
        Ok(Some(client)) => client,
        Ok(None) => return print_error("Existing Runtime Host is offline"),
        Err(error) => return print_error(error),
    };
    let raw = match client
        .request_value_with_deadline("runtimeSettings.get", &json!({}), 5500)
        .await
    {
        Ok(value) => value,
        Err(error) => return print_error(error),
    };
    let (revision, mut settings) = match decode_host_snapshot(&raw) {
        Ok(value) => value,
        Err(error) => return print_error(error),
    };
    let host_os_identity = raw
        .get("hostOsIdentity")
        .cloned()
        .and_then(|value| serde_json::from_value::<HostOsIdentity>(value).ok())
        .filter(HostOsIdentity::validated);
    if matches!(action, RuntimeAgyAccountsAction::Get) {
        print_value(
            &account_response(revision, &settings, host_os_identity.as_ref()),
            json_output,
            "Runtime Host Antigravity registrations loaded",
        );
        return 0;
    }
    let expected = match &action {
        RuntimeAgyAccountsAction::Get => unreachable!(),
        RuntimeAgyAccountsAction::Upsert {
            expected_revision, ..
        }
        | RuntimeAgyAccountsAction::Remove {
            expected_revision, ..
        } => *expected_revision,
    };
    if expected != revision {
        return print_error("Runtime account registrations changed; refresh and retry");
    }
    if let Err(error) = apply_account_change(&mut settings, &action) {
        return print_error(error);
    }
    let wanted_accounts = json!(settings.agy_profiles);
    let wanted_groups = json!(settings.history_sync.account_groups);
    let result = match client
        .request_value_with_deadline(
            "runtimeSettings.update",
            &json!({"expectedRevision": revision, "agentQuotas": settings}),
            6500,
        )
        .await
    {
        Ok(value) => value,
        Err(error) => return print_error(error),
    };
    if result.pointer("/agentQuotas/agyProfiles") != Some(&wanted_accounts)
        || result.pointer("/agentQuotas/historySync/accountGroups") != Some(&wanted_groups)
    {
        return print_error("Host did not confirm AGY account changes; refresh required");
    }
    let updated_revision = match result.get("revision").and_then(Value::as_i64) {
        Some(version) if version > revision => version,
        _ => return print_error("Host did not confirm updated settings revision"),
    };
    let result_settings = match decode_host_snapshot(&result) {
        Ok((_, values)) => values,
        Err(error) => return print_error(error),
    };
    print_value(
        &account_response(
            updated_revision,
            &result_settings,
            host_os_identity.as_ref(),
        ),
        json_output,
        "Antigravity registration saved on Runtime Host",
    );
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use alera_core::runtime::{RuntimeAgentHistorySyncMode, RuntimeAgentHistorySyncSettings};

    fn sample() -> RuntimeAgentQuotaSettings {
        let mut settings = RuntimeAgentQuotaSettings::default();
        settings.history_sync = RuntimeAgentHistorySyncSettings {
            mode: RuntimeAgentHistorySyncMode::Groups,
            ..Default::default()
        };
        settings
    }

    #[test]
    fn rejects_legacy_host_and_preserves_empty_accounts() {
        assert!(
            decode_host_snapshot(&json!({"revision":0,"agentQuotas":{"historySync":{}}})).is_err()
        );
        let value = serde_json::to_value(sample()).unwrap();
        let (_, settings) =
            decode_host_snapshot(&json!({"revision":0,"agentQuotas":value})).unwrap();
        assert!(settings.agy_profiles.is_empty());
        assert_eq!(
            account_response(0, &settings, None)["identityCapability"]["supportsAccountSwitching"],
            false
        );
    }

    #[test]
    fn add_rename_and_remove_only_metadata_and_group_assignment() {
        let mut settings = sample();
        apply_account_change(
            &mut settings,
            &RuntimeAgyAccountsAction::Upsert {
                account_id: "research".into(),
                alias: "  Research  ".into(),
                expected_revision: 0,
            },
        )
        .unwrap();
        assert_eq!(settings.agy_profiles.len(), 1);
        assert_eq!(settings.agy_profiles[0].alias, "Research");
        settings
            .history_sync
            .account_groups
            .insert("agy:research".into(), "team".into());
        apply_account_change(
            &mut settings,
            &RuntimeAgyAccountsAction::Upsert {
                account_id: "research".into(),
                alias: "Personal".into(),
                expected_revision: 1,
            },
        )
        .unwrap();
        assert_eq!(settings.agy_profiles.len(), 1);
        assert_eq!(settings.agy_profiles[0].alias, "Personal");
        assert_eq!(settings.history_sync.account_groups["agy:research"], "team");
        apply_account_change(
            &mut settings,
            &RuntimeAgyAccountsAction::Remove {
                account_id: "research".into(),
                expected_revision: 2,
            },
        )
        .unwrap();
        assert!(settings.agy_profiles.is_empty());
        assert!(settings.history_sync.account_groups.is_empty());
        assert_eq!(
            settings.history_sync.mode,
            RuntimeAgentHistorySyncMode::Groups
        );
    }

    #[test]
    fn rejects_bad_ids_duplicates_and_credential_like_fields() {
        let mut settings = sample();
        for id in ["default", "", "../escape", "a:b", "a b"] {
            assert!(apply_account_change(
                &mut settings,
                &RuntimeAgyAccountsAction::Upsert {
                    account_id: id.into(),
                    alias: "Work".into(),
                    expected_revision: 0,
                }
            )
            .is_err());
        }
        apply_account_change(
            &mut settings,
            &RuntimeAgyAccountsAction::Upsert {
                account_id: "work".into(),
                alias: "Work".into(),
                expected_revision: 0,
            },
        )
        .unwrap();
        assert!(apply_account_change(
            &mut settings,
            &RuntimeAgyAccountsAction::Upsert {
                account_id: "another".into(),
                alias: "Work".into(),
                expected_revision: 0,
            }
        )
        .is_err());
        assert!(serde_json::from_value::<RuntimeAgyQuotaProfile>(json!({
            "alias": "Unsafe", "accountId": "unsafe", "token": "should-not-be-saved"
        }))
        .is_err());
    }
}
