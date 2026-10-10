use alera_core::agent_descriptor::canonical_agent_id;
use alera_core::runtime::RuntimeAgentQuotaSettings;
use serde_json::{json, Value};

use super::host_service_agent_quota::{merge_quota_snapshots, upsert_quota_snapshot};

pub(super) fn optional_quota_text(payload: &Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Builds the fetch body for a full snapshot or for one provider/account.
///
/// `provider` narrows the request to that agent. `account_id` is only applied
/// for providers that have more than one account (Claude, Codex, OpenCode).
pub(super) fn quota_fetch_payload(
    settings: &RuntimeAgentQuotaSettings,
    environment_values: &Value,
    provider: Option<&str>,
    account_id: Option<&str>,
) -> Value {
    let provider = provider.map(str::trim).filter(|value| !value.is_empty());
    let account_id = account_id.map(str::trim).filter(|value| !value.is_empty());
    let providers = if let Some(provider) = provider {
        vec![canonical_agent_id(provider).unwrap_or(provider).to_string()]
    } else if settings.enabled_providers.is_empty() {
        vec!["__none__".to_string()]
    } else {
        settings.enabled_providers.clone()
    };
    let mut claude_default_enabled = settings.claude_default_enabled;
    let mut claude_profiles = settings.claude_profiles.clone();
    let mut codex_profiles = settings.codex_profiles.clone();
    if provider.is_some() {
        if !providers.iter().any(|item| item == "claude") {
            claude_default_enabled = false;
            claude_profiles.clear();
        }
        if !providers.iter().any(|item| item == "codex") {
            codex_profiles.clear();
        }
    }
    if let Some(account_id) = account_id {
        if providers.iter().any(|item| item == "claude") {
            claude_default_enabled = account_id == "default" && settings.claude_default_enabled;
            claude_profiles.retain(|profile| profile.profile == account_id);
        }
        if providers.iter().any(|item| item == "codex") {
            codex_profiles.retain(|profile| profile.profile == account_id);
        }
    }
    json!({
        "providers": providers,
        "accountId": account_id,
        "claudeDefaultEnabled": claude_default_enabled,
        "codexProfiles": codex_profiles,
        "claudeProfiles": claude_profiles,
        "environmentNames": settings.environment,
        "environmentValues": environment_values,
        "allowCliFallback": false,
    })
}

/// Upserts the fetched rows into an existing cache. Returns `None` for the
/// cache when there was nothing to update, so a partial result cannot become
/// the full snapshot.
pub(super) fn apply_partial_quota_update(
    cached: Option<&Value>,
    fresh: Value,
) -> (Option<Value>, Value) {
    let Some(cached) = cached else {
        return (None, fresh);
    };
    let reply = merge_quota_snapshots(cached, fresh.clone());
    let mut merged = cached.clone();
    if let Some(snapshots) = reply.get("snapshots").and_then(Value::as_array) {
        for snapshot in snapshots {
            merged = upsert_quota_snapshot(Some(&merged), snapshot.clone());
        }
    }
    if let Some(environment) = fresh.get("environment") {
        if let Some(object) = merged.as_object_mut() {
            object.insert("environment".to_string(), environment.clone());
        }
    }
    (Some(merged), reply)
}

#[cfg(test)]
mod tests {
    use alera_core::runtime::{RuntimeClaudeQuotaProfile, RuntimeCodexQuotaProfile};

    use super::*;

    fn settings() -> RuntimeAgentQuotaSettings {
        RuntimeAgentQuotaSettings {
            enabled_providers: vec!["claude".into(), "codex".into()],
            claude_default_enabled: true,
            claude_profiles: vec![
                RuntimeClaudeQuotaProfile {
                    alias: "Work".into(),
                    profile: "work".into(),
                    show_in_usage: true,
                    usage_display_name: None,
                },
                RuntimeClaudeQuotaProfile {
                    alias: "Home".into(),
                    profile: "home".into(),
                    show_in_usage: true,
                    usage_display_name: None,
                },
            ],
            codex_profiles: vec![RuntimeCodexQuotaProfile {
                alias: "Work".into(),
                profile: "work".into(),
            }],
            ..RuntimeAgentQuotaSettings::default()
        }
    }

    #[test]
    fn full_payload_keeps_every_enabled_provider() {
        let payload = quota_fetch_payload(&settings(), &json!({}), None, None);
        assert_eq!(payload["providers"], json!(["claude", "codex"]));
        assert!(payload["claudeDefaultEnabled"].as_bool().unwrap());
        assert_eq!(payload["claudeProfiles"].as_array().unwrap().len(), 2);
        assert!(payload["accountId"].is_null());
    }

    #[test]
    fn claude_account_payload_fetches_only_that_profile() {
        let payload = quota_fetch_payload(&settings(), &json!({}), Some("claude"), Some("work"));
        assert_eq!(payload["providers"], json!(["claude"]));
        assert_eq!(payload["accountId"], "work");
        assert!(!payload["claudeDefaultEnabled"].as_bool().unwrap());
        assert_eq!(payload["claudeProfiles"].as_array().unwrap().len(), 1);
        assert_eq!(payload["claudeProfiles"][0]["profile"], "work");
        assert!(payload["codexProfiles"].as_array().unwrap().is_empty());
    }

    #[test]
    fn partial_update_keeps_other_agents_and_stale_failure() {
        let cached = json!({
            "snapshots": [
                {"provider":"claude","accountId":"work","status":"ok","windows":[1]},
                {"provider":"codex","accountId":"default","status":"ok","windows":[2]}
            ],
            "environment": {"KIMI": true}
        });
        let fresh = json!({
            "snapshots": [
                {"provider":"claude","accountId":"work","status":"error","error":"offline","windows":[]}
            ],
            "environment": {"KIMI": false}
        });
        let (updated, reply) = apply_partial_quota_update(Some(&cached), fresh);
        let updated = updated.unwrap();
        assert_eq!(updated["snapshots"].as_array().unwrap().len(), 2);
        assert_eq!(updated["snapshots"][0]["status"], "stale");
        assert_eq!(updated["snapshots"][0]["windows"], json!([1]));
        assert_eq!(updated["snapshots"][0]["error"], "offline");
        assert_eq!(updated["snapshots"][1]["provider"], "codex");
        assert_eq!(updated["snapshots"][1]["status"], "ok");
        assert_eq!(updated["environment"]["KIMI"], false);
        assert_eq!(reply["snapshots"].as_array().unwrap().len(), 1);
        assert_eq!(reply["snapshots"][0]["status"], "stale");
    }

    #[test]
    fn failed_provider_keeps_previous_snapshot_as_stale() {
        let cached = json!({"snapshots": [{"provider":"codex","accountId":"default","status":"ok","windows":[1]}]});
        let fresh = json!({"snapshots": [{"provider":"codex","accountId":"default","status":"error","error":"offline","windows":[]}]});
        let merged = merge_quota_snapshots(&cached, fresh);
        assert_eq!(merged["snapshots"][0]["status"], "stale");
        assert_eq!(merged["snapshots"][0]["windows"], json!([1]));
        assert_eq!(merged["snapshots"][0]["error"], "offline");
    }

    #[test]
    fn upsert_replaces_matching_claude_account() {
        let cached = json!({
            "snapshots": [
                {"provider":"claude","accountId":"default","status":"unavailable"},
                {"provider":"codex","accountId":"default","status":"ok"}
            ],
            "environment": {"KIMI": true}
        });
        let merged = upsert_quota_snapshot(
            Some(&cached),
            json!({"provider":"claude","accountId":"default","status":"ok","windows":[1]}),
        );
        assert_eq!(merged["snapshots"].as_array().unwrap().len(), 2);
        assert_eq!(merged["snapshots"][0]["status"], "ok");
        assert_eq!(merged["snapshots"][0]["windows"], json!([1]));
        assert_eq!(merged["snapshots"][1]["provider"], "codex");
        assert_eq!(merged["environment"]["KIMI"], true);
    }

    #[test]
    fn partial_update_does_not_invent_a_full_cache() {
        let fresh =
            json!({"snapshots": [{"provider":"codex","accountId":"default","status":"ok"}]});
        let (updated, reply) = apply_partial_quota_update(None, fresh);
        assert!(updated.is_none());
        assert_eq!(reply["snapshots"][0]["provider"], "codex");
    }
}
