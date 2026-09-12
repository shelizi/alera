const DEVIN_USER_STATUS_PATH: &str =
    "/exa.seat_management_pb.SeatManagementService/GetUserStatus";
const DEVIN_DAILY_WINDOW_MINUTES: i64 = 1_440;
const DEVIN_CLI_VERSION_TIMEOUT: Duration = Duration::from_secs(5);
/// The service parses version metadata and fails outright on values it cannot
/// parse, so an undetectable CLI still sends a parseable semver.
const DEVIN_CLI_VERSION_FALLBACK: &str = "1.0.0";

#[derive(Debug, Deserialize)]
struct DevinCredentialFile {
    windsurf_api_key: Option<String>,
    api_server_url: Option<String>,
}

struct DevinCredentials {
    api_key: String,
    api_server_url: String,
}

async fn fetch_devin() -> QuotaSnapshot {
    let credentials = match read_devin_credentials().await {
        Ok(Some(credentials)) => credentials,
        Ok(None) => {
            return QuotaSnapshot::unavailable(
                "devin",
                "default",
                "Devin",
                "Sign in with Devin to show daily and weekly quota.",
            );
        }
        Err(error) => {
            return QuotaSnapshot::error(
                "devin",
                "default",
                "Devin",
                format!("Could not read Devin credentials: {error}"),
            );
        }
    };

    let cli_version = devin_cli_version()
        .await
        .unwrap_or_else(|| DEVIN_CLI_VERSION_FALLBACK.to_string());

    let url = match devin_user_status_url(&credentials.api_server_url) {
        Ok(url) => url,
        Err(error) => {
            return QuotaSnapshot::error(
                "devin",
                "default",
                "Devin",
                error.to_string(),
            );
        }
    };

    let response = match reqwest::Client::new()
        .post(url)
        .timeout(FETCH_TIMEOUT)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .header("Connect-Protocol-Version", "1")
        .header("User-Agent", "alera/devin-quota")
        .json(&json!({
            "metadata": {
                "apiKey": credentials.api_key,
                "ideName": "devin",
                "ideVersion": &cli_version,
                "extensionName": "devin-cli",
                "extensionVersion": &cli_version,
                "locale": "en",
            }
        }))
        .send()
        .await
    {
        Ok(response) => response,
        Err(_) => {
            return QuotaSnapshot::error(
                "devin",
                "default",
                "Devin",
                "Devin user-status API is unavailable.",
            );
        }
    };

    if matches!(response.status().as_u16(), 401 | 403) {
        return QuotaSnapshot::unavailable(
            "devin",
            "default",
            "Devin",
            "Devin sign-in expired. Sign in with Devin again.",
        );
    }
    if !response.status().is_success() {
        return QuotaSnapshot::error(
            "devin",
            "default",
            "Devin",
            format!(
                "Devin user-status API returned HTTP {}.",
                response.status().as_u16()
            ),
        );
    }

    let payload = match response.json::<Value>().await {
        Ok(payload) => payload,
        Err(_) => {
            return QuotaSnapshot::error(
                "devin",
                "default",
                "Devin",
                "Devin user-status API returned invalid JSON.",
            );
        }
    };
    match parse_devin_quota_windows(&payload) {
        Ok(windows) => QuotaSnapshot::ok("devin", "default", "Devin", windows, Vec::new()),
        Err(error) => QuotaSnapshot::error("devin", "default", "Devin", error.to_string()),
    }
}

async fn read_devin_credentials() -> Result<Option<DevinCredentials>> {
    for path in devin_credentials_paths().await {
        let raw = match tokio::fs::read_to_string(&path).await {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error).context("credential file is unreadable"),
        };
        let file: DevinCredentialFile =
            toml::from_str(&raw).context("credential file contains invalid TOML")?;
        let api_key = file
            .windsurf_api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("credential file does not contain a local API key"))?;
        let api_server_url = file
            .api_server_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("credential file does not contain an API server"))?;
        return Ok(Some(DevinCredentials {
            api_key: api_key.to_string(),
            api_server_url: api_server_url.to_string(),
        }));
    }
    Ok(None)
}

/// The installed CLI's own version, reported in the request the same way the
/// CLI identifies itself to the seat-management service.
async fn devin_cli_version() -> Option<String> {
    for binary in devin_cli_binary_candidates() {
        let probe = alera_core::child_process::windowless_async_command(&binary)
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output();
        let output = match tokio::time::timeout(DEVIN_CLI_VERSION_TIMEOUT, probe).await {
            Ok(Ok(output)) if output.status.success() => output,
            _ => continue,
        };
        if let Some(version) = parse_devin_cli_version(&String::from_utf8_lossy(&output.stdout))
        {
            return Some(version);
        }
    }
    None
}

/// `devin 3000.6.14 (18033302)` -> `3000.6.14`.
fn parse_devin_cli_version(output: &str) -> Option<String> {
    let version = output.lines().next()?.split_whitespace().nth(1)?;
    let semver = version.contains('.')
        && version
            .split('.')
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()));
    semver.then(|| version.to_string())
}

/// PATH first, then the standard install root the CLI's own installer uses.
fn devin_cli_binary_candidates() -> Vec<PathBuf> {
    let binary = if cfg!(windows) { "devin.exe" } else { "devin" };
    let mut candidates = vec![PathBuf::from(binary)];
    for base in [dirs::data_local_dir(), dirs::data_dir()]
        .into_iter()
        .flatten()
    {
        candidates.push(base.join("devin/cli/bin").join(binary));
    }
    if let Some(home) = home_dir() {
        candidates.push(home.join(".local/share/devin/cli/bin").join(binary));
    }
    candidates
}

async fn devin_credentials_paths() -> Vec<PathBuf> {
    let xdg_data_home = shell_environment_value("XDG_DATA_HOME").await;
    let home = home_dir();
    devin_credentials_path_candidates(
        xdg_data_home.as_deref(),
        home.as_deref(),
        dirs::data_dir(),
        dirs::data_local_dir(),
    )
}

fn devin_credentials_path_candidates(
    xdg_data_home: Option<&str>,
    home: Option<&std::path::Path>,
    data_dir: Option<PathBuf>,
    data_local_dir: Option<PathBuf>,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(value) = xdg_data_home {
        let path = PathBuf::from(value.trim());
        if !path.as_os_str().is_empty() {
            paths.push(path.join("devin/credentials.toml"));
        }
    }
    // Platform data dirs cover Windows (%APPDATA%, %LOCALAPPDATA%) and macOS
    // (~/Library/Application Support); on Linux data_dir resolves to the same
    // ~/.local/share as the home fallback below and is deduplicated.
    for base in [data_dir, data_local_dir].into_iter().flatten() {
        paths.push(base.join("devin/credentials.toml"));
    }
    if let Some(home) = home {
        paths.push(home.join(".local/share/devin/credentials.toml"));
    }
    let mut unique = Vec::with_capacity(paths.len());
    for path in paths {
        if !unique.iter().any(|candidate| candidate == &path) {
            unique.push(path);
        }
    }
    unique
}

fn devin_user_status_url(server: &str) -> Result<reqwest::Url> {
    let mut url = reqwest::Url::parse(server.trim()).context("Invalid Devin API server URL")?;
    if url.scheme() != "https" || url.host_str().is_none() {
        return Err(anyhow!("Devin API server must be a valid HTTPS URL"));
    }
    let base_path = url.path().trim_end_matches('/');
    let path = format!("{base_path}{DEVIN_USER_STATUS_PATH}");
    url.set_path(&path);
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

fn parse_devin_quota_windows(payload: &Value) -> Result<Vec<QuotaWindow>> {
    let plan_status = payload
        .pointer("/userStatus/planStatus")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("Devin user-status response did not contain planStatus"))?;
    let mut windows = Vec::with_capacity(2);
    push_devin_quota_window(
        &mut windows,
        plan_status,
        "Daily",
        "dailyQuotaRemainingPercent",
        "dailyQuotaResetAtUnix",
        DEVIN_DAILY_WINDOW_MINUTES,
    );
    push_devin_quota_window(
        &mut windows,
        plan_status,
        "Weekly",
        "weeklyQuotaRemainingPercent",
        "weeklyQuotaResetAtUnix",
        WEEKLY_WINDOW_MINUTES,
    );
    if windows.is_empty() {
        return Err(anyhow!(
            "Devin user-status response did not contain daily or weekly quota"
        ));
    }
    Ok(windows)
}

fn push_devin_quota_window(
    windows: &mut Vec<QuotaWindow>,
    plan_status: &serde_json::Map<String, Value>,
    label: &str,
    remaining_key: &str,
    reset_key: &str,
    window_minutes: i64,
) {
    let Some(remaining_percent) = plan_status.get(remaining_key).and_then(numeric) else {
        return;
    };
    if !remaining_percent.is_finite() {
        return;
    }
    let resets_at = plan_status
        .get(reset_key)
        .and_then(numeric)
        .filter(|value| value.is_finite() && *value > 0.0)
        .map(|value| normalize_timestamp_millis(value.round() as i64));
    windows.push(QuotaWindow {
        label: label.to_string(),
        used_percent: (100.0 - remaining_percent).clamp(0.0, 100.0),
        window_minutes: Some(window_minutes),
        resets_at,
        reset_description: None,
    });
}

#[cfg(test)]
mod devin_tests {
    use super::*;

    #[test]
    fn parses_daily_and_weekly_remaining_quota() {
        let windows = parse_devin_quota_windows(&json!({
            "userStatus": {
                "planStatus": {
                    "dailyQuotaRemainingPercent": 75,
                    "dailyQuotaResetAtUnix": 1_800_000_000,
                    "weeklyQuotaRemainingPercent": "25",
                    "weeklyQuotaResetAtUnix": 1_900_000_000,
                }
            }
        }))
        .expect("Devin quota windows");

        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].label, "Daily");
        assert_eq!(windows[0].used_percent, 25.0);
        assert_eq!(windows[0].window_minutes, Some(1_440));
        assert_eq!(windows[0].resets_at, Some(1_800_000_000_000));
        assert_eq!(windows[1].label, "Weekly");
        assert_eq!(windows[1].used_percent, 75.0);
        assert_eq!(windows[1].window_minutes, Some(WEEKLY_WINDOW_MINUTES));
        assert_eq!(windows[1].resets_at, Some(1_900_000_000_000));
    }

    #[test]
    fn rejects_user_status_without_quota_windows() {
        let error = parse_devin_quota_windows(&json!({
            "userStatus": { "planStatus": { "availablePromptCredits": 42 } }
        }))
        .expect_err("missing quota must fail");

        assert!(error.to_string().contains("daily or weekly quota"));
    }

    #[test]
    fn builds_credentials_paths_from_xdg_platform_dirs_then_home() {
        let paths = devin_credentials_path_candidates(
            Some("C:/xdg-data"),
            Some(std::path::Path::new("C:/Users/test")),
            Some(PathBuf::from("C:/Users/test/AppData/Roaming")),
            Some(PathBuf::from("C:/Users/test/AppData/Local")),
        );

        assert_eq!(paths.len(), 4);
        assert!(paths[0].ends_with("devin/credentials.toml"));
        assert_eq!(
            paths[1],
            PathBuf::from("C:/Users/test/AppData/Roaming/devin/credentials.toml")
        );
        assert_eq!(
            paths[2],
            PathBuf::from("C:/Users/test/AppData/Local/devin/credentials.toml")
        );
        assert!(paths[3].ends_with(".local/share/devin/credentials.toml"));
    }

    #[test]
    fn deduplicates_data_dir_matching_home_fallback() {
        let paths = devin_credentials_path_candidates(
            None,
            Some(std::path::Path::new("/home/test")),
            Some(PathBuf::from("/home/test/.local/share")),
            Some(PathBuf::from("/home/test/.local/share")),
        );

        assert_eq!(
            paths,
            vec![PathBuf::from("/home/test/.local/share/devin/credentials.toml")]
        );
    }

    #[test]
    fn parses_cli_version_output() {
        assert_eq!(
            parse_devin_cli_version("devin 3000.6.14 (18033302)\n"),
            Some("3000.6.14".to_string())
        );
        assert_eq!(parse_devin_cli_version("devin unknown"), None);
        assert_eq!(parse_devin_cli_version(""), None);
        assert_eq!(parse_devin_cli_version("devin"), None);
    }

    #[test]
    fn requires_https_for_user_status_api() {
        assert!(devin_user_status_url("http://example.com").is_err());
        let url = devin_user_status_url("https://example.com/base/").expect("https url");
        assert_eq!(
            url.as_str(),
            "https://example.com/base/exa.seat_management_pb.SeatManagementService/GetUserStatus"
        );
    }
}
