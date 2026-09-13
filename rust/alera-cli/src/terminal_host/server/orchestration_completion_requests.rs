use alera_core::runtime::{
    NewOrchestrationMessage, OrchestrationDispatchContext, OrchestrationMessagePriority,
    OrchestrationMessageType,
};
use serde_json::{json, Value};

use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::orchestration::dispatch_preamble::{build_worker_contract, WorkerKind};

use super::orchestration_validation::{
    optional_string, require_string, state_error, validate_result_schema,
};
use super::requests::idempotency_receipts::{
    optional_client_mutation_id, payload_digest, prepare_receipt, remove_receipt, settle_receipt,
    ReceiptPrepareOutcome,
};
use super::ServerActor;

impl ServerActor {
    pub(super) async fn orchestration_context(&mut self, payload: &Value) -> HostResult<Value> {
        let dispatch = self.active_worker_dispatch(payload).await?;
        let mut task = self
            .runtime_store
            .orchestration_task_by_id(&dispatch.task_id)
            .await
            .map_err(state_error)?;
        let mut base_drift = Value::Null;
        if let Some((task_spec, stored_drift)) =
            self.orchestration_preflight_context(&dispatch).await?
        {
            if let Some(task) = task.as_mut() {
                task.spec = task_spec;
            }
            base_drift = stored_drift;
        }
        self.compose_orchestration_task_prompt(&mut task, dispatch.agent_profile.as_deref())
            .await?;
        let gate_resolution = self.latest_resolved_gate(&dispatch.task_id).await?;
        let worker_instructions = build_worker_contract(
            &dispatch.coordinator_handle,
            WorkerKind::PromptReturningAgent,
        );
        Ok(json!({
            "task": task,
            "dispatch": dispatch,
            "baseDrift": base_drift,
            "gateResolution": gate_resolution.map(|gate| json!({
                "question": gate.question,
                "resolution": gate.resolution,
            })),
            "workerInstructions": worker_instructions,
            "coordinatorHandle": dispatch.coordinator_handle,
            "assigneeHandle": dispatch.assignee_handle,
            "phase": dispatch.status.as_str(),
            "lastActivityAt": dispatch.last_activity_at,
            "completionState": dispatch.status.as_str(),
        }))
    }

    pub(super) async fn orchestration_preflight_context(
        &self,
        dispatch: &OrchestrationDispatchContext,
    ) -> HostResult<Option<(String, Value)>> {
        let Some(handle) = dispatch.assignee_handle.as_deref() else {
            return Ok(None);
        };
        let Some(tab) = self
            .runtime_store
            .find_workspace_tab(handle)
            .await
            .map_err(state_error)?
        else {
            return Ok(None);
        };
        let Some(preflight) = tab.payload.get("orchestrationPreflight") else {
            return Ok(None);
        };
        if preflight.get("taskId").and_then(Value::as_str) != Some(dispatch.task_id.as_str())
            || preflight.get("dispatchId").and_then(Value::as_str) != Some(dispatch.id.as_str())
        {
            return Ok(None);
        }
        let Some(task_spec) = preflight.get("taskSpec").and_then(Value::as_str) else {
            return Ok(None);
        };
        Ok(Some((
            task_spec.to_string(),
            preflight.get("baseDrift").cloned().unwrap_or(Value::Null),
        )))
    }

    pub(super) async fn orchestration_heartbeat(&mut self, payload: &Value) -> HostResult<Value> {
        let dispatch = self.active_worker_dispatch(payload).await?;
        let accepted = self
            .runtime_store
            .record_orchestration_activity(&dispatch.id)
            .await
            .map_err(state_error)?;
        if !accepted {
            return Err(HostError::state("heartbeat rejected for inactive dispatch"));
        }
        Ok(
            json!({ "lifecycleAccepted": true, "dispatchId": dispatch.id, "phase": optional_string(payload, "phase") }),
        )
    }

    pub(super) async fn orchestration_escalate(
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
                "orchestration.escalate",
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
                        "clientMutationId was already used with a different orchestration.escalate payload.",
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
            let dispatch = self.active_worker_dispatch(payload).await?;
            let assignee = dispatch.assignee_handle.clone().unwrap_or_default();
            let message = self
                .runtime_store
                .insert_orchestration_message(NewOrchestrationMessage {
                    from_handle: assignee,
                    to_handle: dispatch.coordinator_handle.clone(),
                    subject: require_string(payload, "subject")?,
                    body: optional_string(payload, "body").unwrap_or_default(),
                    message_type: OrchestrationMessageType::Escalation,
                    priority: OrchestrationMessagePriority::High,
                    thread_id: None,
                    payload: Some(
                        json!({
                            "taskId": dispatch.task_id,
                            "dispatchId": dispatch.id,
                        })
                        .to_string(),
                    ),
                    run_id: dispatch.run_id.clone(),
                    workspace_id: Some(dispatch.workspace_id.clone()),
                    task_id: Some(dispatch.task_id.clone()),
                    dispatch_id: Some(dispatch.id.clone()),
                    expires_at: None,
                })
                .await
                .map_err(state_error)?;
            self.runtime_store
                .record_orchestration_activity(&dispatch.id)
                .await
                .map_err(state_error)?;
            self.queue_escalation_push(&dispatch.task_id, &message.subject)
                .await;
            self.notify_message_arrived(
                &dispatch.coordinator_handle,
                OrchestrationMessageType::Escalation,
            )
            .await;
            Ok(json!({ "lifecycleAccepted": true, "message": message }))
        }
        .await;
        match result {
            Ok(result) => {
                if let Some((caller_scope, client_mutation_id, digest)) = receipt_context {
                    settle_receipt(
                        &self.runtime_store,
                        "orchestration.escalate",
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
                        "orchestration.escalate",
                        &caller_scope,
                        &client_mutation_id,
                        &digest,
                    )
                    .await
                    {
                        tracing::error!(
                            client_mutation_id = %client_mutation_id,
                            "failed to roll back orchestration.escalate receipt: {cleanup_error}"
                        );
                    }
                }
                Err(error)
            }
        }
    }

    pub(super) async fn orchestration_complete(&mut self, payload: &Value) -> HostResult<Value> {
        let (dispatch, completion_replay) = self.completion_worker_dispatch(payload).await?;
        let assignee = dispatch
            .assignee_handle
            .clone()
            .ok_or_else(|| HostError::state("active dispatch has no assignee"))?;
        let result = payload
            .get("result")
            .and_then(Value::as_object)
            .ok_or_else(|| HostError::format("result must be an object."))?;
        let summary = result
            .get("summary")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| HostError::format("result.summary is required."))?;
        let completion_kind = result
            .get("completionKind")
            .and_then(Value::as_str)
            .unwrap_or("success");
        if completion_replay && completion_kind != "success" {
            return Err(HostError::state(format!(
                "no active dispatch for terminal {assignee}"
            )));
        }
        for field in ["artifacts", "filesModified", "validation"] {
            if !result.get(field).is_some_and(Value::is_array) {
                return Err(HostError::format(format!(
                    "result.{field} must be an array."
                )));
            }
        }
        let task = self
            .runtime_store
            .orchestration_task_by_id(&dispatch.task_id)
            .await
            .map_err(state_error)?
            .ok_or_else(|| HostError::state("dispatch task no longer exists"))?;
        validate_result_schema(result, task.result_schema.as_deref())?;
        let result_json =
            serde_json::to_string(result).map_err(|error| HostError::format(error.to_string()))?;
        if completion_kind == "failure" {
            self.remove_dispatch_context(&assignee);
            let failed = self
                .runtime_store
                .fail_orchestration_dispatch_with_result(&dispatch.id, summary, Some(&result_json))
                .await
                .map_err(state_error)?;
            return Ok(json!({
                "lifecycleAccepted": true,
                "taskId": dispatch.task_id,
                "dispatchId": failed.id,
                "dispatchStatus": failed.status.as_str(),
            }));
        }
        if completion_kind != "success" {
            return Err(HostError::format(
                "result.completionKind must be success or failure.",
            ));
        }
        let completed = self
            .runtime_store
            .complete_orchestration_dispatch(&dispatch.id, &assignee, &result_json)
            .await
            .map_err(state_error)?;
        if !completion_replay {
            self.apply_terminal_completion_policy(&assignee, &completed.terminal_policy)
                .await?;
        }
        Ok(json!({
            "delivered": true,
            "lifecycleAccepted": true,
            "taskId": completed.task_id,
            "dispatchId": completed.id,
            "taskStatus": "completed",
            "dispatchStatus": completed.status.as_str(),
        }))
    }

    pub(super) async fn orchestration_worker_done(&mut self, payload: &Value) -> HostResult<Value> {
        let terminal = optional_string(payload, "terminal")
            .ok_or_else(|| HostError::format("terminal is required."))?;
        let task_id = require_string(payload, "task")?;
        let dispatch_id = require_string(payload, "dispatch")?;
        let dispatch = self
            .runtime_store
            .orchestration_dispatch_by_id(&dispatch_id)
            .await
            .map_err(state_error)?
            .ok_or_else(|| HostError::state(format!("dispatch not found: {dispatch_id}")))?;
        if dispatch.task_id != task_id || dispatch.assignee_handle.as_deref() != Some(&terminal) {
            return Err(HostError::state("worker-done authority rejected"));
        }
        let result = payload
            .get("result")
            .and_then(Value::as_object)
            .ok_or_else(|| HostError::format("result must be an object."))?;
        result
            .get("summary")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| HostError::format("result.summary is required."))?;
        let completion_kind = result
            .get("completionKind")
            .and_then(Value::as_str)
            .unwrap_or("success");
        if completion_kind != "success" {
            return Err(HostError::format(
                "worker-done result.completionKind must be success.",
            ));
        }
        for field in ["artifacts", "filesModified", "validation"] {
            if !result.get(field).is_some_and(Value::is_array) {
                return Err(HostError::format(format!(
                    "result.{field} must be an array."
                )));
            }
        }
        let task = self
            .runtime_store
            .orchestration_task_by_id(&task_id)
            .await
            .map_err(state_error)?
            .ok_or_else(|| HostError::state("dispatch task no longer exists"))?;
        validate_result_schema(result, task.result_schema.as_deref())?;
        let result_json =
            serde_json::to_string(result).map_err(|error| HostError::format(error.to_string()))?;
        let completed = self
            .runtime_store
            .complete_orchestration_dispatch(&dispatch_id, &terminal, &result_json)
            .await
            .map_err(state_error)?;
        self.apply_terminal_completion_policy(&terminal, &completed.terminal_policy)
            .await?;
        Ok(json!({
            "delivered": true,
            "lifecycleAccepted": true,
            "taskId": completed.task_id,
            "dispatchId": completed.id,
            "taskStatus": "completed",
            "dispatchStatus": completed.status.as_str(),
        }))
    }

    async fn apply_terminal_completion_policy(
        &mut self,
        handle: &str,
        policy: &str,
    ) -> HostResult<()> {
        match policy {
            "return-to-shell" => {
                let _ = self.queue_orchestration_control(handle, b"\x04");
            }
            "close-on-success" => {
                if let Some(tab_id) = self
                    .sessions
                    .get(handle)
                    .map(|session| session.tab_id.clone())
                {
                    self.runtime_store
                        .remove_workspace_tab(&tab_id)
                        .await
                        .map_err(state_error)?;
                    self.terminate_sessions_for_tab(&tab_id).await;
                    self.broadcast_authenticated(crate::terminal_host::protocol::event(
                        "workspaceTabsChanged",
                        json!({}),
                    ));
                } else {
                    self.agent_presence.remove(handle);
                    self.schedule_shutdown_if_idle();
                }
            }
            _ => {}
        }
        Ok(())
    }
}
