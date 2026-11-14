//! Explicit, identity-pinned, loopback SSH routing for a second Windows user.
//! SSH has a separate OS user token and Windows Credential Manager namespace.
//! No passwords, Google tokens, private keys or Alera full-host tokens cross
//! this boundary. The user's existing SSH known_hosts and public-key auth
//! enforce server identity; the remote Alera CLI proves the OS user identity.

use std::process::Stdio;
use std::time::Duration;

use alera_core::child_process::windowless_async_command;
use alera_core::runtime::{
    RuntimeAgentQuotaSettings, RuntimeAgyQuotaProfile, RuntimeStore, SshAuthKind, SshTarget,
};
use anyhow::{anyhow, bail, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde_json::{json, Value};
use tokio::time::timeout;

use crate::runtime_os_identity::{current_host_os_identity, HostOsIdentity};

const IDENTITY_DEADLINE: Duration = Duration::from_secs(12);
const QUOTA_DEADLINE: Duration = Duration::from_secs(72);
const MAX_OUTPUT: usize = 96 * 1024;

#[derive(Debug, Clone)]
pub(crate) struct VerifiedAgySshRoute {
    pub target: SshTarget,
    pub host_identity: HostOsIdentity,
}

fn safe_loopback_ssh_target(target: &SshTarget) -> Result<()> {
    if !matches!(target.host.as_str(), "127.0.0.1" | "::1")
        || !(1..=65535).contains(&target.port)
        || !matches!(target.auth_kind, SshAuthKind::Key | SshAuthKind::Agent)
        || target.username.is_empty()
        || target.username.len() > 128
        || !target
            .username
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
        || target.username.starts_with('-')
    {
        bail!("AGY SSH routing needs a pinned loopback SSH target with public-key authentication");
    }
    Ok(())
}

fn ssh_args(target: &SshTarget, tty: bool) -> Result<Vec<String>> {
    safe_loopback_ssh_target(target)?;
    let mut args = vec![
        "-oBatchMode=yes".into(),
        "-oStrictHostKeyChecking=yes".into(),
        "-oForwardAgent=no".into(),
        "-oClearAllForwardings=yes".into(),
        "-oPermitLocalCommand=no".into(),
        "-oConnectTimeout=5".into(),
        "-oConnectionAttempts=1".into(),
        "-oPreferredAuthentications=publickey".into(),
        "-oLogLevel=ERROR".into(),
        "-p".into(),
        target.port.to_string(),
        if tty { "-tt" } else { "-T" }.into(),
        format!("{}@{}", target.username, target.host),
    ];
    if args.iter().any(|arg| arg.contains(['\r', '\n', '\0'])) {
        bail!("Invalid SSH routing argument");
    }
    Ok(std::mem::take(&mut args))
}

async fn ssh_run(target: &SshTarget, command: &'static str, deadline: Duration) -> Result<Value> {
    let mut args = ssh_args(target, false)?;
    args.push(command.to_owned());
    let child = windowless_async_command("ssh")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| anyhow!("OpenSSH client unavailable or could not start"))?;
    let output = match timeout(deadline, child.wait_with_output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(_)) => bail!("Loopback SSH command failed"),
        Err(_) => bail!("Loopback SSH command timed out"),
    };
    if !output.status.success() {
        bail!("Loopback SSH command failed (check known_hosts, key auth and the other user's Alera CLI)");
    }
    if output.stdout.len() > MAX_OUTPUT {
        bail!("Loopback SSH response exceeds the maximum size");
    }
    serde_json::from_slice::<Value>(&output.stdout)
        .map_err(|_| anyhow!("Invalid JSON from independently authenticated SSH account"))
}

pub(crate) async fn probe_ssh_os_identity(target: &SshTarget) -> Result<HostOsIdentity> {
    let result = ssh_run(
        target,
        "alera runtime --json os-identity",
        IDENTITY_DEADLINE,
    )
    .await?;
    let identity: HostOsIdentity = serde_json::from_value(result)
        .map_err(|_| anyhow!("Remote Alera CLI did not return an OS user identity"))?;
    if !identity.validated() {
        bail!("Remote Alera OS user identity is invalid");
    }
    if cfg!(windows) && identity.kind != "windows-token-sid" {
        bail!("Remote AGY account is not running under a separate Windows security token");
    }
    Ok(identity)
}

pub(crate) async fn probe_separate_ssh_target(
    target: &SshTarget,
    local_host_identity: &HostOsIdentity,
) -> Result<HostOsIdentity> {
    safe_loopback_ssh_target(target)?;
    if !local_host_identity.validated() {
        bail!("Local Host OS identity is not verified");
    }
    let remote = probe_ssh_os_identity(target).await?;
    if remote.kind != local_host_identity.kind
        || remote.definitely_same_os_user(local_host_identity)
    {
        bail!("SSH target is not a distinct Windows OS user/keyring");
    }
    Ok(remote)
}

pub(crate) async fn verify_ssh_route(
    store: &RuntimeStore,
    profile: &RuntimeAgyQuotaProfile,
) -> Result<VerifiedAgySshRoute> {
    let target_id = profile
        .ssh_target_id
        .as_deref()
        .ok_or_else(|| anyhow!("AGY account is not paired with a separate Windows user"))?;
    let pinned = profile
        .host_identity_hash
        .as_deref()
        .ok_or_else(|| anyhow!("AGY account is missing its pinned OS user identity"))?;
    if pinned.len() != 64 || !pinned.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("AGY account OS user pin is invalid");
    }
    let target = store
        .find_ssh_target(target_id)
        .await?
        .ok_or_else(|| anyhow!("Registered AGY SSH target no longer exists"))?;
    safe_loopback_ssh_target(&target)?;
    let observed = probe_ssh_os_identity(&target).await?;
    let local = current_host_os_identity()?;
    if observed.definitely_same_os_user(&local) {
        bail!("AGY SSH target uses the same OS user/keyring as the default account");
    }
    if observed.subject_hash != pinned {
        bail!("AGY SSH target OS user changed; unpair and verify again");
    }
    Ok(VerifiedAgySshRoute {
        target,
        host_identity: observed,
    })
}

/// Start a PowerShell in the SAME workspace path under the second Windows
/// user. Encode the fixed script rather than interpolate a workspace path
/// into any shell command line. If that user cannot access the repo, abort
/// instead of running AGY in the wrong project.
pub(crate) fn interactive_ssh_launch(
    route: &VerifiedAgySshRoute,
    working_directory: &str,
) -> Result<(String, Vec<String>)> {
    if !std::path::Path::new(working_directory).is_absolute()
        || working_directory.contains(['\r', '\n', '\0'])
        || working_directory.len() > 4096
    {
        bail!("AGY remote workspace must be an absolute, valid path");
    }
    let escaped = working_directory.replace("'", "''");
    let script = format!(
        "$ErrorActionPreference='Stop'; try {{ Set-Location -LiteralPath '{escaped}' }} catch {{ [Console]::Error.WriteLine('AGY workspace unavailable'); exit 73 }}"
    );
    let utf16le: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let encoded = STANDARD.encode(utf16le);
    let mut args = ssh_args(&route.target, true)?;
    args.push(format!(
        "powershell.exe -NoLogo -NoProfile -NoExit -EncodedCommand {encoded}"
    ));
    debug_assert!(route.host_identity.validated());
    Ok(("ssh".to_owned(), args))
}

pub(crate) async fn ssh_agy_quota(
    store: &RuntimeStore,
    profile: &RuntimeAgyQuotaProfile,
) -> Result<Value> {
    let route = verify_ssh_route(store, profile).await?;
    // The separately logged-in Windows user must have its own persistent
    // Alera Host and AGY login. No local Default quota is substituted.
    let response = ssh_run(
        &route.target,
        "alera quota --no-start --json snapshot --force-refresh --provider agy --account-id default",
        QUOTA_DEADLINE,
    )
    .await?;
    let rows = response
        .get("snapshots")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("Remote AGY Host returned no quota list"))?;
    if rows.len() > 16 {
        bail!("Remote AGY Host quota list exceeds limit");
    }
    let remote = rows
        .iter()
        .find(|row| {
            row.get("provider").and_then(Value::as_str) == Some("agy")
                && row.get("accountId").and_then(Value::as_str) == Some("default")
        })
        .ok_or_else(|| anyhow!("Remote AGY Host did not return its own default account"))?;
    let mut quota = remote.clone();
    quota["accountId"] = json!(profile.account_id);
    quota["displayName"] = json!(profile.alias);
    // Preserve the existing quota snapshot schema. Identity is validated
    // before the remote quota request and never exposed as a Google account.
    Ok(quota)
}

/// Replace only the AGY rows that are explicitly paired to a different
/// Windows OS user. On failure retain a fail-closed unavailable row, not a
/// cached default-user reading. Other providers are never touched.
pub(crate) async fn replace_verified_agy_ssh_quotas(
    store: &RuntimeStore,
    settings: &RuntimeAgentQuotaSettings,
    provider: Option<&str>,
    requested_account: Option<&str>,
    payload: &mut Value,
) {
    if provider.is_some_and(|p| !matches!(p, "agy" | "antigravity")) {
        return;
    }
    let Some(rows) = payload.get_mut("snapshots").and_then(Value::as_array_mut) else {
        return;
    };
    if !rows
        .iter()
        .any(|row| row.get("provider").and_then(Value::as_str) == Some("agy"))
    {
        return;
    }
    let mut tasks = tokio::task::JoinSet::new();
    let permits = std::sync::Arc::new(tokio::sync::Semaphore::new(2));
    for profile in &settings.agy_profiles {
        if requested_account.is_some_and(|id| id != profile.account_id)
            || profile.ssh_target_id.is_none()
            || profile.host_identity_hash.is_none()
        {
            continue;
        }
        let store = store.clone();
        let profile = profile.clone();
        let permits = permits.clone();
        tasks.spawn(async move {
            let _guard = permits.acquire_owned().await.ok();
            let fetched = ssh_agy_quota(&store, &profile).await;
            (profile, fetched)
        });
    }
    while let Some(Ok((profile, fetched))) = tasks.join_next().await {
        let Some(row) = rows.iter_mut().find(|row| {
            row.get("provider").and_then(Value::as_str) == Some("agy")
                && row.get("accountId").and_then(Value::as_str) == Some(profile.account_id.as_str())
        }) else {
            continue;
        };
        match fetched {
            Ok(remote) => {
                // Never label a failed remote login as verified quota.
                *row = remote;
                if row.get("status").and_then(Value::as_str) != Some("ok") {
                    row["dataQuality"] = json!("identity-unverified");
                }
            }
            Err(error) => {
                row["status"] = json!("unavailable");
                row["error"] = json!(format!("Isolated AGY SSH Host unavailable: {error}"));
                row["dataQuality"] = json!("identity-unverified");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alera_core::runtime::SshBootstrapStatus;
    use chrono::Utc;

    fn sample(host: &str, username: &str) -> SshTarget {
        SshTarget {
            id: "test-target".into(),
            alias: "Isolated AGY".into(),
            host: host.into(),
            port: 22,
            username: username.into(),
            platform: Some("windows".into()),
            arch: None,
            auth_kind: SshAuthKind::Key,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_status: None,
            install_dir: None,
            runtime_version: None,
            runtime_platform: None,
            runtime_arch: None,
            bootstrap_status: SshBootstrapStatus::NotInstalled,
            last_bootstrap_at: None,
            last_checked_at: None,
            last_error: None,
        }
    }

    #[test]
    fn only_strict_loopback_public_key_ssh_routes_are_allowed() {
        assert!(safe_loopback_ssh_target(&sample("127.0.0.1", "agyalice")).is_ok());
        for (host, user) in [
            ("evil.example", "agyalice"),
            ("127.0.0.2", "agyalice"),
            ("localhost", "agyalice"),
            ("127.0.0.1", "-bad"),
            ("127.0.0.1", "test;calc"),
            ("127.0.0.1", "bad user"),
        ] {
            assert!(safe_loopback_ssh_target(&sample(host, user)).is_err());
        }
        let args = ssh_args(&sample("127.0.0.1", "agyalice"), true).unwrap();
        assert!(args.contains(&"-oStrictHostKeyChecking=yes".to_string()));
        assert!(args.contains(&"-oBatchMode=yes".to_string()));
        assert!(args.contains(&"-tt".to_string()));
        assert!(args.contains(&"agyalice@127.0.0.1".to_string()));
        let mut password = sample("127.0.0.1", "agyalice");
        password.auth_kind = SshAuthKind::Password;
        assert!(ssh_args(&password, false).is_err());
    }
}
