use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::agent_quota::{
    consume_codex_reset_credit, fetch_agent_quotas, fetch_agent_usage, fetch_claude_tui,
};
use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::protocol::{error_response, event, ok_response};

use super::deferred_admission::DeferredRequestClass;
use super::host_service_requests::required_non_blank;
use super::requests::idempotency_receipts::{
    optional_client_mutation_id, payload_digest, prepare_receipt, remove_receipt, settle_receipt,
    ReceiptPrepareOutcome,
};
use super::{ServerActor, ServerCommand};

const AGENT_QUOTA_CACHE_TTL: Duration = Duration::from_secs(15 * 60);

impl ServerActor {
    pub(super) fn start_agent_usage_request(
        &mut self,
        client_id: u64,
        request_id: i64,
        payload: &Value,
    ) -> HostResult<()> {
        let since_day = required_non_blank(payload, "sinceDay")?;
        let until_day = required_non_blank(payload, "untilDay")?;
        let include_grok = payload.get("includeGrok").and_then(Value::as_bool) == Some(true);
        let store = self.runtime_store.clone();
        let inbox = self.inbox.clone();
        self.deferred_admission.schedule(
            DeferredRequestClass::Bulk,
            "agentUsage.snapshot",
            Some(client_id),
            async move {
                let result = async {
                    let settings = store
                        .agent_quota_settings()
                        .await
                        .map_err(|error| HostError::state(error.to_string()))?;
                    let claude_profiles = settings.claude_profiles_for_usage();
                    fetch_agent_usage(json!({
                        "sinceDay": since_day,
                        "untilDay": until_day,
                        "includeGrok": include_grok,
                        "claudeDefaultEnabled": settings.claude_default_show_in_usage,
                        "claudeProfiles": claude_profiles,
                    }))
                    .await
                    .map_err(|error| HostError::state(error.to_string()))
                }
                .await;
                let _ = inbox.send(ServerCommand::HostToolFinished {
                    client_id,
                    request_id,
                    result,
                    operation_id: None,
                    skill: None,
                });
            },
        )?;
        Ok(())
    }

    pub(super) fn start_agent_quota_request(
        &mut self,
        client_id: u64,
        request_id: i64,
        payload: &Value,
    ) -> HostResult<()> {
        let force_refresh = payload
            .get("forceRefresh")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let environment_values = payload
            .get("environmentValues")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let environment_signature = quota_environment_signature(&environment_values);
        if !force_refresh {
            if let Some((fetched_at, cached_signature, cached)) = &self.agent_quota_cache {
                if *cached_signature == environment_signature
                    && fetched_at.elapsed() < AGENT_QUOTA_CACHE_TTL
                {
                    self.client_write(client_id, ok_response(request_id, cached.clone()));
                    return Ok(());
                }
            }
        }
        let store = self.runtime_store.clone();
        let inbox = self.inbox.clone();
        self.deferred_admission.schedule(
            DeferredRequestClass::Bulk,
            "agentQuota.snapshot",
            Some(client_id),
            async move {
                let result = async {
                    let settings = store
                        .agent_quota_settings()
                        .await
                        .map_err(|error| HostError::state(error.to_string()))?;
                    let providers = if settings.enabled_providers.is_empty() {
                        vec!["__none__".to_string()]
                    } else {
                        settings.enabled_providers
                    };
                    let quota_payload = json!({
                        "providers": providers,
                        "claudeDefaultEnabled": settings.claude_default_enabled,
                        "claudeProfiles": settings.claude_profiles,
                        "environmentNames": settings.environment,
                        "environmentValues": environment_values,
                        "allowCliFallback": false,
                    });
                    fetch_agent_quotas(quota_payload)
                        .await
                        .map_err(|error| HostError::state(error.to_string()))
                }
                .await;
                let _ = inbox.send(ServerCommand::AgentQuotaFinished {
                    client_id,
                    request_id,
                    environment_signature,
                    result,
                });
            },
        )?;
        Ok(())
    }

    pub(super) fn start_agent_quota_claude_tui_request(
        &mut self,
        client_id: u64,
        request_id: i64,
        payload: &Value,
    ) -> HostResult<()> {
        let account_id = required_non_blank(payload, "accountId")?;
        let environment_signature = self
            .agent_quota_cache
            .as_ref()
            .map(|(_, signature, _)| *signature)
            .unwrap_or(0);
        let store = self.runtime_store.clone();
        let inbox = self.inbox.clone();
        self.deferred_admission.schedule(
            DeferredRequestClass::Bulk,
            "agentQuota.fetchClaudeTui",
            Some(client_id),
            async move {
                let result = async {
                    let settings = store
                        .agent_quota_settings()
                        .await
                        .map_err(|error| HostError::state(error.to_string()))?;
                    let display_name = if account_id == "default" {
                        "Default".to_string()
                    } else {
                        settings
                            .claude_profiles
                            .iter()
                            .find(|profile| profile.profile == account_id)
                            .map(|profile| profile.alias.clone())
                            .unwrap_or_else(|| account_id.clone())
                    };
                    let snapshot_value = fetch_claude_tui(&account_id, &display_name).await;
                    Ok(json!({
                        "snapshot": snapshot_value.clone(),
                        "snapshots": [snapshot_value],
                        "environment": {},
                    }))
                }
                .await;
                let _ = inbox.send(ServerCommand::AgentQuotaClaudeTuiFinished {
                    client_id,
                    request_id,
                    environment_signature,
                    result,
                });
            },
        )?;
        Ok(())
    }

    pub(super) async fn start_agent_quota_codex_reset_request(
        &mut self,
        client_id: u64,
        request_id: i64,
        payload: &Value,
    ) -> HostResult<()> {
        let environment_signature = self
            .agent_quota_cache
            .as_ref()
            .map(|(_, signature, _)| *signature)
            .unwrap_or(0);
        let client_mutation_id = optional_client_mutation_id(payload)?;
        let (receipt_context, replay_result) = if let Some(client_mutation_id) = client_mutation_id
        {
            let caller_scope = self.agent_profile_launch_caller_scope(client_id)?;
            let digest =
                payload_digest(payload).map_err(|error| HostError::state(error.to_string()))?;
            let outcome = prepare_receipt(
                &self.runtime_store,
                "agentQuota.consumeCodexResetCredit",
                &caller_scope,
                &client_mutation_id,
                &digest,
                None,
            )
            .await
            .map_err(|error| HostError::state(error.to_string()))?;
            match outcome {
                ReceiptPrepareOutcome::Created => {
                    (Some((caller_scope, client_mutation_id, digest)), None)
                }
                ReceiptPrepareOutcome::Replay(result) => (None, Some(result)),
                ReceiptPrepareOutcome::Conflict => {
                    return Err(HostError::state(
                        "clientMutationId was already used with a different agentQuota.consumeCodexResetCredit payload.",
                    ));
                }
                ReceiptPrepareOutcome::InProgress => {
                    return Err(HostError::state("clientMutationId is already in progress."));
                }
            }
        } else {
            (None, None)
        };
        let store = self.runtime_store.clone();
        let payload = payload.clone();
        let inbox = self.inbox.clone();
        let scheduled_receipt_context = receipt_context.clone();
        let scheduled = self.deferred_admission.schedule(
            DeferredRequestClass::Bulk,
            "agentQuota.consumeCodexResetCredit",
            Some(client_id),
            async move {
                let result = if let Some(result) = replay_result {
                    Ok(result)
                } else {
                    match consume_codex_reset_credit(&store, payload).await {
                        Ok(result) => {
                            if let Some((caller_scope, client_mutation_id, digest)) =
                                scheduled_receipt_context.as_ref()
                            {
                                match settle_receipt(
                                    &store,
                                    "agentQuota.consumeCodexResetCredit",
                                    caller_scope,
                                    client_mutation_id,
                                    digest,
                                    &result,
                                )
                                .await
                                {
                                    Ok(()) => Ok(result),
                                    Err(error) => Err(HostError::state(error.to_string())),
                                }
                            } else {
                                Ok(result)
                            }
                        }
                        Err(error) => {
                            if let Some((caller_scope, client_mutation_id, digest)) =
                                scheduled_receipt_context.as_ref()
                            {
                                if let Err(cleanup_error) = remove_receipt(
                                    &store,
                                    "agentQuota.consumeCodexResetCredit",
                                    caller_scope,
                                    client_mutation_id,
                                    digest,
                                )
                                .await
                                {
                                    tracing::error!(
                                        client_mutation_id = %client_mutation_id,
                                        "failed to roll back Codex reset receipt: {cleanup_error}"
                                    );
                                }
                            }
                            Err(HostError::state(error.to_string()))
                        }
                    }
                };
                let _ = inbox.send(ServerCommand::AgentQuotaCodexResetFinished {
                    client_id,
                    request_id,
                    environment_signature,
                    result,
                });
            },
        );
        if let Err(error) = scheduled {
            if let Some((caller_scope, client_mutation_id, digest)) = receipt_context {
                if let Err(cleanup_error) = remove_receipt(
                    &self.runtime_store,
                    "agentQuota.consumeCodexResetCredit",
                    &caller_scope,
                    &client_mutation_id,
                    &digest,
                )
                .await
                {
                    tracing::error!(
                        client_mutation_id = %client_mutation_id,
                        "failed to roll back rejected Codex reset receipt: {cleanup_error}"
                    );
                }
            }
            return Err(error);
        }
        Ok(())
    }

    pub(super) fn handle_agent_quota_claude_tui_finished(
        &mut self,
        client_id: u64,
        request_id: i64,
        environment_signature: u64,
        result: HostResult<Value>,
    ) {
        match result {
            Ok(fresh) => {
                let Some(snapshot) = fresh.get("snapshot").cloned() else {
                    self.client_write(
                        client_id,
                        error_response(
                            request_id,
                            &HostError::state("Claude TUI response missing snapshot."),
                        ),
                    );
                    return;
                };
                let merged = upsert_quota_snapshot(
                    self.agent_quota_cache
                        .as_ref()
                        .filter(|(_, cached_signature, _)| {
                            *cached_signature == environment_signature
                        })
                        .map(|(_, _, cached)| cached),
                    snapshot,
                );
                self.agent_quota_cache =
                    Some((Instant::now(), environment_signature, merged.clone()));
                self.client_write(
                    client_id,
                    ok_response(
                        request_id,
                        json!({
                            "snapshot": fresh.get("snapshot").cloned().unwrap_or(Value::Null),
                            "snapshots": merged.get("snapshots").cloned().unwrap_or(json!([])),
                            "environment": merged.get("environment").cloned().unwrap_or(json!({})),
                        }),
                    ),
                );
                self.broadcast_authenticated(event("agentQuotasChanged", json!({})));
            }
            Err(error) => {
                self.client_write(client_id, error_response(request_id, &error));
            }
        }
    }

    pub(super) fn handle_agent_quota_finished(
        &mut self,
        client_id: u64,
        request_id: i64,
        environment_signature: u64,
        result: HostResult<Value>,
    ) {
        match result {
            Ok(fresh) => {
                let merged = self
                    .agent_quota_cache
                    .as_ref()
                    .filter(|(_, cached_signature, _)| *cached_signature == environment_signature)
                    .map(|(_, _, cached)| merge_quota_snapshots(cached, fresh.clone()))
                    .unwrap_or(fresh);
                self.agent_quota_cache =
                    Some((Instant::now(), environment_signature, merged.clone()));
                self.client_write(client_id, ok_response(request_id, merged));
                self.broadcast_authenticated(event("agentQuotasChanged", json!({})));
            }
            Err(error) => {
                if let Some((_, cached_signature, cached)) = &self.agent_quota_cache {
                    if *cached_signature != environment_signature {
                        self.client_write(client_id, error_response(request_id, &error));
                        return;
                    }
                    self.client_write(
                        client_id,
                        ok_response(
                            request_id,
                            mark_quota_payload_stale(cached, &error.wire_message()),
                        ),
                    );
                } else {
                    self.client_write(client_id, error_response(request_id, &error));
                }
            }
        }
    }

    pub(super) fn handle_agent_quota_codex_reset_finished(
        &mut self,
        client_id: u64,
        request_id: i64,
        environment_signature: u64,
        result: HostResult<Value>,
    ) {
        match result {
            Ok(mut payload) => {
                if let Some(snapshot) = payload.get("snapshot").cloned() {
                    let merged = upsert_quota_snapshot(
                        self.agent_quota_cache
                            .as_ref()
                            .filter(|(_, signature, _)| *signature == environment_signature)
                            .map(|(_, _, cached)| cached),
                        snapshot,
                    );
                    self.agent_quota_cache =
                        Some((Instant::now(), environment_signature, merged.clone()));
                    if let Some(object) = payload.as_object_mut() {
                        object.insert(
                            "snapshots".to_string(),
                            merged
                                .get("snapshots")
                                .cloned()
                                .unwrap_or_else(|| json!([])),
                        );
                    }
                }
                self.client_write(client_id, ok_response(request_id, payload));
                self.broadcast_authenticated(event("agentQuotasChanged", json!({})));
            }
            Err(error) => {
                self.client_write(client_id, error_response(request_id, &error));
            }
        }
    }
}

fn quota_environment_signature(value: &Value) -> u64 {
    let normalized = value
        .as_object()
        .map(|values| {
            let mut entries = values
                .iter()
                .filter_map(|(name, value)| value.as_str().map(|value| (name, value)))
                .collect::<Vec<_>>();
            entries.sort_unstable();
            entries
        })
        .unwrap_or_default();
    let mut hasher = DefaultHasher::new();
    normalized.hash(&mut hasher);
    hasher.finish()
}

fn merge_quota_snapshots(cached: &Value, mut fresh: Value) -> Value {
    let previous = cached
        .get("snapshots")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let Some(snapshots) = fresh.get_mut("snapshots").and_then(Value::as_array_mut) else {
        return fresh;
    };
    for snapshot in snapshots {
        if snapshot.get("status").and_then(Value::as_str) == Some("ok") {
            continue;
        }
        let provider = snapshot.get("provider");
        let account_id = snapshot.get("accountId");
        let Some(old) = previous.iter().find(|candidate| {
            candidate.get("provider") == provider && candidate.get("accountId") == account_id
        }) else {
            continue;
        };
        let error = snapshot.get("error").cloned().unwrap_or(Value::Null);
        *snapshot = old.clone();
        if let Some(object) = snapshot.as_object_mut() {
            object.insert("status".to_string(), json!("stale"));
            object.insert("error".to_string(), error);
        }
    }
    fresh
}

fn upsert_quota_snapshot(cached: Option<&Value>, snapshot: Value) -> Value {
    let mut base = cached.cloned().unwrap_or_else(|| {
        json!({
            "snapshots": [],
            "environment": {},
        })
    });
    let provider = snapshot.get("provider").cloned();
    let account_id = snapshot.get("accountId").cloned();
    if let Some(snapshots) = base.get_mut("snapshots").and_then(Value::as_array_mut) {
        if let Some(index) = snapshots.iter().position(|candidate| {
            candidate.get("provider") == provider.as_ref()
                && candidate.get("accountId") == account_id.as_ref()
        }) {
            snapshots[index] = snapshot;
        } else {
            snapshots.push(snapshot);
        }
    }
    base
}

fn mark_quota_payload_stale(cached: &Value, error: &str) -> Value {
    let mut value = cached.clone();
    if let Some(snapshots) = value.get_mut("snapshots").and_then(Value::as_array_mut) {
        for snapshot in snapshots {
            if let Some(object) = snapshot.as_object_mut() {
                object.insert("status".to_string(), json!("stale"));
                object.insert("error".to_string(), json!(error));
            }
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
