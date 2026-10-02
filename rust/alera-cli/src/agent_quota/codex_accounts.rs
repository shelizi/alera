async fn fetch_codex_profile(profile: &CodexProfileRequest) -> QuotaSnapshot {
    let home = PathBuf::from(&profile.profile);
    if !home.is_absolute() {
        return QuotaSnapshot::error(
            "codex",
            &profile.profile,
            &profile.alias,
            "Codex home must be an absolute path",
        );
    }
    let mut snapshot = fetch_codex_in_home(Some(&home)).await;
    snapshot.account_id = profile.profile.clone();
    snapshot.display_name = profile.alias.clone();
    // The reset mutation currently resolves only the default auth context.
    if let Some(credits) = snapshot.rate_limit_reset_credits.as_mut() {
        credits.can_consume = false;
    }
    snapshot
}

async fn read_codex_backend_auth() -> Result<Option<CodexBackendAuth>> {
    read_codex_auth_in_home(None).await
}

async fn read_codex_auth_in_home(
    configured_home: Option<&std::path::Path>,
) -> Result<Option<CodexBackendAuth>> {
    let home = match configured_home {
        Some(home) => Some(home.to_path_buf()),
        None => shell_environment_value("CODEX_HOME")
            .await
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|home| home.join(".codex"))),
    };
    let Some(home) = home else {
        return Ok(None);
    };
    let contents = match tokio::fs::read_to_string(home.join("auth.json")).await {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("Could not read Codex auth.json"),
    };
    let raw: Value =
        serde_json::from_str(&contents).context("Codex auth.json is not valid JSON")?;
    let Some(access_token) = raw
        .pointer("/tokens/access_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let account_id = raw
        .pointer("/tokens/account_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    Ok(Some(CodexBackendAuth {
        access_token: access_token.to_string(),
        account_id,
    }))
}

#[cfg(test)]
mod codex_account_tests {
    use super::*;

    #[tokio::test]
    async fn codex_profile_credentials_never_fall_back_to_another_home() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        for (home, token, account) in [
            (&first, "first-token", "first-account"),
            (&second, "second-token", "second-account"),
        ] {
            std::fs::write(
                home.path().join("auth.json"),
                serde_json::to_vec(&json!({
                    "tokens": {"access_token": token, "account_id": account}
                }))
                .unwrap(),
            )
            .unwrap();
        }
        let auth = read_codex_auth_in_home(Some(first.path()))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(auth.access_token, "first-token");
        assert_eq!(auth.account_id.as_deref(), Some("first-account"));
        let auth = read_codex_auth_in_home(Some(second.path()))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(auth.access_token, "second-token");
        std::fs::remove_file(second.path().join("auth.json")).unwrap();
        assert!(read_codex_auth_in_home(Some(second.path()))
            .await
            .unwrap()
            .is_none());
    }
}
