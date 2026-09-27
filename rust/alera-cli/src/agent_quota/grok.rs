struct GrokAuth {
    issuer_key: String,
    access_token: String,
    user_id: Option<String>,
    refresh_token: Option<String>,
    oidc_issuer: Option<String>,
    oidc_client_id: Option<String>,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl GrokAuth {
    fn expired(&self) -> bool {
        self.expires_at.is_some_and(|expiry| {
            expiry <= chrono::Utc::now() + chrono::Duration::minutes(2)
        })
    }

    fn can_refresh(&self) -> bool {
        [&self.refresh_token, &self.oidc_issuer, &self.oidc_client_id]
            .iter()
            .all(|value| value.as_deref().is_some_and(|value| !value.trim().is_empty()))
    }
}

async fn fetch_grok() -> QuotaSnapshot {
    let Some(home) = home_dir() else {
        return QuotaSnapshot::unavailable(
            "grok",
            "default",
            "Grok Build",
            "Home directory is unavailable",
        );
    };
    let root = shell_environment_value("GROK_HOME")
        .await
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".grok"));
    let auth_path = root.join("auth.json");
    let raw = match tokio::fs::read_to_string(&auth_path).await {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return QuotaSnapshot::unavailable(
                "grok",
                "default",
                "Grok Build",
                "Not signed in to Grok",
            )
        }
        Err(_) => {
            return QuotaSnapshot::error(
                "grok",
                "default",
                "Grok Build",
                "Unable to read Grok auth file",
            )
        }
    };
    let parsed: Value = match serde_json::from_str(&raw) {
        Ok(value) => value,
        Err(_) => {
            return QuotaSnapshot::error(
                "grok",
                "default",
                "Grok Build",
                "Grok auth file is invalid",
            )
        }
    };
    let Some(entries) = parsed.as_object() else {
        return QuotaSnapshot::error("grok", "default", "Grok Build", "Grok auth file is invalid");
    };
    let selected = entries
        .iter()
        .filter(|(issuer, _)| issuer.starts_with("https://auth.x.ai"))
        .chain(entries.iter())
        .find_map(|(issuer_key, value)| {
            let token = value.get("key").and_then(Value::as_str)?;
            if token.trim().is_empty() {
                return None;
            }
            Some(GrokAuth {
                issuer_key: issuer_key.clone(),
                access_token: token.to_string(),
                user_id: value
                    .get("user_id")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                refresh_token: value
                    .get("refresh_token")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                oidc_issuer: value
                    .get("oidc_issuer")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                oidc_client_id: value
                    .get("oidc_client_id")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                expires_at: value
                    .get("expires_at")
                    .and_then(Value::as_str)
                    .and_then(grok_timestamp),
            })
        });
    let Some(mut auth) = selected else {
        return QuotaSnapshot::unavailable(
            "grok",
            "default",
            "Grok Build",
            "Not signed in to Grok",
        );
    };
    let base = shell_environment_value("GROK_CLI_CHAT_PROXY_BASE_URL")
        .await
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "https://cli-chat-proxy.grok.com/v1".to_string());
    let base = base.trim_end_matches('/').to_string();
    let client = reqwest::Client::new();

    // The stored access token expires a few hours after issue; the CLI keeps
    // the session alive by refreshing on demand, so a stale token here is the
    // normal case rather than a signed-out one.
    let mut refresh_attempted = false;
    if auth.expired() && auth.can_refresh() {
        refresh_attempted = true;
        let _ = refresh_grok_auth(&client, &auth_path, &mut auth).await;
    }

    let credits_url = format!("{base}/billing?format=credits");
    let mut credits = fetch_grok_billing(&client, &credits_url, &auth).await;
    if matches!(credits, Ok(None)) && !refresh_attempted && auth.can_refresh() {
        refresh_attempted = true;
        if refresh_grok_auth(&client, &auth_path, &mut auth).await.is_ok() {
            credits = fetch_grok_billing(&client, &credits_url, &auth).await;
        }
    }
    if let Ok(Some(data)) = credits {
        let config = data.get("config").unwrap_or(&data);
        if let Some(used_percent) = config.get("creditUsagePercent").and_then(numeric) {
            let reset = config
                .get("currentPeriod")
                .and_then(|value| value.get("end"))
                .or_else(|| config.get("billingPeriodEnd"))
                .and_then(Value::as_str);
            return QuotaSnapshot::ok(
                "grok",
                "default",
                "Grok Build",
                vec![QuotaWindow {
                    label: "Weekly".to_string(),
                    used_percent: used_percent.clamp(0.0, 100.0),
                    window_minutes: Some(WEEKLY_WINDOW_MINUTES),
                    resets_at: reset.and_then(parse_timestamp_millis),
                    reset_description: reset.map(str::to_string),
                }],
                Vec::new(),
            );
        }
    }
    let billing_url = format!("{base}/billing");
    let mut usage = fetch_grok_billing(&client, &billing_url, &auth).await;
    if matches!(usage, Ok(None))
        && !refresh_attempted
        && auth.can_refresh()
        && refresh_grok_auth(&client, &auth_path, &mut auth).await.is_ok()
    {
        usage = fetch_grok_billing(&client, &billing_url, &auth).await;
    }
    let default_data = match usage {
        Ok(Some(value)) => value,
        Ok(None) => {
            return QuotaSnapshot::unavailable(
                "grok",
                "default",
                "Grok Build",
                "Grok sign-in expired. Sign in with Grok again.",
            )
        }
        Err(error) => return command_error_snapshot("grok", "default", "Grok Build", error),
    };
    let config = default_data.get("config").unwrap_or(&default_data);
    let limit = config
        .get("monthlyLimit")
        .and_then(|value| value.get("val"))
        .and_then(numeric);
    let used = config
        .get("used")
        .and_then(|value| value.get("val"))
        .and_then(numeric);
    match (limit, used) {
        (Some(limit), Some(used)) if limit > 0.0 => QuotaSnapshot::ok(
            "grok",
            "default",
            "Grok Build",
            vec![QuotaWindow {
                label: "Monthly".to_string(),
                used_percent: ((used / limit) * 100.0).clamp(0.0, 100.0),
                window_minutes: Some(43_200),
                resets_at: None,
                reset_description: None,
            }],
            Vec::new(),
        ),
        _ => QuotaSnapshot::unavailable(
            "grok",
            "default",
            "Grok Build",
            "Grok billing response did not include usage",
        ),
    }
}

/// `Ok(None)` when the token was rejected, so the caller can refresh and retry.
async fn fetch_grok_billing(
    client: &reqwest::Client,
    url: &str,
    auth: &GrokAuth,
) -> Result<Option<Value>> {
    let mut request = client
        .get(url)
        .header(AUTHORIZATION, format!("Bearer {}", auth.access_token))
        .header("X-XAI-Token-Auth", "xai-grok-cli")
        .header(ACCEPT, "application/json")
        .timeout(FETCH_TIMEOUT);
    if let Some(user_id) = auth.user_id.as_deref() {
        request = request.header("x-userid", user_id);
    }
    let response = request.send().await?;
    let status = response.status().as_u16();
    if status == 401 || status == 403 {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(anyhow!("Grok usage request failed (HTTP {status})"));
    }
    Ok(Some(response.json().await?))
}

async fn refresh_grok_auth(
    client: &reqwest::Client,
    auth_path: &std::path::Path,
    auth: &mut GrokAuth,
) -> Result<()> {
    let issuer = auth
        .oidc_issuer
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("Grok auth entry has no OIDC issuer"))?;
    let client_id = auth
        .oidc_client_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("Grok auth entry has no OIDC client id"))?;
    let consumed_refresh = auth
        .refresh_token
        .clone()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("Grok auth entry has no refresh token"))?;
    let response = client
        .post(format!("{}/oauth2/token", issuer.trim_end_matches('/')))
        .header(ACCEPT, "application/json")
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(serde_urlencoded::to_string([
            ("grant_type", "refresh_token"),
            ("refresh_token", consumed_refresh.as_str()),
            ("client_id", client_id),
        ])?)
        .timeout(FETCH_TIMEOUT)
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(anyhow!(
            "Grok token refresh failed (HTTP {})",
            response.status().as_u16()
        ));
    }
    let body: Value = response.json().await?;
    let access_token = body
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("Grok token refresh response had no access token"))?
        .to_string();
    let rotated_refresh = body
        .get("refresh_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let expires_at = body
        .get("expires_in")
        .and_then(numeric)
        .map(|seconds| chrono::Utc::now() + chrono::Duration::seconds(seconds as i64));
    // The token endpoint rotates the refresh token, so it must be persisted
    // for the CLI to keep its session. A failed write still leaves a usable
    // access token for this fetch.
    if let Err(error) = persist_grok_auth(
        auth_path,
        &auth.issuer_key,
        &consumed_refresh,
        &access_token,
        rotated_refresh.as_deref(),
        expires_at,
    )
    .await
    {
        tracing::warn!("grok quota: could not persist refreshed token: {error}");
    }
    auth.access_token = access_token;
    if let Some(rotated) = rotated_refresh {
        auth.refresh_token = Some(rotated);
    }
    if expires_at.is_some() {
        auth.expires_at = expires_at;
    }
    Ok(())
}

/// Replaces the stored tokens only while the consumed refresh token is still
/// the persisted one: when the CLI refreshed in parallel it already wrote the
/// newer set, and overwriting it would strand the rotated credentials.
async fn persist_grok_auth(
    auth_path: &std::path::Path,
    issuer_key: &str,
    consumed_refresh: &str,
    access_token: &str,
    refresh_token: Option<&str>,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<()> {
    let raw = tokio::fs::read_to_string(auth_path).await?;
    let mut parsed: Value = serde_json::from_str(&raw)?;
    let entry = parsed
        .get_mut(issuer_key)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| anyhow!("Grok auth entry disappeared from auth.json"))?;
    if entry.get("refresh_token").and_then(Value::as_str) != Some(consumed_refresh) {
        return Ok(());
    }
    entry.insert("key".to_string(), json!(access_token));
    if let Some(refresh_token) = refresh_token {
        entry.insert("refresh_token".to_string(), json!(refresh_token));
    }
    if let Some(expires_at) = expires_at {
        entry.insert(
            "expires_at".to_string(),
            json!(expires_at.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true)),
        );
    }
    let tmp_path = {
        let mut name = auth_path.as_os_str().to_os_string();
        name.push(".tmp");
        PathBuf::from(name)
    };
    tokio::fs::write(&tmp_path, serde_json::to_string_pretty(&parsed)?).await?;
    tokio::fs::rename(&tmp_path, auth_path).await?;
    Ok(())
}

fn grok_timestamp(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&chrono::Utc))
}
