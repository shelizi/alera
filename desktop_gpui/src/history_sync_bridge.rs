//! Native GPUI -> authenticated Alera CLI -> existing Runtime Host.
//! Never shares Native's settings JSON with Classic; the Host owns this policy.
use crate::runtime_bridge::{wait_with_timeout, RuntimeCliBridge};
use alera_core::child_process::windowless_command;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{self, Read};
use std::process::Stdio;
use std::thread;
use std::time::Duration;

const RESPONSE_LIMIT: u64 = 256 * 1024;
const ERROR_LIMIT: u64 = 1024;
const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HistorySyncMode {
    All,
    Separate,
    Groups,
}
impl HistorySyncMode {
    fn cli_value(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Separate => "separate",
            Self::Groups => "groups",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySyncPolicy {
    pub mode: HistorySyncMode,
    #[serde(default)]
    pub account_groups: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySyncAccount {
    pub provider: String,
    pub account_id: String,
    pub label: String,
    /// AGY can be configured, but its native history copying is not yet supported.
    pub supported: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoryArchiveSummary {
    pub sessions: u64,
    pub versions: u64,
    pub truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistorySyncSnapshot {
    pub revision: i64,
    pub history_sync: HistorySyncPolicy,
    pub accounts: Vec<HistorySyncAccount>,
    #[serde(default)]
    pub archive_summary: Option<HistoryArchiveSummary>,
}

impl HistorySyncSnapshot {
    pub fn group_for(&self, account: &HistorySyncAccount) -> &str {
        self.history_sync
            .account_groups
            .get(&format!("{}:{}", account.provider, account.account_id))
            .map(String::as_str)
            .unwrap_or("")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgyAccountRegistration {
    pub account_id: String,
    pub alias: String,
    #[serde(default)]
    pub ssh_target_id: Option<String>,
    #[serde(default)]
    pub host_identity_hash: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgyIdentityCapability {
    pub credential_scope: String,
    pub account_authentication: String,
    pub host_os_user_proof: String,
    pub supports_independent_local_accounts: bool,
    pub supports_account_switching: bool,
    #[serde(default)]
    pub supports_isolated_ssh_routing: bool,
    pub recommended_isolation: String,
    pub can_register_names: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgyHostOsIdentity {
    pub kind: String,
    pub subject_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgyAccountsSnapshot {
    pub revision: i64,
    pub agy_accounts: Vec<AgyAccountRegistration>,
    pub host_os_identity: Option<AgyHostOsIdentity>,
    pub identity_capability: AgyIdentityCapability,
}

impl AgyAccountsSnapshot {
    fn validate(&self) -> io::Result<()> {
        let mut ids = std::collections::HashSet::new();
        let mut names = std::collections::HashSet::new();
        let identity = &self.identity_capability;
        let host_proof_valid = match &self.host_os_identity {
            Some(host_identity) => {
                matches!(
                    host_identity.kind.as_str(),
                    "windows-token-sid" | "unix-effective-uid"
                ) && host_identity.subject_hash.len() == 64
                    && host_identity
                        .subject_hash
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit())
                    && identity.host_os_user_proof == "os-token-digest"
            }
            None => identity.host_os_user_proof == "unavailable",
        };
        if self.revision < 0
            || self.agy_accounts.len() > 32
            || !host_proof_valid
            || identity.credential_scope != "host-os-user"
            || identity.account_authentication != "unverified"
            || identity.supports_account_switching
            || identity.supports_independent_local_accounts
            || !identity.can_register_names
            || self.agy_accounts.iter().any(|account| {
                !valid_agy_account_id(&account.account_id)
                    || account.alias.trim() != account.alias
                    || account.alias.chars().count() > 80
                    || account.alias.is_empty()
                    || account.alias.chars().any(char::is_control)
                    || !ids.insert(account.account_id.as_str())
                    || !names.insert(account.alias.as_str())
                    || account.ssh_target_id.is_some() != account.host_identity_hash.is_some()
                    || account.ssh_target_id.as_ref().is_some_and(|id| {
                        id.is_empty()
                            || id.len() > 128
                            || !id
                                .bytes()
                                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
                    })
                    || account.host_identity_hash.as_ref().is_some_and(|hash| {
                        hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit())
                    })
            })
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Runtime Host returned invalid Antigravity account capabilities",
            ));
        }
        Ok(())
    }
}

pub fn valid_agy_account_id(value: &str) -> bool {
    value != "default"
        && !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
}

impl RuntimeCliBridge {
    pub fn read_agy_accounts(&self) -> io::Result<AgyAccountsSnapshot> {
        self.agy_accounts_request(&[OsString::from("get")])
    }

    pub fn upsert_agy_account(
        &self,
        revision: i64,
        account_id: &str,
        alias: &str,
    ) -> io::Result<AgyAccountsSnapshot> {
        if revision < 0
            || !valid_agy_account_id(account_id)
            || alias.trim() != alias
            || alias.is_empty()
            || alias.chars().count() > 80
            || alias.chars().any(char::is_control)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid Antigravity registration",
            ));
        }
        self.agy_accounts_request(&[
            OsString::from("upsert"),
            OsString::from("--account-id"),
            OsString::from(account_id),
            OsString::from("--alias"),
            OsString::from(alias),
            OsString::from("--expected-revision"),
            OsString::from(revision.to_string()),
        ])
    }

    pub fn remove_agy_account(
        &self,
        revision: i64,
        account_id: &str,
    ) -> io::Result<AgyAccountsSnapshot> {
        if revision < 0 || !valid_agy_account_id(account_id) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid Antigravity account ID",
            ));
        }
        self.agy_accounts_request(&[
            OsString::from("remove"),
            OsString::from("--account-id"),
            OsString::from(account_id),
            OsString::from("--expected-revision"),
            OsString::from(revision.to_string()),
        ])
    }

    fn agy_accounts_request(&self, action: &[OsString]) -> io::Result<AgyAccountsSnapshot> {
        let snapshot: AgyAccountsSnapshot =
            self.runtime_settings_request("agy-accounts", action)?;
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub fn read_history_sync(&self) -> io::Result<HistorySyncSnapshot> {
        self.history_sync_request(&[OsString::from("get")])
    }

    pub fn update_history_sync_mode(
        &self,
        revision: i64,
        mode: HistorySyncMode,
    ) -> io::Result<HistorySyncSnapshot> {
        if revision < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid Host revision",
            ));
        }
        self.history_sync_request(&[
            OsString::from("set-mode"),
            OsString::from("--mode"),
            OsString::from(mode.cli_value()),
            OsString::from("--expected-revision"),
            OsString::from(revision.to_string()),
        ])
    }

    pub fn update_history_sync_group(
        &self,
        revision: i64,
        account: &HistorySyncAccount,
        group: &str,
    ) -> io::Result<HistorySyncSnapshot> {
        if revision < 0
            || !matches!(account.provider.as_str(), "codex" | "claude" | "agy")
            || account.account_id.is_empty()
            || account.account_id.len() > 4096
            || group.len() > 64
            || group.chars().any(char::is_control)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid history sync account or group",
            ));
        }
        self.history_sync_request(&[
            OsString::from("set-group"),
            OsString::from("--provider"),
            OsString::from(&account.provider),
            OsString::from("--account-id"),
            OsString::from(&account.account_id),
            OsString::from("--group-name"),
            OsString::from(group),
            OsString::from("--expected-revision"),
            OsString::from(revision.to_string()),
        ])
    }

    fn history_sync_request(&self, action: &[OsString]) -> io::Result<HistorySyncSnapshot> {
        let snapshot: HistorySyncSnapshot =
            self.runtime_settings_request("history-sync", action)?;
        if snapshot.revision < 0 || snapshot.accounts.len() > 256 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid history sync response",
            ));
        }
        Ok(snapshot)
    }

    /// Shared bounded no-start JSON transport for Host-owned settings.
    fn runtime_settings_request<T: DeserializeOwned>(
        &self,
        command: &str,
        action: &[OsString],
    ) -> io::Result<T> {
        let (executable, runtime_dir) = self.agent_cli_paths();
        if !runtime_dir.join("host.json").is_file()
            && !runtime_dir.join("runtime-host.json").is_file()
        {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "Existing Runtime Host is offline",
            ));
        }
        let mut process = windowless_command(executable);
        process
            .arg("runtime")
            .arg("--runtime-dir")
            .arg(runtime_dir)
            .arg("--json")
            .arg(command)
            .args(action)
            .env_remove("ALERA_WORKSPACE_ID")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = process.spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("CLI output unavailable"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| io::Error::other("CLI error channel unavailable"))?;
        let out_reader = thread::spawn(move || -> io::Result<Vec<u8>> {
            let mut bytes = Vec::new();
            stdout.take(RESPONSE_LIMIT + 1).read_to_end(&mut bytes)?;
            Ok(bytes)
        });
        let err_reader = thread::spawn(move || -> io::Result<Vec<u8>> {
            let mut bytes = Vec::new();
            stderr.take(ERROR_LIMIT + 1).read_to_end(&mut bytes)?;
            Ok(bytes)
        });
        let status = wait_with_timeout(&mut child, TIMEOUT);
        let output = out_reader
            .join()
            .map_err(|_| io::Error::other("CLI output reader panicked"))??;
        let error = err_reader
            .join()
            .map_err(|_| io::Error::other("CLI error reader panicked"))??;
        let status = status?;
        if !status.success() {
            // CLI error strings are deliberately narrow; never echo authentication material.
            let message = String::from_utf8_lossy(&error);
            let error = message
                .lines()
                .next()
                .unwrap_or("Runtime Host rejected history sync change");
            let error = error.chars().take(240).collect::<String>();
            return Err(io::Error::other(error));
        }
        if output.len() as u64 > RESPONSE_LIMIT {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Host history sync response too large",
            ));
        }
        let snapshot: T = serde_json::from_slice(&output)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        Ok(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn agy_snapshot_reports_explicit_unverified_keyring_and_rejects_fake_switching() {
        let valid = json!({
            "revision": 7,
            "agyAccounts": [{"accountId": "research", "alias": "Research"}],
            "hostOsIdentity": {"kind": "windows-token-sid", "subjectHash": "a".repeat(64)},
            "identityCapability": {
                "credentialScope": "host-os-user",
                "accountAuthentication": "unverified",
                "hostOsUserProof": "os-token-digest",
                "supportsIndependentLocalAccounts": false,
                "supportsAccountSwitching": false,
                "recommendedIsolation": "separate-os-user-runtime-host",
                "canRegisterNames": true
            }
        });
        let snapshot: AgyAccountsSnapshot = serde_json::from_value(valid.clone()).unwrap();
        snapshot.validate().unwrap();
        assert!(!snapshot.identity_capability.supports_account_switching);
        let mut fake = valid.clone();
        fake["identityCapability"]["supportsAccountSwitching"] = json!(true);
        assert!(serde_json::from_value::<AgyAccountsSnapshot>(fake)
            .unwrap()
            .validate()
            .is_err());
        let mut forged_home = valid.clone();
        forged_home["hostOsIdentity"]["kind"] = json!("userprofile-env");
        assert!(serde_json::from_value::<AgyAccountsSnapshot>(forged_home)
            .unwrap()
            .validate()
            .is_err());
        let mut fake_id = valid;
        fake_id["agyAccounts"][0]["accountId"] = json!("../escape");
        assert!(serde_json::from_value::<AgyAccountsSnapshot>(fake_id)
            .unwrap()
            .validate()
            .is_err());
    }

    #[test]
    fn native_agy_account_requires_complete_ssh_identity_binding() {
        let baseline = json!({
            "revision": 2,
            "agyAccounts": [{
                "accountId": "work",
                "alias": "Work",
                "sshTargetId": "agy2-ssh",
                "hostIdentityHash": "f".repeat(64)
            }],
            "hostOsIdentity": {"kind": "windows-token-sid", "subjectHash": "a".repeat(64)},
            "identityCapability": {
                "credentialScope": "host-os-user",
                "accountAuthentication": "unverified",
                "hostOsUserProof": "os-token-digest",
                "supportsIndependentLocalAccounts": false,
                "supportsAccountSwitching": false,
                "supportsIsolatedSshRouting": true,
                "recommendedIsolation": "separate-os-user-runtime-host",
                "canRegisterNames": true
            }
        });
        let paired: AgyAccountsSnapshot = serde_json::from_value(baseline.clone()).unwrap();
        paired.validate().unwrap();
        assert!(paired.identity_capability.supports_isolated_ssh_routing);
        let mut partial = baseline.clone();
        partial["agyAccounts"][0]
            .as_object_mut()
            .unwrap()
            .remove("hostIdentityHash");
        assert!(serde_json::from_value::<AgyAccountsSnapshot>(partial)
            .unwrap()
            .validate()
            .is_err());
        let mut forged_path = baseline;
        forged_path["agyAccounts"][0]["sshTargetId"] = json!("../otheruser");
        assert!(serde_json::from_value::<AgyAccountsSnapshot>(forged_path)
            .unwrap()
            .validate()
            .is_err());
    }

    #[test]
    fn agy_offline_registration_never_starts_host_or_accepts_bad_ids() {
        let temp = tempfile::tempdir().unwrap();
        let bridge = RuntimeCliBridge::new(temp.path().join("missing-cli"), temp.path());
        assert_eq!(
            bridge.read_agy_accounts().unwrap_err().kind(),
            io::ErrorKind::NotConnected
        );
        assert_eq!(
            bridge.remove_agy_account(0, "default").unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            bridge
                .upsert_agy_account(0, "../escape", "Research")
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            bridge
                .upsert_agy_account(0, "research", "Foo\nBar")
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(!temp.path().join("host.json").exists());
    }

    #[test]
    fn reads_shared_host_policy_and_scopes_each_account_group() {
        let snapshot: HistorySyncSnapshot = serde_json::from_value(json!({
            "revision": 11,
            "historySync": {"mode": "groups", "accountGroups": {
                "codex:default": "team", "claude:default": "personal"
            }},
            "accounts": [
                {"provider":"codex","accountId":"default","label":"Codex Default","supported":true},
                {"provider":"claude","accountId":"default","label":"Claude Default","supported":true}
            ],
            "archiveSummary":{"sessions":1,"versions":2,"truncated":false}
        })).unwrap();
        assert_eq!(snapshot.history_sync.mode, HistorySyncMode::Groups);
        assert_eq!(snapshot.group_for(&snapshot.accounts[0]), "team");
        assert_eq!(snapshot.group_for(&snapshot.accounts[1]), "personal");
        assert_eq!(snapshot.archive_summary.as_ref().unwrap().versions, 2);
        assert!(serde_json::from_value::<HistorySyncSnapshot>(json!({
            "revision": 1, "historySync": {"mode":"all"}, "accounts":[], "secret":"no"
        }))
        .is_err());
    }

    #[test]
    fn offline_host_never_starts_process_or_creates_history_dirs() {
        let temp = tempfile::tempdir().unwrap();
        let bridge = RuntimeCliBridge::new(temp.path().join("missing-cli"), temp.path());
        assert_eq!(
            bridge.read_history_sync().unwrap_err().kind(),
            io::ErrorKind::NotConnected
        );
        assert!(!temp.path().join("host.json").exists());
        let account = HistorySyncAccount {
            provider: "codex".into(),
            account_id: "default".into(),
            label: "Default".into(),
            supported: true,
        };
        assert!(bridge
            .update_history_sync_group(3, &account, "bad\nname")
            .is_err());
        assert!(bridge
            .update_history_sync_mode(-1, HistorySyncMode::All)
            .is_err());
    }
}
