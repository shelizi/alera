//! Read-only, no-start adapter to the existing Host quota service.
//! The Native process never sees control-file tokens or provider credentials.
use serde_json::{json, Value};

use crate::cli::{AgentQuotaAction, AgentQuotaCommand};
use crate::runtime_host_client::RuntimeHostRpcClient;
use crate::{print_error, print_value, runtime_dir, USAGE_EXIT_CODE};

const QUOTA_DEADLINE_MS: u64 = 60_000;
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

pub async fn run(command: AgentQuotaCommand) -> i32 {
    if !command.no_start {
        eprintln!("Quota inspection requires --no-start and an existing Runtime Host");
        return USAGE_EXIT_CODE;
    }
    let (method, payload) = match command.action {
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
            )
        }
        AgentQuotaAction::Config => ("runtimeSettings.get", json!({})),
    };
    let runtime = runtime_dir(&command.runtime);
    let Some(mut client) = (match RuntimeHostRpcClient::connect(&runtime).await {
        Ok(client) => client,
        Err(error) => return print_error(error),
    }) else {
        return print_error("Existing Alera Runtime Host is not running");
    };
    match client
        .request_value_with_deadline(method, &payload, QUOTA_DEADLINE_MS)
        .await
    {
        Ok(value) if method == "runtimeSettings.get" => match quota_display_config(&value) {
            Ok(safe) => {
                print_value(&safe, command.output.json, "Quota display order fetched");
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
    fn validates_bounded_host_payload_without_credential_fields() {
        assert!(validate_quota_response(&json!({"snapshots":[]})).is_ok());
        assert!(validate_quota_response(&json!({"secret":"ignored"})).is_err());
        assert!(validate_quota_response(&json!({"snapshots":vec![Value::Null; 129]})).is_err());
    }
}
