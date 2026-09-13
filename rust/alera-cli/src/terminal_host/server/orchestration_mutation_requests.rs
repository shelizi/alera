use alera_core::runtime::{
    NewOrchestrationMessage, OrchestrationMessagePriority, OrchestrationMessageType,
};
use serde_json::{json, Value};

use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::orchestration::group_resolution::{
    is_group_address, resolve_group_address,
};
use crate::terminal_host::orchestration::lifecycle_reconciliation::reconcile_lifecycle_message;

use super::orchestration_validation::{
    optional_string, parse_message_type, parse_priority, prefixed_subject, require_string,
    state_error,
};
use super::requests::idempotency_receipts::{
    optional_client_mutation_id, payload_digest, prepare_receipt, remove_receipt, settle_receipt,
    ReceiptPrepareOutcome,
};
use super::ServerActor;

impl ServerActor {
    pub(super) async fn orchestration_send(
        &mut self,
        client_id: u64,
        payload: &Value,
    ) -> HostResult<Value> {
        let client_mutation_id = optional_client_mutation_id(payload)?;
        let receipt_context = if let Some(client_mutation_id) = client_mutation_id {
            let caller_scope = self.agent_profile_launch_caller_scope(client_id)?;
            let digest =
                payload_digest(payload).map_err(|error| HostError::state(error.to_string()))?;
            match prepare_receipt(
                &self.runtime_store,
                "orchestration.send",
                &caller_scope,
                &client_mutation_id,
                &digest,
                None,
            )
            .await
            .map_err(|error| HostError::state(error.to_string()))?
            {
                ReceiptPrepareOutcome::Created => Some((caller_scope, client_mutation_id, digest)),
                ReceiptPrepareOutcome::Replay(result) => return Ok(result),
                ReceiptPrepareOutcome::Conflict => {
                    return Err(HostError::state(
                        "clientMutationId was already used with a different orchestration.send payload.",
                    ));
                }
                ReceiptPrepareOutcome::InProgress => {
                    return Err(HostError::state("clientMutationId is already in progress."));
                }
            }
        } else {
            None
        };

        let result: HostResult<Value> = async {
            let from_handle = require_string(payload, "from")?;
            let to = require_string(payload, "to")?;
            let subject = require_string(payload, "subject")?;
            let body = optional_string(payload, "body").unwrap_or_default();
            let message_type = parse_message_type(payload)?;
            let priority = parse_priority(payload)?;
            let message_payload = optional_string(payload, "payload");
            let explicit_thread_id = optional_string(payload, "threadId");

            if message_type.is_lifecycle() {
                return Err(HostError::state(format!(
                    "{} is a lifecycle operation; use heartbeat or complete instead",
                    message_type.as_str()
                )));
            }

            if message_type.is_lifecycle() && is_group_address(&to) {
                return Err(HostError::state(format!(
                    "{} messages cannot be sent to a group address",
                    message_type.as_str()
                )));
            }

            let recipients = resolve_group_address(
                &to,
                &from_handle,
                &self.group_resolution_terminals(),
                &self.agent_presence,
            );
            if recipients.is_empty() {
                return Err(HostError::state(format!("No recipients resolved for {to}")));
            }

            // Group fan-out shares a thread id so replies converge.
            let thread_id = if recipients.len() > 1 {
                explicit_thread_id.or_else(|| Some(generated_group_thread_id()))
            } else {
                explicit_thread_id
            };

            let mut inserted = Vec::new();
            for recipient in &recipients {
                let message = self
                    .runtime_store
                    .insert_orchestration_message(NewOrchestrationMessage {
                        from_handle: from_handle.clone(),
                        to_handle: recipient.clone(),
                        subject: subject.clone(),
                        body: body.clone(),
                        message_type,
                        priority,
                        thread_id: thread_id.clone(),
                        payload: message_payload.clone(),
                        run_id: optional_string(payload, "runId"),
                        workspace_id: optional_string(payload, "workspaceId"),
                        task_id: optional_string(payload, "taskId"),
                        dispatch_id: optional_string(payload, "dispatchId"),
                        expires_at: optional_string(payload, "expiresAt"),
                    })
                    .await
                    .map_err(state_error)?;
                inserted.push(message);
            }

            // Lifecycle messages reconcile BEFORE waking waiters so the dispatch
            // lock is released by the time the coordinator reads the result.
            if message_type.is_lifecycle() {
                for message in &inserted {
                    let mut log = |line: String| tracing::info!("[orchestration] {line}");
                    let _ =
                        reconcile_lifecycle_message(&self.runtime_store, message, &mut log).await;
                }
            }

            for recipient in &recipients {
                self.deliver_pending_messages_if_idle(recipient).await;
                self.notify_message_arrived(recipient, message_type).await;
            }
            self.broadcast_authenticated(crate::terminal_host::protocol::event(
                "orchestrationMessagesChanged",
                json!({}),
            ));

            Ok(json!({
                "messages": inserted,
                "recipients": recipients,
            }))
        }
        .await;
        match result {
            Ok(result) => {
                if let Some((caller_scope, client_mutation_id, digest)) = receipt_context {
                    settle_receipt(
                        &self.runtime_store,
                        "orchestration.send",
                        &caller_scope,
                        &client_mutation_id,
                        &digest,
                        &result,
                    )
                    .await
                    .map_err(|error| HostError::state(error.to_string()))?;
                }
                Ok(result)
            }
            Err(error) => {
                if let Some((caller_scope, client_mutation_id, digest)) = receipt_context {
                    if let Err(cleanup_error) = remove_receipt(
                        &self.runtime_store,
                        "orchestration.send",
                        &caller_scope,
                        &client_mutation_id,
                        &digest,
                    )
                    .await
                    {
                        tracing::error!(
                            client_mutation_id = %client_mutation_id,
                            "failed to roll back orchestration.send receipt: {cleanup_error}"
                        );
                    }
                }
                Err(error)
            }
        }
    }

    pub(super) async fn orchestration_reply(
        &mut self,
        client_id: u64,
        payload: &Value,
    ) -> HostResult<Value> {
        let client_mutation_id = optional_client_mutation_id(payload)?;
        let receipt_context = if let Some(client_mutation_id) = client_mutation_id {
            let caller_scope = self.agent_profile_launch_caller_scope(client_id)?;
            let digest =
                payload_digest(payload).map_err(|error| HostError::state(error.to_string()))?;
            match prepare_receipt(
                &self.runtime_store,
                "orchestration.reply",
                &caller_scope,
                &client_mutation_id,
                &digest,
                None,
            )
            .await
            .map_err(|error| HostError::state(error.to_string()))?
            {
                ReceiptPrepareOutcome::Created => Some((caller_scope, client_mutation_id, digest)),
                ReceiptPrepareOutcome::Replay(result) => return Ok(result),
                ReceiptPrepareOutcome::Conflict => {
                    return Err(HostError::state(
                        "clientMutationId was already used with a different orchestration.reply payload.",
                    ));
                }
                ReceiptPrepareOutcome::InProgress => {
                    return Err(HostError::state("clientMutationId is already in progress."));
                }
            }
        } else {
            None
        };

        let result: HostResult<Value> = async {
            let message_id = require_string(payload, "id")?;
            let body = require_string(payload, "body")?;
            let original = self
                .runtime_store
                .orchestration_message_by_id(&message_id)
                .await
                .map_err(state_error)?
                .ok_or_else(|| HostError::state(format!("message not found: {message_id}")))?;
            self.runtime_store
                .mark_orchestration_messages_read(std::slice::from_ref(&original.id))
                .await
                .map_err(state_error)?;
            let reply = self
                .runtime_store
                .insert_orchestration_message(NewOrchestrationMessage {
                    from_handle: original.to_handle.clone(),
                    to_handle: original.from_handle.clone(),
                    subject: prefixed_subject("Re: ", &original.subject),
                    body,
                    message_type: OrchestrationMessageType::Status,
                    priority: OrchestrationMessagePriority::Normal,
                    thread_id: original
                        .thread_id
                        .clone()
                        .or_else(|| Some(original.id.clone())),
                    payload: None,
                    run_id: original.run_id.clone(),
                    workspace_id: original.workspace_id.clone(),
                    task_id: original.task_id.clone(),
                    dispatch_id: original.dispatch_id.clone(),
                    expires_at: None,
                })
                .await
                .map_err(state_error)?;
            let recipient = reply.to_handle.clone();
            self.deliver_pending_messages_if_idle(&recipient).await;
            self.notify_message_arrived(&recipient, OrchestrationMessageType::Status)
                .await;
            Ok(json!(reply))
        }
        .await;
        match result {
            Ok(result) => {
                if let Some((caller_scope, client_mutation_id, digest)) = receipt_context {
                    settle_receipt(
                        &self.runtime_store,
                        "orchestration.reply",
                        &caller_scope,
                        &client_mutation_id,
                        &digest,
                        &result,
                    )
                    .await
                    .map_err(|error| HostError::state(error.to_string()))?;
                }
                Ok(result)
            }
            Err(error) => {
                if let Some((caller_scope, client_mutation_id, digest)) = receipt_context {
                    if let Err(cleanup_error) = remove_receipt(
                        &self.runtime_store,
                        "orchestration.reply",
                        &caller_scope,
                        &client_mutation_id,
                        &digest,
                    )
                    .await
                    {
                        tracing::error!(
                            client_mutation_id = %client_mutation_id,
                            "failed to roll back orchestration.reply receipt: {cleanup_error}"
                        );
                    }
                }
                Err(error)
            }
        }
    }
}

pub(super) fn generated_group_thread_id() -> String {
    format!("thread_{}", uuid::Uuid::new_v4().simple())
}
