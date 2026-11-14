//! Read-only, no-start adapter to the existing Host quota service.
//! The Native process never sees control-file tokens or provider credentials.
use serde_json::{json, Value};

use crate::cli::{AgentQuotaAction, AgentQuotaCommand};
use crate::runtime_host_client::RuntimeHostRpcClient;
use crate::{print_error, print_value, runtime_dir, USAGE_EXIT_CODE};

const QUOTA_DEADLINE_MS: u64 = 60_000;
const USAGE_DEADLINE_MS: u64 = 90_000;
const MAX_SNAPSHOTS: usize = 128;
const PROVIDERS: &[&str] = &[
    "claude", "codex", "kimi", "grok", "cursor", "agy", "minimax", "zai", "devin", "opencode",
];

fn validate_quota_response(response: &Value) -> Result<(), &'static str> {
    match response.get("snapshots").and_then(Value::as_array) {
        Some(rows) if rows.len() <= MAX_SNAPSHOTS => Ok(()),
        _ => Err("Invalid or oversized Host quota response"),
    }
}

/// Filter Host runtime settings to provider IDs only; environment values
/// remain private to the Runtime Host.
fn quota_display_config(response: &Value) -> Result<Value, &'static str> {
    let providers = response
        .get("agentQuotas")
        .and_then(|quotas| quotas.get("enabledProviders"))
        .and_then(Value::as_array)
        .ok_or("Host quota configuration is unavailable")?;
    if providers.len() > PROVIDERS.len() {
        return Err("Quota provider list exceeds limits");
    }
    let mut seen = std::collections::HashSet::new();
    let mut safe = Vec::with_capacity(providers.len());
    for provider in providers {
        let id = provider.as_str().ok_or("Invalid quota provider ID")?;
        if !PROVIDERS.contains(&id) || !seen.insert(id) {
            return Err("Unknown or duplicate quota provider ID");
        }
        safe.push(id);
    }
    Ok(json!({"enabledProviders": safe}))
}

/// Return only the aggregate daily/provider totals needed by Native.
/// Never expose raw model/session/transcript details or settings in the CLI
/// output; usage scanning and profile resolution remain Host-owned.
fn usage_display_summary(response: &Value) -> Result<Value, &'static str> {
    use std::collections::BTreeMap;
    let since = response["sinceDay"]
        .as_str()
        .filter(|day| day.len() == 10)
        .ok_or("Invalid Host usage start day")?;
    let until = response["untilDay"]
        .as_str()
        .filter(|day| day.len() == 10)
        .ok_or("Invalid Host usage end day")?;
    let buckets = response["buckets"]
        .as_array()
        .filter(|rows| rows.len() <= 20_000)
        .ok_or("Invalid or oversized Host usage response")?;
    let mut providers = BTreeMap::<String, (u64, f64)>::new();
    let mut days = BTreeMap::<String, (u64, f64)>::new();
    let (mut total_tokens, mut cost_usd, mut unpriced_records) = (0_u64, 0.0_f64, 0_u64);
    for bucket in buckets {
        let provider = bucket["provider"]
            .as_str()
            .filter(|provider| matches!(*provider, "claude" | "codex" | "grok"))
            .ok_or("Invalid Host usage provider")?;
        let day = bucket["day"]
            .as_str()
            .filter(|day| day.len() == 10 && *day >= since && *day <= until)
            .ok_or("Invalid Host usage day")?;
        let totals = bucket["totals"]
            .as_object()
            .ok_or("Missing Host usage token totals")?;
        let mut tokens = 0_u64;
        for key in [
            "uncachedInputTokens",
            "cachedInputTokens",
            "cacheCreationTokens",
            "outputTokens",
        ] {
            let count = totals
                .get(key)
                .and_then(Value::as_u64)
                .ok_or("Invalid Host usage tokens")?;
            tokens = tokens
                .checked_add(count)
                .ok_or("Host usage token overflow")?;
        }
        let cost = bucket["costUsd"]
            .as_f64()
            .filter(|value| value.is_finite() && *value >= 0.0)
            .ok_or("Invalid Host usage cost")?;
        let unpriced = bucket["unpricedRecords"].as_u64().unwrap_or(0);
        total_tokens = total_tokens
            .checked_add(tokens)
            .ok_or("Host usage token overflow")?;
        unpriced_records = unpriced_records.saturating_add(unpriced);
        cost_usd += cost;
        if !cost_usd.is_finite() {
            return Err("Host usage cost overflow");
        }
        for record in [&mut providers, &mut days].into_iter().zip([provider, day]) {
            let (map, key) = record;
            let entry = map.entry(key.to_owned()).or_default();
            entry.0 = entry.0.saturating_add(tokens);
            entry.1 += cost;
        }
    }
    let provider_rows = providers.into_iter().map(|(provider, (tokens, cost))|
        json!({"provider":provider,"tokens":tokens,"costUsd":cost})
    ).collect::<Vec<_>>();
    let day_rows = days
        .into_iter()
        .map(|(day, (tokens, cost))| json!({"day":day,"tokens":tokens,"costUsd":cost}))
        .collect::<Vec<_>>();
    let incomplete_sources = response
        .get("sources")
        .and_then(Value::as_array)
        .is_some_and(|sources| {
            sources
                .iter()
                .any(|source| source["status"].as_str() != Some("ok"))
        });
    Ok(json!({
        "sinceDay": since, "untilDay": until, "totalTokens": total_tokens,
        "costUsd": cost_usd, "unpricedRecords": unpriced_records,
        "incompleteSources": incomplete_sources,
        "providers": provider_rows, "days": day_rows,
    }))
}

pub async fn run(command: AgentQuotaCommand) -> i32 {
    if !command.no_start {
        eprintln!("Quota inspection requires --no-start and an existing Runtime Host");
        return USAGE_EXIT_CODE;
    }
    let (method, payload, deadline_ms) = match command.action {
        AgentQuotaAction::Snapshot {
            force_refresh,
            provider,
            account_id,
        } => {
            if provider
                .as_ref()
                .is_some_and(|provider| !PROVIDERS.contains(&provider.as_str()))
            {
                return print_error("Unknown quota provider");
            }
            if account_id
                .as_ref()
                .is_some_and(|account| account.trim().is_empty() || account.len() > 256)
            {
                return print_error("Invalid quota account");
            }
            (
                "agentQuota.snapshot",
                json!({"forceRefresh": force_refresh, "provider": provider, "accountId": account_id}),
                QUOTA_DEADLINE_MS,
            )
        }
        AgentQuotaAction::Usage { days } => {
            if days != 7 && days != 30 {
                return print_error("Usage period must be 7 or 30 days");
            }
            let today = chrono::Local::now().date_naive();
            let since = today - chrono::Duration::days(i64::from(days) - 1);
            (
                "agentUsage.snapshot",
                json!({
                    "sinceDay": since.format("%Y-%m-%d").to_string(),
                    "untilDay": today.format("%Y-%m-%d").to_string(),
                    "includeGrok": true
                }),
                USAGE_DEADLINE_MS,
            )
        }
        AgentQuotaAction::Config => ("runtimeSettings.get", json!({}), QUOTA_DEADLINE_MS),
    };
    let runtime = runtime_dir(&command.runtime);
    let Some(mut client) = (match RuntimeHostRpcClient::connect(&runtime).await {
        Ok(client) => client,
        Err(error) => return print_error(error),
    }) else {
        return print_error("Existing Alera Runtime Host is not running");
    };
    match client
        .request_value_with_deadline(method, &payload, deadline_ms)
        .await
    {
        Ok(value) if method == "runtimeSettings.get" => match quota_display_config(&value) {
            Ok(safe) => {
                print_value(&safe, command.output.json, "Quota display order fetched");
                0
            }
            Err(error) => print_error(error),
        },
        Ok(value) if method == "agentUsage.snapshot" => match usage_display_summary(&value) {
            Ok(summary) => {
                print_value(&summary, command.output.json, "Agent usage fetched");
                0
            }
            Err(error) => print_error(error),
        },
        Ok(value) => match validate_quota_response(&value) {
            Ok(()) => {
                print_value(&value, command.output.json, "Agent quotas fetched");
                0
            }
            Err(error) => print_error(error),
        },
        Err(error) => print_error(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{Cli, Command};
    use clap::Parser;

    #[test]
    fn quota_cli_is_explicit_no_start_and_account_scoped() {
        let Cli { command } = Cli::try_parse_from([
            "alera",
            "quota",
            "--no-start",
            "--json",
            "snapshot",
            "--force-refresh",
            "--provider",
            "agy",
            "--account-id",
            "default",
        ])
        .unwrap();
        let Command::Quota(command) = command else {
            panic!("expected quota")
        };
        assert!(command.no_start);
        assert!(command.output.json);
        let AgentQuotaAction::Snapshot {
            force_refresh,
            provider,
            account_id,
        } = command.action
        else {
            panic!("expected snapshot")
        };
        assert!(force_refresh);
        assert_eq!(provider.as_deref(), Some("agy"));
        assert_eq!(account_id.as_deref(), Some("default"));
        assert!(Cli::try_parse_from([
            "alera",
            "quota",
            "--no-start",
            "snapshot",
            "--account-id",
            "default"
        ])
        .is_err());
    }

    #[test]
    fn quota_config_returns_only_allowlisted_provider_ids() {
        let raw = json!({"agentQuotas":{
            "enabledProviders":["agy","claude","codex"],
            "environment":{"zaiBaseUrl":"private-endpoint"}
        }});
        assert_eq!(
            quota_display_config(&raw).unwrap(),
            json!({"enabledProviders":["agy","claude","codex"]})
        );
        assert!(quota_display_config(&json!({"agentQuotas":{
            "enabledProviders":["claude","claude"]
        }}))
        .is_err());
        let parsed = Cli::try_parse_from(["alera", "quota", "--no-start", "config"]).unwrap();
        assert!(matches!(
            parsed.command,
            Command::Quota(AgentQuotaCommand {
                action: AgentQuotaAction::Config,
                ..
            })
        ));
    }

    #[test]
    fn usage_cli_rejects_other_periods_and_summary_only_exports_aggregates() {
        let parsed = Cli::try_parse_from([
            "alera",
            "quota",
            "--no-start",
            "--json",
            "usage",
            "--days",
            "7",
        ])
        .unwrap();
        assert!(matches!(
            parsed.command,
            Command::Quota(AgentQuotaCommand {
                action: AgentQuotaAction::Usage { days: 7 },
                ..
            })
        ));
        let raw = json!({
            "sinceDay": "2026-11-08", "untilDay": "2026-11-14",
            "buckets": [
                {"provider":"codex","day":"2026-11-13","accountId":"secret","model":"private",
                 "totals":{"uncachedInputTokens":10,"cachedInputTokens":20,
                    "cacheCreationTokens":30,"outputTokens":40,"reasoningTokens":15},
                 "costUsd":0.2,"unpricedRecords":2},
                {"provider":"claude","day":"2026-11-14",
                 "totals":{"uncachedInputTokens":1,"cachedInputTokens":0,
                    "cacheCreationTokens":0,"outputTokens":2}, "costUsd":0.1}
            ], "sources": [{"status":"ok"}]
        });
        let summary = usage_display_summary(&raw).unwrap();
        assert_eq!(summary["totalTokens"], 103);
        assert_eq!(summary["unpricedRecords"], 2);
        assert_eq!(summary["providers"].as_array().unwrap().len(), 2);
        let serialized = summary.to_string();
        assert!(!serialized.contains("secret"));
        assert!(!serialized.contains("private"));
        assert!(usage_display_summary(&json!({"buckets":[]})).is_err());
        assert!(usage_display_summary(&json!({
            "sinceDay":"2026-11-08","untilDay":"2026-11-14",
            "buckets":[{"provider":"claude","day":"2026-11-14","totals":{},
                         "costUsd":0.1}]
        }))
        .is_err());
    }

    #[test]
    fn validates_bounded_host_payload_without_credential_fields() {
        assert!(validate_quota_response(&json!({"snapshots":[]})).is_ok());
        assert!(validate_quota_response(&json!({"secret":"ignored"})).is_err());
        assert!(validate_quota_response(&json!({"snapshots":vec![Value::Null; 129]})).is_err());
    }
}
