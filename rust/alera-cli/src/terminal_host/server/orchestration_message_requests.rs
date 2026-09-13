use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use alera_core::runtime::{
    NewOrchestrationMessage, OrchestrationMessage, OrchestrationMessagePriority,
    OrchestrationMessageType,
};
use serde_json::{json, Value};

use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::orchestration::group_resolution::is_group_address;
use crate::terminal_host::orchestration::lifecycle_reconciliation::reconcile_lifecycle_message;
use crate::terminal_host::orchestration::message_formatter::format_messages_for_injection;
use crate::terminal_host::orchestration::message_waiters::WaitKind;

use super::orchestration_validation::{
    optional_string, parse_type_filter, require_string, state_error, wait_timeout_ms,
};
use super::requests::idempotency_receipts::{
    optional_client_mutation_id, payload_digest, prepare_receipt, remove_receipt,
    ReceiptPrepareOutcome,
};
use super::ServerActor;

#[derive(Clone, Debug)]
pub(super) struct AskReceiptContext {
    pub(super) caller_scope: String,
    pub(super) client_mutation_id: String,
    pub(super) payload_digest: String,
}

// WaitKind stays receipt-agnostic, so keyed ask waiters keep their receipt link here.
static ASK_RECEIPT_CONTEXTS: LazyLock<Mutex<HashMap<String, AskReceiptContext>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn ask_receipt_contexts() -> std::sync::MutexGuard<'static, HashMap<String, AskReceiptContext>> {
    ASK_RECEIPT_CONTEXTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(super) fn insert_ask_receipt_context(thread_id: String, context: AskReceiptContext) {
    ask_receipt_contexts().insert(thread_id, context);
}

pub(super) fn take_ask_receipt_context(thread_id: &str) -> Option<AskReceiptContext> {
    ask_receipt_contexts().remove(thread_id)
}

impl ServerActor {
    // --- check ------------------------------------------------------------

    pub(super) async fn orchestration_check(
        &mut self,
        client_id: u64,
        request_id: i64,
        payload: &Value,
    ) -> HostResult<Option<Value>> {
        let handle = require_string(payload, "terminal")?;
        let all = payload.get("all").and_then(Value::as_bool).unwrap_or(false);
        let wait = payload
            .get("wait")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let inject = payload
            .get("inject")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let type_filter = parse_type_filter(payload)?;

        if all {
            // `--all` is a read-only view: never marks read, never waits.
            let limit = payload.get("limit").and_then(Value::as_i64).unwrap_or(100);
            let filter = if type_filter.is_empty() {
                None
            } else {
                Some(type_filter.as_slice())
            };
            let messages = self
                .runtime_store
                .all_orchestration_messages_for_handle(&handle, filter, limit)
                .await
                .map_err(state_error)?;
            return Ok(Some(check_response(&messages, inject)));
        }

        let messages = self.consume_unread_messages(&handle, &type_filter).await?;
        if !messages.is_empty() || !wait {
            return Ok(Some(check_response(&messages, inject)));
        }

        // Nothing unread and the caller asked to wait: park the request.
        let waiter_id = self.orchestration_waiters.register(
            client_id,
            request_id,
            handle,
            WaitKind::Check {
                type_filter,
                inject,
            },
        );
        if let Err(error) = self.spawn_wait_timeout(waiter_id, wait_timeout_ms(payload), client_id)
        {
            self.orchestration_waiters.take_by_id(waiter_id);
            return Err(error);
        }
        Ok(None)
    }

    /// Reads unread messages (optionally filtered), reconciles lifecycle
    /// messages inline, and marks them read.
    pub(super) async fn consume_unread_messages(
        &mut self,
        handle: &str,
        type_filter: &[OrchestrationMessageType],
    ) -> HostResult<Vec<OrchestrationMessage>> {
        let filter = if type_filter.is_empty() {
            None
        } else {
            Some(type_filter)
        };
        let messages = self
            .runtime_store
            .unread_orchestration_messages(handle, filter)
            .await
            .map_err(state_error)?;
        if messages.is_empty() {
            return Ok(messages);
        }
        for message in &messages {
            if message.message_type.is_lifecycle() {
                let mut log = |line: String| tracing::info!("[orchestration] {line}");
                let _ = reconcile_lifecycle_message(&self.runtime_store, message, &mut log).await;
            }
        }
        let ids: Vec<String> = messages.iter().map(|message| message.id.clone()).collect();
        self.runtime_store
            .mark_orchestration_messages_read(&ids)
            .await
            .map_err(state_error)?;
        Ok(messages)
    }

    // --- inbox ------------------------------------------------------------

    pub(super) async fn orchestration_inbox(&mut self, payload: &Value) -> HostResult<Value> {
        let limit = payload.get("limit").and_then(Value::as_i64).unwrap_or(50);
        let direction =
            optional_string(payload, "direction").unwrap_or_else(|| "inbox".to_string());
        let terminal_filter = optional_string(payload, "terminal");
        let messages = match (terminal_filter.clone(), direction.as_str()) {
            (Some(handle), "outbox") => self
                .runtime_store
                .all_orchestration_messages_from_handle(&handle, limit)
                .await
                .map_err(state_error)?,
            (Some(handle), _) => self
                .runtime_store
                .all_orchestration_messages_for_handle(&handle, None, limit)
                .await
                .map_err(state_error)?,
            (None, "outbox") => {
                return Err(HostError::format("--terminal is required for outbox."))
            }
            (None, _) => self
                .runtime_store
                .orchestration_inbox(limit)
                .await
                .map_err(state_error)?,
        };
        Ok(
            json!({ "kind": "messages", "items": messages, "messages": messages, "filters": { "terminal": terminal_filter, "direction": direction } }),
        )
    }

    // --- ask --------------------------------------------------------------

    pub(super) async fn orchestration_ask(
        &mut self,
        client_id: u64,
        request_id: i64,
        payload: &Value,
    ) -> HostResult<Option<Value>> {
        let client_mutation_id = optional_client_mutation_id(payload)?;
        let receipt_context = if let Some(client_mutation_id) = client_mutation_id {
            let caller_scope = self.agent_profile_launch_caller_scope(client_id)?;
            let digest =
                payload_digest(payload).map_err(|error| HostError::state(error.to_string()))?;
            match prepare_receipt(
                &self.runtime_store,
                "orchestration.ask",
                &caller_scope,
                &client_mutation_id,
                &digest,
                None,
            )
            .await
            .map_err(|error| HostError::state(error.to_string()))?
            {
                ReceiptPrepareOutcome::Created => Some(AskReceiptContext {
                    caller_scope,
                    client_mutation_id,
                    payload_digest: digest,
                }),
                ReceiptPrepareOutcome::Replay(result) => return Ok(Some(result)),
                ReceiptPrepareOutcome::Conflict => {
                    return Err(HostError::state(
                        "clientMutationId was already used with a different orchestration.ask payload.",
                    ));
                }
                ReceiptPrepareOutcome::InProgress => {
                    return Err(HostError::state("clientMutationId is already in progress."));
                }
            }
        } else {
            None
        };

        let mut ask_thread_id = None;
        let result: HostResult<Option<Value>> = async {
            let from_handle = require_string(payload, "from")?;
            let requested_to = require_string(payload, "to")?;
            let active_dispatch = self
                .runtime_store
                .active_orchestration_dispatch_for_handle(&from_handle)
                .await
                .map_err(state_error)?;
            let to = active_dispatch
                .as_ref()
                .map(|dispatch| dispatch.coordinator_handle.clone())
                .unwrap_or(requested_to);
            if is_group_address(&to) {
                return Err(HostError::state(
                    "ask requires a single terminal handle, not a group address",
                ));
            }
            let question = require_string(payload, "question")?;
            let options = optional_string(payload, "options");
            let body = match &options {
                Some(options) => format!("{question}\nOptions: {options}"),
                None => question.clone(),
            };
            let message = self
                .runtime_store
                .insert_orchestration_message(NewOrchestrationMessage {
                    from_handle: from_handle.clone(),
                    to_handle: to.clone(),
                    subject: format!("Question: {question}"),
                    body,
                    message_type: OrchestrationMessageType::DecisionGate,
                    priority: OrchestrationMessagePriority::High,
                    thread_id: None,
                    payload: None,
                    run_id: active_dispatch
                        .as_ref()
                        .and_then(|dispatch| dispatch.run_id.clone())
                        .or_else(|| optional_string(payload, "runId")),
                    workspace_id: active_dispatch
                        .as_ref()
                        .map(|dispatch| dispatch.workspace_id.clone())
                        .or_else(|| optional_string(payload, "workspaceId")),
                    task_id: active_dispatch
                        .as_ref()
                        .map(|dispatch| dispatch.task_id.clone())
                        .or_else(|| optional_string(payload, "taskId")),
                    dispatch_id: active_dispatch
                        .as_ref()
                        .map(|dispatch| dispatch.id.clone())
                        .or_else(|| optional_string(payload, "dispatchId")),
                    expires_at: optional_string(payload, "expiresAt"),
                })
                .await
                .map_err(state_error)?;
            ask_thread_id = Some(message.id.clone());
            if let Some(context) = receipt_context.as_ref() {
                insert_ask_receipt_context(message.id.clone(), context.clone());
            }
            // The question is its own thread root: replies inherit thread_id =
            // message id via `reply`.
            self.deliver_pending_messages_if_idle(&to).await;
            self.notify_message_arrived(&to, OrchestrationMessageType::DecisionGate)
                .await;

            let waiter_id = self.orchestration_waiters.register(
                client_id,
                request_id,
                from_handle,
                WaitKind::Ask {
                    thread_id: message.id.clone(),
                    after_sequence: message.sequence,
                },
            );
            if let Err(error) =
                self.spawn_wait_timeout(waiter_id, wait_timeout_ms(payload), client_id)
            {
                self.orchestration_waiters.take_by_id(waiter_id);
                return Err(error);
            }
            Ok(None)
        }
        .await;
        match result {
            Ok(result) => Ok(result),
            Err(error) => {
                if let Some(thread_id) = ask_thread_id {
                    let _ = take_ask_receipt_context(&thread_id);
                }
                if let Some(context) = receipt_context {
                    if let Err(cleanup_error) = remove_receipt(
                        &self.runtime_store,
                        "orchestration.ask",
                        &context.caller_scope,
                        &context.client_mutation_id,
                        &context.payload_digest,
                    )
                    .await
                    {
                        tracing::error!(
                            client_mutation_id = %context.client_mutation_id,
                            "failed to roll back orchestration.ask receipt: {cleanup_error}"
                        );
                    }
                }
                Err(error)
            }
        }
    }
}

pub(super) fn check_response(messages: &[OrchestrationMessage], inject: bool) -> Value {
    let mut response = json!({ "messages": messages });
    if inject {
        response["formatted"] = Value::String(format_messages_for_injection(messages));
    }
    response
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::super::orchestration_mutation_requests::generated_group_thread_id;

    #[test]
    fn generated_group_thread_ids_are_unique() {
        let ids = (0..512)
            .map(|_| generated_group_thread_id())
            .collect::<HashSet<_>>();

        assert_eq!(ids.len(), 512);
    }
}
