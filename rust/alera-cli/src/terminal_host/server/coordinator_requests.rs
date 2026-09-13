use alera_core::agent_descriptor::agent_descriptor;
use alera_core::runtime::{
    OrchestrationCoordinatorStatus, OrchestrationGateStatus, OrchestrationMessageType,
    OrchestrationTaskStatus,
};
use serde_json::{json, Value};

use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::orchestration::coordinator_loop::{
    acceptance_timeout_threshold_iso, hung_dispatch_threshold_iso, CoordinatorConfig,
    CoordinatorHandle, COORDINATOR_DEFAULT_POLL_MS, COORDINATOR_MAX_CONCURRENT_DEFAULT,
};
use crate::terminal_host::orchestration::lifecycle_reconciliation::{
    reconcile_lifecycle_message, LifecycleReconciliation,
};
use crate::terminal_host::protocol::event;

use super::{ServerActor, ServerCommand};

fn state_error(error: impl std::fmt::Display) -> HostError {
    HostError::state(error.to_string())
}

fn payload_value(message: &alera_core::runtime::OrchestrationMessage) -> Option<Value> {
    message
        .payload
        .as_deref()
        .and_then(|payload| serde_json::from_str::<Value>(payload).ok())
}

fn payload_string(payload: Option<&Value>, key: &str) -> Option<String> {
    payload?
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

impl ServerActor {
    pub(super) async fn orchestration_run(&mut self, payload: &Value) -> HostResult<Value> {
        let spec = payload
            .get("spec")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| HostError::format("spec is required."))?;
        let coordinator_handle = payload
            .get("from")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .ok_or_else(|| HostError::format("from is required."))?;
        let deliveries = &self.orchestration_delivery_in_flight;
        if deliveries.contains(&coordinator_handle) {
            return Err(HostError::state("prompt delivery in flight; retry"));
        }
        let poll_interval_ms = payload
            .get("pollIntervalMs")
            .and_then(Value::as_u64)
            .unwrap_or(COORDINATOR_DEFAULT_POLL_MS);
        let max_concurrent = payload
            .get("maxConcurrent")
            .and_then(Value::as_u64)
            .map(|value| value.max(1) as usize)
            .unwrap_or(COORDINATOR_MAX_CONCURRENT_DEFAULT);
        let workspace_id = payload
            .get("workspace")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| "global".to_string());
        let agent_type = payload
            .get("agent")
            .and_then(Value::as_str)
            .unwrap_or("claude")
            .to_string();
        if agent_descriptor(&agent_type).is_none() {
            return Err(HostError::format(format!(
                "unsupported agent type: {agent_type}"
            )));
        }
        if self
            .coordinators
            .values()
            .any(|handle| handle.config.workspace_id.as_deref() == Some(workspace_id.as_str()))
        {
            return Err(HostError::state(format!(
                "a coordinator run is already active for workspace {workspace_id}"
            )));
        }
        // Decomposition is the caller's responsibility in v1: the coordinator
        // manages the DAG, it does not invent it.
        let tasks = self
            .runtime_store
            .list_scoped_orchestration_tasks(None, None, None)
            .await
            .map_err(state_error)?;
        if tasks.is_empty() {
            return Err(HostError::state(
                "no orchestration tasks exist; create tasks with task-create before run",
            ));
        }
        let run = self
            .runtime_store
            .create_scoped_orchestration_coordinator_run(
                spec,
                Some(&coordinator_handle),
                poll_interval_ms as i64,
                &workspace_id,
                max_concurrent as i64,
            )
            .await
            .map_err(state_error)?;
        let bound = self
            .runtime_store
            .bind_manual_tasks_to_run(&run.id, &workspace_id, &coordinator_handle)
            .await
            .map_err(state_error)?;
        if bound == 0 {
            self.runtime_store
                .finish_orchestration_coordinator_run(
                    &run.id,
                    OrchestrationCoordinatorStatus::Failed,
                )
                .await
                .map_err(state_error)?;
            return Err(HostError::state(
                "no unowned tasks in this workspace belong to the coordinator",
            ));
        }
        let config = CoordinatorConfig {
            run_id: run.id.clone(),
            coordinator_handle: Some(coordinator_handle),
            poll_interval_ms,
            max_concurrent,
            workspace_id: Some(workspace_id),
            agent_type,
        };
        self.coordinators.insert(
            run.id.clone(),
            CoordinatorHandle::start(config, self.inbox.clone(), |run_id| {
                ServerCommand::CoordinatorTick { run_id }
            }),
        );
        self.cancel_shutdown_timer();
        Ok(json!({ "runId": run.id, "status": "running" }))
    }

    pub(super) async fn orchestration_run_stop(&mut self, payload: &Value) -> HostResult<Value> {
        let requested_run_id = payload
            .get("id")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let run_id = match requested_run_id {
            Some(id) => id,
            None if self.coordinators.len() == 1 => {
                self.coordinators.keys().next().cloned().unwrap()
            }
            None => {
                return Err(HostError::format(
                    "id is required when zero or multiple runs are active.",
                ))
            }
        };
        let run_id = run_id.as_str();
        let run = self
            .runtime_store
            .orchestration_coordinator_run_by_id(run_id)
            .await
            .map_err(state_error)?
            .filter(|run| {
                matches!(
                    run.status,
                    OrchestrationCoordinatorStatus::Running
                        | OrchestrationCoordinatorStatus::Stopping
                )
            })
            .ok_or_else(|| HostError::state(format!("coordinator run is not active: {run_id}")))?;
        let actor = payload
            .get("actor")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty());
        let force = payload
            .get("force")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !force && actor != run.coordinator_handle.as_deref() {
            return Err(HostError::state(format!(
                "only coordinator {} can stop run {run_id}; use --force for audited recovery",
                run.coordinator_handle.as_deref().unwrap_or("<unassigned>")
            )));
        }
        let handle = self.coordinators.remove(run_id);
        if let Some(handle) = handle.as_ref() {
            handle.stop();
        }
        let config_workspace = Some(run.workspace_id.clone());
        let reason = payload
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("coordinator stopped");
        if payload
            .get("cancelActive")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            let tasks = self
                .runtime_store
                .list_scoped_orchestration_tasks(None, Some(run_id), config_workspace.as_deref())
                .await
                .map_err(state_error)?;
            for task in tasks.into_iter().filter(|task| {
                matches!(
                    task.status,
                    OrchestrationTaskStatus::Dispatched | OrchestrationTaskStatus::Stalled
                )
            }) {
                self.orchestration_task_cancel(&json!({
                    "id": task.id,
                    "reason": reason,
                    "actor": actor,
                    "force": force,
                }))
                .await?;
            }
        }
        self.runtime_store
            .stop_orchestration_coordinator_run(run_id, reason)
            .await
            .map_err(state_error)?;
        self.runtime_store
            .insert_orchestration_audit_event(
                actor,
                if force { "run.stop.force" } else { "run.stop" },
                run_id,
                reason,
            )
            .await
            .map_err(state_error)?;
        self.schedule_shutdown_if_idle();
        Ok(json!({ "runId": run_id, "status": "stopped" }))
    }

    // --- tick ---------------------------------------------------------------

    /// One coordinator tick, executed inside the actor. Order mirrors Orca:
    /// process messages -> re-assert gate blocks -> warn stale dispatches ->
    /// dispatch ready tasks -> check convergence.
    pub(super) async fn handle_coordinator_tick(&mut self, run_id: String) {
        if self.mutation_queue.has_runtime_mutations() {
            return;
        }
        let Some(handle) = self.coordinators.get(&run_id) else {
            return;
        };
        let config = handle.config.clone();

        if let Err(error) = self.coordinator_process_messages(&config).await {
            self.coordinator_log(&format!("message processing failed: {error}"));
        }
        if let Err(error) = self.coordinator_assert_gate_blocks().await {
            self.coordinator_log(&format!("gate check failed: {error}"));
        }
        if let Err(error) = self.coordinator_warn_stale_dispatches().await {
            self.coordinator_log(&format!("stale check failed: {error}"));
        }
        if let Err(error) = self.coordinator_dispatch_ready_tasks(&config).await {
            self.coordinator_log(&format!("dispatch failed: {error}"));
        }
        match self.coordinator_check_convergence(&config).await {
            Ok(Some(final_status)) => {
                let _ = self
                    .runtime_store
                    .finish_orchestration_coordinator_run(&run_id, final_status)
                    .await;
                if let Some(handle) = self.coordinators.remove(&run_id) {
                    handle.stop();
                }
                self.coordinator_log(&format!(
                    "coordinator run {run_id} finished: {}",
                    final_status.as_str()
                ));
                self.schedule_shutdown_if_idle();
            }
            Ok(None) => {}
            Err(error) => self.coordinator_log(&format!("convergence check failed: {error}")),
        }
    }

    /// Coordinator inbox: worker_done/heartbeat reconcile, escalations fail
    /// the dispatch (circuit breaker), decision_gate messages create gates.
    async fn coordinator_process_messages(
        &mut self,
        config: &CoordinatorConfig,
    ) -> anyhow::Result<()> {
        let Some(coordinator_handle) = &config.coordinator_handle else {
            return Ok(());
        };
        let messages = self
            .runtime_store
            .unread_orchestration_coordinator_messages(coordinator_handle)
            .await?;
        if messages.is_empty() {
            return Ok(());
        }
        let mut logs = Vec::new();
        let mut read_ids = Vec::new();
        for message in &messages {
            let mark_read = match message.message_type {
                OrchestrationMessageType::WorkerDone | OrchestrationMessageType::Heartbeat => {
                    let mut sink = |line: String| logs.push(line);
                    let outcome =
                        reconcile_lifecycle_message(&self.runtime_store, message, &mut sink)
                            .await?;
                    match outcome {
                        LifecycleReconciliation::Completed { task_id, .. } => {
                            logs.push(format!("task {task_id} completed"));
                        }
                        LifecycleReconciliation::Failed { task_id, .. } => {
                            logs.push(format!("task {task_id} failed"));
                        }
                        _ => {}
                    }
                    true
                }
                OrchestrationMessageType::Escalation => {
                    logs.push(format!(
                        "escalation from {}: {}",
                        message.from_handle, message.subject
                    ));
                    let payload = payload_value(message);
                    let task_id = message
                        .task_id
                        .clone()
                        .or_else(|| payload_string(payload.as_ref(), "taskId"));
                    let dispatch_id = message
                        .dispatch_id
                        .clone()
                        .or_else(|| payload_string(payload.as_ref(), "dispatchId"));
                    if let Some(task_id) = task_id {
                        if let Some(dispatch) = self
                            .runtime_store
                            .active_orchestration_dispatch_for_task(&task_id)
                            .await?
                        {
                            let assignee_matches = dispatch.assignee_handle.as_deref()
                                == Some(message.from_handle.as_str());
                            let dispatch_matches =
                                dispatch_id.as_deref() == Some(dispatch.id.as_str());
                            if !assignee_matches || !dispatch_matches {
                                logs.push(format!("stale escalation for task {task_id} ignored"));
                            } else {
                                let failed = self
                                    .runtime_store
                                    .fail_orchestration_dispatch(&dispatch.id, &message.subject)
                                    .await?;
                                logs.push(format!(
                                    "dispatch {} failed ({} failures)",
                                    failed.id, failed.failure_count
                                ));
                            }
                        }
                    }
                    true
                }
                OrchestrationMessageType::DecisionGate => {
                    let payload = payload_value(message);
                    let task_id = message
                        .task_id
                        .clone()
                        .or_else(|| payload_string(payload.as_ref(), "taskId"));
                    let dispatch_id = message
                        .dispatch_id
                        .clone()
                        .or_else(|| payload_string(payload.as_ref(), "dispatchId"));
                    if let Some(task_id) = task_id {
                        let question = payload
                            .as_ref()
                            .and_then(|value| value.get("question").and_then(Value::as_str))
                            .unwrap_or(&message.subject)
                            .to_string();
                        let active = self
                            .runtime_store
                            .active_orchestration_dispatch_for_task(&task_id)
                            .await?;
                        let sender_owns_active_dispatch = active.as_ref().is_some_and(|dispatch| {
                            dispatch.assignee_handle.as_deref()
                                == Some(message.from_handle.as_str())
                                && dispatch_id.as_deref() == Some(dispatch.id.as_str())
                        });
                        if !sender_owns_active_dispatch {
                            logs.push(format!("stale decision gate for task {task_id} ignored"));
                        } else {
                            match self
                                .runtime_store
                                .create_orchestration_gate(&task_id, &question, &[])
                                .await
                            {
                                Ok(_) => {
                                    logs.push(format!("gate created for task {task_id}"));
                                    self.queue_gate_push(&task_id, &question).await;
                                }
                                Err(error) => {
                                    logs.push(format!("gate for task {task_id} ignored: {error}"))
                                }
                            }
                        }
                        true
                    } else {
                        false
                    }
                    // Worker-side `ask` questions carry no taskId; they are
                    // answered by a human/agent via `reply`, not by gates.
                }
                _ => false,
            };
            if mark_read {
                read_ids.push(message.id.clone());
            }
        }
        if !read_ids.is_empty() {
            self.runtime_store
                .mark_orchestration_messages_read(&read_ids)
                .await?;
        }
        for line in logs {
            self.coordinator_log(&line);
        }
        Ok(())
    }

    /// Invariant: a task with a pending gate stays blocked.
    async fn coordinator_assert_gate_blocks(&mut self) -> anyhow::Result<()> {
        let pending_gates = self
            .runtime_store
            .list_orchestration_gates(None, Some(OrchestrationGateStatus::Pending))
            .await?;
        for gate in pending_gates {
            if let Some(task) = self
                .runtime_store
                .orchestration_task_by_id(&gate.task_id)
                .await?
            {
                // A stalled task keeps its status while its stall gate is
                // pending. Flipping it to blocked would close the dispatch and
                // claim the worker stopped, which is exactly what a stall
                // cannot assert.
                if task.status != OrchestrationTaskStatus::Blocked
                    && task.status != OrchestrationTaskStatus::Completed
                    && task.status != OrchestrationTaskStatus::Failed
                    && task.status != OrchestrationTaskStatus::Cancelled
                    && task.status != OrchestrationTaskStatus::Stalled
                {
                    let _ = self
                        .runtime_store
                        .update_orchestration_task_status(
                            &task.id,
                            OrchestrationTaskStatus::Blocked,
                            None,
                        )
                        .await;
                }
            }
        }
        Ok(())
    }

    /// A dispatch with no runtime activity past the lease is stalled. It is
    /// never auto-retried, preventing two agents from doing the same work.
    async fn coordinator_warn_stale_dispatches(&mut self) -> anyhow::Result<()> {
        let acceptance_threshold = acceptance_timeout_threshold_iso(chrono::Utc::now());
        let unaccepted = self
            .runtime_store
            .expire_unaccepted_orchestration_dispatches(&acceptance_threshold)
            .await?;
        for dispatch in unaccepted {
            if let Some(handle) = dispatch.assignee_handle.as_deref() {
                self.remove_dispatch_context(handle);
                self.cleanup_failed_owned_spawn(handle).await;
            }
            self.coordinator_log(&format!(
                "dispatch {} did not accept before the startup deadline",
                dispatch.id
            ));
        }
        let threshold = hung_dispatch_threshold_iso(chrono::Utc::now());
        let stale = self
            .runtime_store
            .stall_expired_orchestration_dispatches(&threshold)
            .await?;
        for dispatch in stale {
            self.coordinator_log(&format!(
                "dispatch {} on {} exceeded its activity lease and is stalled since {}",
                dispatch.id,
                dispatch.assignee_handle.as_deref().unwrap_or("<unknown>"),
                dispatch
                    .last_heartbeat_at
                    .as_deref()
                    .or(dispatch.dispatched_at.as_deref())
                    .unwrap_or("dispatch")
            ));
            self.apply_stall_policy(&dispatch).await;
        }
        self.coordinator_process_stall_decisions().await?;
        Ok(())
    }

    pub(super) async fn coordinator_policy_blocks_dispatch(
        &mut self,
        run_id: &str,
    ) -> anyhow::Result<bool> {
        let Some(run) = self
            .runtime_store
            .orchestration_coordinator_run_by_id(run_id)
            .await?
        else {
            return Ok(false);
        };
        Ok(run.execution_policy_status.blocks_dispatch())
    }

    /// Done when every task is completed or failed. Stuck (only blocked left)
    /// keeps the loop alive but logs the situation.
    async fn coordinator_check_convergence(
        &mut self,
        config: &CoordinatorConfig,
    ) -> anyhow::Result<Option<OrchestrationCoordinatorStatus>> {
        let tasks = self
            .runtime_store
            .list_scoped_orchestration_tasks(
                None,
                Some(&config.run_id),
                config.workspace_id.as_deref(),
            )
            .await?;
        if tasks.is_empty() {
            return Ok(Some(OrchestrationCoordinatorStatus::Failed));
        }
        let all_terminal = tasks.iter().all(|task| {
            matches!(
                task.status,
                OrchestrationTaskStatus::Completed
                    | OrchestrationTaskStatus::Failed
                    | OrchestrationTaskStatus::Cancelled
            )
        });
        if all_terminal {
            let any_failed = tasks
                .iter()
                .any(|task| task.status == OrchestrationTaskStatus::Failed);
            return Ok(Some(if any_failed {
                OrchestrationCoordinatorStatus::Failed
            } else {
                OrchestrationCoordinatorStatus::Completed
            }));
        }
        let has_actionable = tasks.iter().any(|task| {
            matches!(
                task.status,
                OrchestrationTaskStatus::Ready | OrchestrationTaskStatus::Dispatched
            )
        });
        if !has_actionable {
            if tasks
                .iter()
                .any(|task| task.status == OrchestrationTaskStatus::Blocked)
            {
                self.coordinator_log(
                    "coordinator stuck: blocked tasks remain; resolve decision gates to continue",
                );
            } else if tasks
                .iter()
                .any(|task| task.status == OrchestrationTaskStatus::Stalled)
            {
                self.coordinator_log(
                    "coordinator waiting: stalled tasks require explicit recovery",
                );
            } else {
                self.coordinator_log(
                    "coordinator run failed: pending tasks have no active dependencies",
                );
                return Ok(Some(OrchestrationCoordinatorStatus::Failed));
            }
        }
        Ok(None)
    }

    pub(super) fn coordinator_log(&self, message: &str) {
        tracing::info!("[coordinator] {message}");
        self.broadcast_authenticated(event(
            "orchestrationCoordinatorLog",
            json!({ "message": message }),
        ));
    }
}
