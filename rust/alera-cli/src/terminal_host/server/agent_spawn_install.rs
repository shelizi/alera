//! The `agentSpawn` step that resumes once a dispatch context install
//! commits: creates the worker tab/PTY and reports to the parked request or
//! the coordinator log. Kept separate from the request validation path so the
//! install state machine boundary stays explicit.

use alera_core::runtime::WorkspaceTabRecord;
use serde_json::{json, Value};

use crate::terminal_host::host_error::HostError;
use crate::terminal_host::orchestration::agent_profile_launch_snapshot::AGENT_PROFILE_LAUNCH_SNAPSHOT_KEY;
use crate::terminal_host::orchestration::agent_registry::AgentStartupPrompt;
use crate::terminal_host::protocol::{error_response, ok_response};

use super::dispatch_context_install::{DispatchInstallOrigin, PendingAgentSpawn};
use super::ServerActor;

impl ServerActor {
    /// The spawn step of `agentSpawn`, resumed once the dispatch context
    /// install committed: creates the worker tab/PTY, then answers the parked
    /// request or reports on the coordinator log.
    pub(super) async fn finish_agent_spawn_install(
        &mut self,
        origin: DispatchInstallOrigin,
        pending: PendingAgentSpawn,
    ) {
        let PendingAgentSpawn {
            handle: id,
            resolved,
            descriptor,
            preflight,
            bootstrap,
            keep_on_failure,
            task,
            workspace_id,
            from,
            title,
            dispatch_response,
        } = pending;
        let dispatch_id = dispatch_response
            .pointer("/dispatch/id")
            .and_then(Value::as_str)
            .map(str::to_string);
        match &origin {
            DispatchInstallOrigin::Request { client_id, .. } => {
                if !self.clients.contains_key(client_id) {
                    self.abort_spawn_install(&id, dispatch_id.as_deref(), "requester disconnected")
                        .await;
                    return;
                }
            }
            DispatchInstallOrigin::Coordinator { run_id } => {
                if !self.coordinators.contains_key(run_id) {
                    self.abort_spawn_install(
                        &id,
                        dispatch_id.as_deref(),
                        "coordinator run stopped",
                    )
                    .await;
                    return;
                }
            }
            DispatchInstallOrigin::Internal => {}
        }
        let command = resolved.command.clone().unwrap_or_else(|| {
            if resolved.managed_launch.is_some() {
                String::new()
            } else {
                descriptor.default_command.to_string()
            }
        });
        let prompt_after_ready =
            descriptor.startup_prompt == AgentStartupPrompt::TerminalAfterReady;
        let now = chrono::Utc::now();
        let task_id = task.id.clone();
        let orchestration_preflight = preflight.as_ref().and_then(|(task_spec, base_drift)| {
            dispatch_id.as_deref().map(|dispatch_id| {
                json!({
                    "taskId": task_id,
                    "dispatchId": dispatch_id,
                    "taskSpec": task_spec,
                    "baseDrift": base_drift.as_ref().map(|drift| json!({
                        "base": drift.base,
                        "behind": drift.behind,
                        "recentSubjects": drift.recent_subjects,
                    })),
                })
            })
        });
        let mut tab_payload = json!({
            "terminalSessionId": id,
            // OnRestart keeps the durable prompt even when delivery waits for
            // a ready event. Each new PTY gets a fresh pending copy.
            "initialPrompt": bootstrap.clone(),
            "pendingAgentPrompt": prompt_after_ready.then(|| json!({
                "agent": descriptor.id,
                "prompt": bootstrap.clone(),
            })),
            "spawnOnCreate": true,
            "orchestrationPreflight": orchestration_preflight,
            "orchestrationSpawn": {
                "task": task_id,
                "from": from,
                "agent": resolved.agent_type,
                "owned": true,
                "keepOnFailure": keep_on_failure,
            }
        });
        if let Some(snapshot) = resolved.launch_snapshot {
            let snapshot = match serde_json::to_value(snapshot) {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    self.fail_spawn_install(
                        origin,
                        &id,
                        dispatch_id.as_deref(),
                        HostError::state(format!(
                            "could not encode agent profile launch snapshot: {error}"
                        )),
                    )
                    .await;
                    return;
                }
            };
            tab_payload[AGENT_PROFILE_LAUNCH_SNAPSHOT_KEY] = snapshot;
        } else {
            tab_payload["initialCommand"] = json!(command);
            tab_payload["initialManagedAgentLaunch"] = json!(resolved.managed_launch);
            tab_payload["agentType"] = json!(resolved.agent_type);
        }
        let agent_type = resolved.agent_type.clone();
        let tab = WorkspaceTabRecord {
            id: id.clone(),
            workspace_id: workspace_id.clone(),
            kind: "terminal".to_string(),
            title: title.unwrap_or_else(|| format!("{} Worker", agent_type)),
            created_at: now,
            updated_at: now,
            payload: tab_payload,
        };
        if let Err(error) = self.upsert_workspace_tab_and_spawn(tab).await {
            self.fail_spawn_install(origin, &id, dispatch_id.as_deref(), error)
                .await;
            return;
        }
        let mut response = json!({
            "terminalHandle": id,
            "agentType": descriptor.id,
            "taskId": task.id,
            "runId": task.run_id,
            "workspaceId": workspace_id,
            "coordinatorHandle": task.coordinator_handle,
            "assigneeHandle": id,
            "startupState": "terminal_started",
            "acceptanceState": "awaiting_acceptance",
        });
        response["dispatch"] = dispatch_response["dispatch"].clone();
        response["contextPath"] = dispatch_response["contextPath"].clone();
        match origin {
            DispatchInstallOrigin::Request {
                client_id,
                request_id,
            } => {
                self.client_write(client_id, ok_response(request_id, response));
            }
            DispatchInstallOrigin::Coordinator { .. } => {
                self.coordinator_log(&format!(
                    "created worker terminal {id} with pre-dispatch for {task_id}"
                ));
            }
            DispatchInstallOrigin::Internal => {}
        }
    }

    /// Drops a parked spawn whose requester or coordinator run went away
    /// while the context install ran.
    async fn abort_spawn_install(&mut self, handle: &str, dispatch_id: Option<&str>, reason: &str) {
        if let Some(dispatch_id) = dispatch_id {
            let _ = self
                .runtime_store
                .fail_orchestration_startup(dispatch_id, reason)
                .await;
        }
        self.remove_dispatch_context(handle);
    }

    async fn fail_spawn_install(
        &mut self,
        origin: DispatchInstallOrigin,
        handle: &str,
        dispatch_id: Option<&str>,
        error: HostError,
    ) {
        self.abort_spawn_install(handle, dispatch_id, "terminal process failed to start")
            .await;
        match origin {
            DispatchInstallOrigin::Request {
                client_id,
                request_id,
            } => {
                self.client_write(client_id, error_response(request_id, &error));
            }
            DispatchInstallOrigin::Coordinator { .. } => {
                self.coordinator_log(&format!("worker spawn failed: {}", error.wire_message()));
            }
            DispatchInstallOrigin::Internal => {}
        }
    }
}
