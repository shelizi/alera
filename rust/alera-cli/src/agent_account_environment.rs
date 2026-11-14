use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

/// An AGY registration label is not an independent Windows keyring login.
/// Strip this Alera-only hint from every shell and refuse an unverified
/// non-default selection for managed AGY Agent tabs. Generic shells and
/// other Agent providers are unaffected.
pub(crate) fn validate_agy_launch_account(
    environment: &mut BTreeMap<String, String>,
    agent_type: Option<&str>,
) -> Result<()> {
    let selected = environment.remove("ALERA_ACCOUNT_AGY_ID");
    if matches!(agent_type, Some("agy" | "antigravity"))
        && selected.as_deref().is_some_and(|id| id != "default")
    {
        return Err(anyhow!(
            "Selected Google / Antigravity account has no verified isolated login. Select Default in Settings, or use a separately authenticated OS-user Runtime Host."
        ));
    }
    Ok(())
}

pub(crate) async fn claude_profile_directory(profile: &str) -> Result<PathBuf> {
    let path = Path::new(profile);
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    if profile.is_empty() || profile == "." || profile == ".." || profile.contains(['/', '\\']) {
        return Err(anyhow!(
            "Use a CCS instance name or an absolute Claude config directory"
        ));
    }
    let root = match crate::login_shell_environment::login_shell_variable("CCS_DIR").await {
        Some(root) => PathBuf::from(root),
        None => {
            let home = crate::login_shell_environment::login_shell_variable(if cfg!(windows) {
                "USERPROFILE"
            } else {
                "HOME"
            })
            .await
            .ok_or_else(|| anyhow!("Home directory is unavailable"))?;
            PathBuf::from(home).join(".ccs")
        }
    };
    Ok(root.join("instances").join(profile))
}

pub(crate) async fn resolve_account_launch_environment(
    environment: &mut BTreeMap<String, String>,
) -> Result<()> {
    if let Some(home) = environment.get("ALERA_ACCOUNT_CODEX_HOME") {
        if !Path::new(home).is_absolute() {
            return Err(anyhow!("Codex home must be an absolute path"));
        }
    }
    if let Some(profile) = environment.remove("ALERA_ACCOUNT_CLAUDE_PROFILE") {
        let directory = claude_profile_directory(&profile).await?;
        environment.insert(
            "ALERA_ACCOUNT_CLAUDE_CONFIG_DIR".to_string(),
            directory.to_string_lossy().into_owned(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_agy_account_cannot_launch_under_default_windows_login() {
        for agent_type in ["agy", "antigravity"] {
            let mut environment = BTreeMap::from([
                ("ALERA_ACCOUNT_AGY_ID".to_string(), "research".to_string()),
                ("PATH".to_string(), "unchanged".to_string()),
            ]);
            let result = validate_agy_launch_account(&mut environment, Some(agent_type));
            assert!(result.is_err());
            assert!(!environment.contains_key("ALERA_ACCOUNT_AGY_ID"));
            assert_eq!(
                environment.get("PATH").map(String::as_str),
                Some("unchanged")
            );
        }
    }

    #[test]
    fn default_agy_and_unrelated_terminal_unchanged() {
        for agent_type in [None, Some("codex"), Some("claude"), Some("agy")] {
            let selected = if matches!(agent_type, Some("agy")) {
                "default"
            } else {
                "research"
            };
            let mut environment =
                BTreeMap::from([("ALERA_ACCOUNT_AGY_ID".to_string(), selected.to_string())]);
            validate_agy_launch_account(&mut environment, agent_type).unwrap();
            assert!(environment.is_empty());
        }
    }

    #[tokio::test]
    async fn explicit_claude_directory_and_codex_home_stay_scoped() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().to_string_lossy().into_owned();
        let mut env = BTreeMap::from([
            ("ALERA_ACCOUNT_CODEX_HOME".into(), path.clone()),
            ("ALERA_ACCOUNT_CLAUDE_PROFILE".into(), path.clone()),
        ]);
        resolve_account_launch_environment(&mut env).await.unwrap();
        assert_eq!(env.get("ALERA_ACCOUNT_CLAUDE_CONFIG_DIR"), Some(&path));
        assert_eq!(env.get("ALERA_ACCOUNT_CODEX_HOME"), Some(&path));
        assert!(!env.contains_key("ALERA_ACCOUNT_CLAUDE_PROFILE"));
        assert_eq!(
            claude_profile_directory(&path).await.unwrap(),
            directory.path()
        );
    }

    #[tokio::test]
    async fn invalid_account_paths_are_rejected() {
        for profile in ["", ".", "..", "../other", "work/other", "work\\other"] {
            assert!(claude_profile_directory(profile).await.is_err());
        }
        let mut env = BTreeMap::from([("ALERA_ACCOUNT_CODEX_HOME".into(), "relative".into())]);
        assert!(resolve_account_launch_environment(&mut env).await.is_err());
    }
}
