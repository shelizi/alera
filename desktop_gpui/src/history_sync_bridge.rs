//! Native GPUI -> authenticated Alera CLI -> existing Runtime Host.
//! Never shares Native's settings JSON with Classic; the Host owns this policy.
use crate::runtime_bridge::{wait_with_timeout, RuntimeCliBridge};
use alera_core::child_process::windowless_command;
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
pub struct HistorySyncSnapshot {
    pub revision: i64,
    pub history_sync: HistorySyncPolicy,
    pub accounts: Vec<HistorySyncAccount>,
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

impl RuntimeCliBridge {
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
            .arg("history-sync")
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
        let snapshot: HistorySyncSnapshot = serde_json::from_slice(&output)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if snapshot.revision < 0 || snapshot.accounts.len() > 256 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid history sync response",
            ));
        }
        Ok(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
            ]
        })).unwrap();
        assert_eq!(snapshot.history_sync.mode, HistorySyncMode::Groups);
        assert_eq!(snapshot.group_for(&snapshot.accounts[0]), "team");
        assert_eq!(snapshot.group_for(&snapshot.accounts[1]), "personal");
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
