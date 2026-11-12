//! Narrow no-start Runtime Host history-sync settings operations for GPUI.
//! Host is authoritative; native settings JSON and provider credentials are untouched.

use alera_core::runtime::{RuntimeAgentHistorySyncMode, RuntimeAgentQuotaSettings};
use serde::Serialize;
use serde_json::{json, Value};

use crate::agent_history_snapshots::summarize_private_versions;
use crate::cli::{RuntimeDirArgs, RuntimeHistorySyncAction, RuntimeHistorySyncModeArg};
use crate::runtime_host_client::RuntimeHostRpcClient;
use crate::{print_error, print_value, runtime_dir};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct HistoryAccount {
    provider: &'static str,
    account_id: String,
    label: String,
    /// AGY per-account native history remains unverified and is not copied yet.
    supported: bool,
}

fn accounts(settings: &RuntimeAgentQuotaSettings) -> Vec<HistoryAccount> {
    let mut result = vec![
        HistoryAccount {
            provider: "codex",
            account_id: "default".into(),
            label: "Codex Default".into(),
            supported: true,
        },
        HistoryAccount {
            provider: "claude",
            account_id: "default".into(),
            label: "Claude Default".into(),
            supported: true,
        },
        HistoryAccount {
            provider: "agy",
            account_id: "default".into(),
            label: "Antigravity Default".into(),
            supported: false,
        },
    ];
    for item in &settings.codex_profiles {
        result.push(HistoryAccount {
            provider: "codex",
            account_id: item.profile.clone(),
            label: format!("Codex · {}", item.alias),
            supported: true,
        });
    }
    for item in &settings.claude_profiles {
        result.push(HistoryAccount {
            provider: "claude",
            account_id: item.profile.clone(),
            label: format!("Claude · {}", item.alias),
            supported: true,
        });
    }
    result
}

fn decode_snapshot(value: &Value) -> Result<(i64, RuntimeAgentQuotaSettings), &'static str> {
    let revision = value
        .get("revision")
        .and_then(Value::as_i64)
        .filter(|revision| *revision >= 0)
        .ok_or("Host did not supply a settings revision")?;
    let quotas = value
        .get("agentQuotas")
        .ok_or("Host quota settings unavailable")?;
    if quotas.get("historySync").is_none() {
        return Err("Runtime Host does not support shared history settings; update the Host");
    }
    let settings: RuntimeAgentQuotaSettings =
        serde_json::from_value(quotas.clone()).map_err(|_| "Host quota settings are invalid")?;
    Ok((revision, settings))
}

fn set_account_group(
    settings: &mut RuntimeAgentQuotaSettings,
    catalog: &[HistoryAccount],
    provider: &str,
    account: &str,
    group: &str,
) -> Result<(), &'static str> {
    if !catalog
        .iter()
        .any(|candidate| candidate.provider == provider && candidate.account_id == account)
    {
        return Err("Unknown Agent account. Refresh settings before editing");
    }
    let group = group.trim();
    if group.len() > 64 || group.chars().any(char::is_control) {
        return Err("History sync group must be 64 bytes or fewer without control characters");
    }
    let key = format!("{provider}:{account}");
    if group.is_empty() {
        settings.history_sync.account_groups.remove(&key);
    } else {
        settings
            .history_sync
            .account_groups
            .insert(key, group.to_owned());
    }
    settings.history_sync.normalize();
    Ok(())
}

fn mode_from_argument(value: RuntimeHistorySyncModeArg) -> RuntimeAgentHistorySyncMode {
    match value {
        RuntimeHistorySyncModeArg::All => RuntimeAgentHistorySyncMode::All,
        RuntimeHistorySyncModeArg::Separate => RuntimeAgentHistorySyncMode::Separate,
        RuntimeHistorySyncModeArg::Groups => RuntimeAgentHistorySyncMode::Groups,
    }
}

pub(crate) async fn run(
    runtime: &RuntimeDirArgs,
    json_output: bool,
    action: RuntimeHistorySyncAction,
) -> i32 {
    // connect() never starts or restarts a Host; GPUI cannot secretly create one.
    let mut client = match RuntimeHostRpcClient::connect(&runtime_dir(runtime)).await {
        Ok(Some(client)) => client,
        Ok(None) => return print_error("Existing Runtime Host is offline"),
        Err(error) => return print_error(error),
    };
    let snapshot = match client
        .request_value_with_deadline("runtimeSettings.get", &json!({}), 5500)
        .await
    {
        Ok(value) => value,
        Err(error) => return print_error(error),
    };
    let (revision, mut settings) = match decode_snapshot(&snapshot) {
        Ok(value) => value,
        Err(error) => return print_error(error),
    };
    let catalog = accounts(&settings);
    let archive_summary = summarize_private_versions(
        &runtime_dir(runtime).join("agent-history-snapshots"),
    )
    .ok()
    .map(|info| json!({"sessions": info.sessions, "versions": info.versions, "truncated": info.truncated}));
    if let RuntimeHistorySyncAction::Get = action {
        print_value(
            &json!({
                "revision": revision,
                "historySync": settings.history_sync,
                "accounts": catalog,
                "archiveSummary": archive_summary,
            }),
            json_output,
            "Runtime Host shared history policy loaded",
        );
        return 0;
    }
    let expected = match &action {
        RuntimeHistorySyncAction::Get => unreachable!(),
        RuntimeHistorySyncAction::SetMode {
            expected_revision, ..
        }
        | RuntimeHistorySyncAction::SetGroup {
            expected_revision, ..
        } => *expected_revision,
    };
    if expected < 0 || expected != revision {
        return print_error("Runtime history sync settings changed; refresh and retry");
    }
    match action {
        RuntimeHistorySyncAction::Get => unreachable!(),
        RuntimeHistorySyncAction::SetMode { mode, .. } => {
            settings.history_sync.mode = mode_from_argument(mode);
        }
        RuntimeHistorySyncAction::SetGroup {
            provider,
            account_id,
            group_name,
            ..
        } => {
            if let Err(error) =
                set_account_group(&mut settings, &catalog, &provider, &account_id, &group_name)
            {
                return print_error(error);
            }
        }
    }
    let wanted = match serde_json::to_value(&settings.history_sync) {
        Ok(value) => value,
        Err(error) => return print_error(error),
    };
    let response = match client
        .request_value_with_deadline(
            "runtimeSettings.update",
            &json!({"expectedRevision": expected, "agentQuotas": settings}),
            6500,
        )
        .await
    {
        Ok(value) => value,
        Err(error) => return print_error(error),
    };
    if response.pointer("/agentQuotas/historySync") != Some(&wanted) {
        return print_error("Host did not confirm history sync policy; settings not verified");
    }
    print_value(
        &json!({
            "revision": response["revision"],
            "historySync": wanted,
            "accounts": catalog,
            "archiveSummary": archive_summary,
        }),
        json_output,
        "Runtime Host history sync settings saved",
    );
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_host_without_history_sync_fails_closed() {
        assert!(decode_snapshot(&json!({
            "revision": 3, "agentQuotas": {"enabledProviders":["codex"]}
        }))
        .is_err());
        let (revision, settings) = decode_snapshot(&json!({
            "revision": 5, "agentQuotas": {"historySync": {"mode": "groups"}}
        }))
        .unwrap();
        assert_eq!(revision, 5);
        assert_eq!(
            settings.history_sync.mode,
            RuntimeAgentHistorySyncMode::Groups
        );
    }

    #[test]
    fn group_selection_only_edits_configured_same_provider_account() {
        let mut settings = RuntimeAgentQuotaSettings::default();
        let catalog = accounts(&settings);
        assert_eq!(
            catalog.iter().filter(|item| item.provider == "agy").count(),
            1
        );
        assert!(set_account_group(&mut settings, &catalog, "codex", "default", "team").is_ok());
        assert!(set_account_group(&mut settings, &catalog, "codex", "unknown", "team").is_err());
        assert!(set_account_group(&mut settings, &catalog, "grok", "default", "team").is_err());
        assert!(
            set_account_group(&mut settings, &catalog, "codex", "default", "a\nsecret").is_err()
        );
        assert_eq!(
            settings.history_sync.account_groups["codex:default"],
            "team"
        );
        assert!(set_account_group(&mut settings, &catalog, "codex", "default", "").is_ok());
        assert!(settings.history_sync.account_groups.is_empty());
    }
}
