use alera_core::agent_descriptor::agent_descriptor;
use serde_json::{json, Value};

use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::orchestration::agent_presence::{AgentPresence, AgentPresenceState};
use crate::terminal_host::orchestration::group_resolution::GroupResolutionTerminal;

use super::dispatch_context_install::{DispatchContextContinuation, DispatchInstallOrigin};
use super::orchestration_dispatch_requests::DispatchPreparation;
use super::ServerActor;

impl ServerActor {
    /// Handles requests; wait-capable verbs return `Ok(None)` until a wake or timeout writes the response.
    pub(super) async fn handle_orchestration_request(
        &mut self,
        client_id: u64,
        request_id: i64,
        request_type: &str,
        payload: &Value,
    ) -> HostResult<Option<Value>> {
        self.require_auth(client_id)?;
        match request_type {
            "orchestration.agentSpawn" => {
                self.orchestration_agent_spawn_request(client_id, request_id, payload)
                    .await
            }
            "orchestration.agentSpawnTimeout" => self
                .orchestration_agent_spawn_timeout(payload)
                .await
                .map(Some),
            "orchestration.runPolicyPropose" => self
                .orchestration_run_policy_propose(payload)
                .await
                .map(Some),
            "orchestration.runPolicyShow" => {
                self.orchestration_run_policy_show(payload).await.map(Some)
            }
            "orchestration.runPolicyApprove" => self
                .orchestration_run_policy_resolve(payload, true)
                .await
                .map(Some),
            "orchestration.runPolicyReject" => self
                .orchestration_run_policy_resolve(payload, false)
                .await
                .map(Some),
            "orchestration.send" => self.orchestration_send(client_id, payload).await.map(Some),
            "orchestration.check" => {
                self.orchestration_check(client_id, request_id, payload)
                    .await
            }
            "orchestration.reply" => self.orchestration_reply(client_id, payload).await.map(Some),
            "orchestration.inbox" => self.orchestration_inbox(payload).await.map(Some),
            "orchestration.ask" => self.orchestration_ask(client_id, request_id, payload).await,
            "orchestration.agentStatus" => self.orchestration_agent_status(payload).await.map(Some),
            "orchestration.terminals" => Ok(Some(self.orchestration_terminals(payload))),
            "orchestration.terminalShow" => {
                self.orchestration_terminal_show(payload).await.map(Some)
            }
            "orchestration.terminalPrune" => {
                self.orchestration_terminal_prune(payload).await.map(Some)
            }
            "orchestration.terminalWait" => {
                self.orchestration_terminal_wait(client_id, request_id, payload)
                    .await
            }
            "orchestration.taskWait" => {
                self.orchestration_task_wait(client_id, request_id, payload)
                    .await
            }
            "orchestration.taskCreate" => self.orchestration_task_create(payload).await.map(Some),
            "orchestration.taskList" => self.orchestration_task_list(payload).await.map(Some),
            "orchestration.taskShow" => self.orchestration_task_show(payload).await.map(Some),
            "orchestration.taskCancel" => self.orchestration_task_cancel(payload).await.map(Some),
            "orchestration.taskRecover" => self.orchestration_task_recover(payload).await.map(Some),
            "orchestration.transferCoordinator" => self
                .orchestration_transfer_coordinator(payload)
                .await
                .map(Some),
            "orchestration.dispatch" => {
                self.orchestration_dispatch_request(client_id, request_id, payload)
                    .await
            }
            "orchestration.dispatchShow" => {
                self.orchestration_dispatch_show(payload).await.map(Some)
            }
            "orchestration.dispatchAccept" => {
                self.orchestration_dispatch_accept(payload).await.map(Some)
            }
            "orchestration.dispatchInterrupt" => self
                .orchestration_dispatch_interrupt(payload)
                .await
                .map(Some),
            "orchestration.context" => self.orchestration_context(payload).await.map(Some),
            "orchestration.heartbeat" => self.orchestration_heartbeat(payload).await.map(Some),
            "orchestration.escalate" => self
                .orchestration_escalate(client_id, payload)
                .await
                .map(Some),
            "orchestration.complete" => self.orchestration_complete(payload).await.map(Some),
            "orchestration.workerDone" => self.orchestration_worker_done(payload).await.map(Some),
            "orchestration.workerHelp" => Ok(Some(self.orchestration_worker_help())),
            "orchestration.gateCreate" => self.orchestration_gate_create(payload).await.map(Some),
            "orchestration.gateResolve" => self.orchestration_gate_resolve(payload).await.map(Some),
            "orchestration.gateList" => self.orchestration_gate_list(payload).await.map(Some),
            "orchestration.run" => self.orchestration_run(payload).await.map(Some),
            "orchestration.runList" => self.orchestration_run_list(payload).await.map(Some),
            "orchestration.runShow" => self.orchestration_run_show(payload).await.map(Some),
            "orchestration.status" => self.orchestration_status(payload).await.map(Some),
            "orchestration.runStop" => self.orchestration_run_stop(payload).await.map(Some),
            "orchestration.reset" => self.orchestration_reset(payload).await.map(Some),
            other => Err(HostError::state(format!(
                "Unknown orchestration request: {other}"
            ))),
        }
    }
    // --- agent status forwarding -------------------------------------------

    pub(super) async fn orchestration_agent_status(
        &mut self,
        payload: &Value,
    ) -> HostResult<Value> {
        let Some(entries) = payload.get("entries").and_then(Value::as_array) else {
            return Err(HostError::format("entries must be an array."));
        };
        let mut became_ready = Vec::new();
        let mut presence_changes = Vec::new();
        for entry in entries {
            let Some(handle) = entry
                .get("terminalSessionId")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            let removed = entry
                .get("removed")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if removed {
                self.cleanup_orchestration_for_closed_session(handle, "agent status was removed")
                    .await;
                continue;
            }
            let Some(state) = entry
                .get("state")
                .and_then(Value::as_str)
                .and_then(AgentPresenceState::parse)
            else {
                continue;
            };
            let agent_type = entry
                .get("agentType")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            let started_at = self.agent_presence_timestamp(entry);
            let previous = self.agent_presence.get(handle);
            let changed = previous.is_none_or(|previous| {
                previous.state != state || previous.state_started_at != started_at
            });
            let updated_at = entry
                .get("updatedAt")
                .and_then(Value::as_str)
                .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
                .map(|value| value.with_timezone(&chrono::Utc))
                .unwrap_or_else(chrono::Utc::now);
            let presence = AgentPresence {
                agent_type: agent_type.clone(),
                state,
                state_started_at: started_at,
                updated_at,
                prompt: entry
                    .get("prompt")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| previous.map(|value| value.prompt.clone()))
                    .unwrap_or_default(),
                tool_name: optional_status_string(entry, "toolName")
                    .or_else(|| previous.and_then(|value| value.tool_name.clone())),
                tool_input: optional_status_string(entry, "toolInput")
                    .or_else(|| previous.and_then(|value| value.tool_input.clone())),
                last_assistant_message: optional_status_string(entry, "lastAssistantMessage")
                    .or_else(|| previous.and_then(|value| value.last_assistant_message.clone())),
                interrupted: entry
                    .get("interrupted")
                    .and_then(Value::as_bool)
                    .or_else(|| previous.and_then(|value| value.interrupted)),
            };
            let presence_change = self.sessions.get(handle).map(|session| {
                json!({
                    "terminalSessionId": handle,
                    "workspaceId": session.workspace_id.as_str(),
                    "tabId": session.tab_id.as_str(),
                    "agentType": presence.agent_type.as_str(),
                    "state": presence.state.as_str(),
                    "stateStartedAt": presence.state_started_at.to_rfc3339(),
                    "updatedAt": presence.updated_at.to_rfc3339(),
                    "prompt": presence.prompt.as_str(),
                    "toolName": presence.tool_name.as_deref(),
                    "toolInput": presence.tool_input.as_deref(),
                    "lastAssistantMessage": presence.last_assistant_message.as_deref(),
                    "interrupted": presence.interrupted,
                })
            });
            self.agent_presence.update_full(handle, presence);
            if let Some(change) = presence_change {
                presence_changes.push(change);
            }
            self.queue_agent_push(handle, &agent_type, state, started_at, changed)
                .await;
            if let Ok(Some(dispatch)) = self
                .runtime_store
                .active_orchestration_dispatch_for_handle(handle)
                .await
            {
                let _ = self
                    .runtime_store
                    .record_orchestration_activity(&dispatch.id)
                    .await;
            }
            if state.accepts_injection() {
                became_ready.push(handle.to_string());
            }
        }
        for handle in became_ready {
            self.dispatch_pending_agent_spawn(&handle).await;
            self.deliver_pending_messages(&handle).await;
        }
        self.broadcast_agent_presence_changes(presence_changes);
        Ok(json!({}))
    }

    async fn dispatch_pending_agent_spawn(&mut self, handle: &str) {
        let Some(session) = self.sessions.get(handle) else {
            return;
        };
        let tab_id = session.tab_id.clone();
        let Ok(Some(tab)) = self.runtime_store.find_workspace_tab(&tab_id).await else {
            return;
        };
        let Some(pending) = tab.payload.get("pendingOrchestration").cloned() else {
            return;
        };
        if pending.is_null() {
            return;
        }
        let Some(task) = pending.get("task").and_then(Value::as_str) else {
            return;
        };
        let Some(from) = pending.get("from").and_then(Value::as_str) else {
            return;
        };
        let dispatch_payload = json!({
            "task": task,
            "to": handle,
            "from": from,
            "inject": true,
            "forceSubmit": pending
                .get("agent")
                .and_then(Value::as_str)
                .and_then(agent_descriptor)
                .is_some_and(|descriptor| descriptor.force_submit),
            "agentProfile": pending.get("profile").cloned(),
            "agentQuotaGroup": pending.get("quotaGroup").cloned(),
            "completionPolicy": "return-immediately",
            "terminalPolicy": "keep-open",
        });
        let prepared = match self.prepare_orchestration_dispatch(&dispatch_payload).await {
            Ok(DispatchPreparation::Ready(prepared)) => prepared,
            // A failed dispatch leaves the pending marker so the next ready
            // event retries.
            _ => return,
        };
        let dispatch_id = prepared.dispatch_id.clone();
        let to = prepared.to.clone();
        if let Err(error) = self.start_dispatch_context_install(
            &to,
            &dispatch_id,
            &prepared.context_token,
            DispatchContextContinuation::DispatchRequest {
                origin: DispatchInstallOrigin::Internal,
                to: prepared.to,
                inject: prepared.inject,
                force_submit: prepared.force_submit,
                preamble: prepared.preamble,
                response: Value::Null,
                consumed_tab: Some(tab),
            },
        ) {
            let _ = self
                .runtime_store
                .fail_orchestration_startup(&dispatch_id, "could not install worker context")
                .await;
            tracing::error!("failed to start dispatch context install: {error}");
        }
    }

    // --- terminals ---------------------------------------------------------

    pub(super) fn group_resolution_terminals(&self) -> Vec<GroupResolutionTerminal> {
        self.sessions
            .iter()
            .filter(|(_, session)| session.running())
            .map(|(session_id, session)| GroupResolutionTerminal {
                handle: session_id.clone(),
                workspace_id: Some(session.workspace_id.clone()),
            })
            .collect()
    }
}

fn optional_status_string(entry: &Value, key: &str) -> Option<String> {
    entry
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}
