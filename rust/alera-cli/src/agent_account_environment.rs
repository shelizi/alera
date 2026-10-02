use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

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
