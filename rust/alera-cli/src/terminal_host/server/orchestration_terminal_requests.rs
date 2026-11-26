use alera_core::runtime::OrchestrationDispatchStatus;
use serde_json::{json, Value};
use std::collections::BTreeSet;

use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::orchestration::agent_presence::AgentPresenceState;

use super::orchestration_validation::{optional_string, require_string, state_error};
use super::ServerActor;

/// The exact native-history source checked at PTY launch can identify a
/// running Agent before its first Hook arrives. A conflicting Hook takes
/// precedence so an incorrectly typed live Tab fails exact navigation closed.
fn reported_agent_type<'a>(
    hook_provider: Option<&'a str>,
    verified_history_provider: Option<&'a str>,
) -> Option<&'a str> {
    hook_provider.or(verified_history_provider)
}

#[cfg(test)]
mod verified_history_agent_type_tests {
    use super::reported_agent_type;

    #[test]
    fn only_verified_history_can_supply_hook_free_identity() {
        assert_eq!(reported_agent_type(None, None), None);
        assert_eq!(reported_agent_type(None, Some("codex")), Some("codex"));
        assert_eq!(reported_agent_type(None, Some("claude")), Some("claude"));
        assert_eq!(
            reported_agent_type(Some("claude"), Some("codex")),
            Some("claude")
        );
    }
}

impl ServerActor {
    pub(super) fn broadcast_builtin_mcp_changed(&self) {
        self.broadcast_authenticated_local(crate::terminal_host::protocol::event(
            "builtinMcpChanged",
            json!({}),
        ));
    }

    /// Bounded, secret-free diagnostics for the two local Settings UIs.
    pub(super) fn builtin_mcp_diagnostics(&self) -> Value {
        let mut sessions: Vec<_> = self
            .sessions
            .iter()
            .filter(|(_, session)| session.running())
            .filter_map(|(handle, session)| {
                let status = self.mcp_ui.connection_status(handle, true);
                let provider = reported_agent_type(
                    self.agent_presence
                        .get(handle)
                        .map(|entry| entry.agent_type.as_str()),
                    session.verified_history_provider.as_deref(),
                );
                if status == "unknown" && provider.is_none() {
                    return None;
                }
                Some(json!({
                    "handle": handle,
                    "workspaceId": session.workspace_id,
                    "tabId": session.tab_id,
                    "provider": provider,
                    "status": status,
                }))
            })
            .collect();
        sessions.sort_by(|a, b| a["handle"].as_str().cmp(&b["handle"].as_str()));
        let total = sessions.len();
        sessions.truncate(100);
        let tools: Vec<_> = crate::mcp::tool_definitions()
            .into_iter()
            .filter_map(|tool| tool.get("name").cloned())
            .collect();
        json!({"server": "alera-workbench", "sessions": sessions, "sessionCount": total, "tools": tools})
    }

    pub(super) fn orchestration_terminals(&self, payload: &Value) -> Value {
        let workspace = optional_string(payload, "workspace");
        let terminals: Vec<Value> = self
            .sessions
            .iter()
            .filter(|(_, session)| {
                workspace
                    .as_ref()
                    .is_none_or(|workspace| &session.workspace_id == workspace)
            })
            .map(|(session_id, session)| {
                let presence = self.agent_presence.get(session_id);
                let agent_type = reported_agent_type(
                    presence.map(|entry| entry.agent_type.as_str()),
                    session.verified_history_provider.as_deref(),
                );
                json!({
                    "handle": session_id,
                    "workspaceId": session.workspace_id,
                    "tabId": session.tab_id,
                    "running": session.running(),
                    "builtinMcpStatus": self.mcp_ui.connection_status(session_id, session.running()),
                    "agentType": agent_type,
                    "agentState": presence.map(|entry| entry.state.as_str()),
                    "stateStartedAt": presence.map(|entry| entry.state_started_at),
                    "updatedAt": presence.map(|entry| entry.updated_at),
                    "prompt": presence.map(|entry| entry.prompt.clone()),
                    "toolName": presence.and_then(|entry| entry.tool_name.clone()),
                    "toolInput": presence.and_then(|entry| entry.tool_input.clone()),
                    "lastAssistantMessage": presence.and_then(|entry| entry.last_assistant_message.clone()),
                    "contextOverview": presence.map(|entry| entry.context_payload()),
                    "interrupted": presence.and_then(|entry| entry.interrupted),
                })
            })
            .collect();
        json!({
            "kind": "terminals",
            "items": terminals,
            "terminals": terminals,
            "filters": { "workspace": workspace },
        })
    }

    pub(super) async fn orchestration_terminal_show(
        &mut self,
        payload: &Value,
    ) -> HostResult<Value> {
        let handle = require_string(payload, "handle")?;
        let session = self
            .sessions
            .get(&handle)
            .ok_or_else(|| HostError::state(format!("terminal not found: {handle}")))?;
        let presence = self.agent_presence.get(&handle);
        let agent_type = reported_agent_type(
            presence.map(|entry| entry.agent_type.as_str()),
            session.verified_history_provider.as_deref(),
        );
        let active_dispatch = self
            .runtime_store
            .active_orchestration_dispatch_for_handle(&handle)
            .await
            .map_err(state_error)?;
        let dispatch = match active_dispatch {
            Some(dispatch) => Some(dispatch),
            None => self
                .runtime_store
                .latest_orchestration_dispatch_for_handle(&handle)
                .await
                .map_err(state_error)?,
        };
        let spawn_failure = self
            .runtime_store
            .find_workspace_tab(&session.tab_id)
            .await
            .map_err(state_error)?
            .and_then(|tab| {
                let recorded = tab
                    .payload
                    .pointer("/orchestrationSpawn/startupFailureRecorded")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                recorded.then(|| {
                    tab.payload
                        .pointer("/orchestrationSpawn/startupFailureReason")
                        .and_then(Value::as_str)
                        .unwrap_or("agent startup failed")
                        .to_string()
                })
            });
        let startup_state = match dispatch.as_ref().map(|value| value.status) {
            _ if !session.running() => "failed",
            Some(OrchestrationDispatchStatus::AwaitingAcceptance) => {
                "dispatch_submitted_unconfirmed"
            }
            Some(OrchestrationDispatchStatus::Dispatched) => "accepted",
            Some(OrchestrationDispatchStatus::Completed) => "completed",
            Some(OrchestrationDispatchStatus::StartupFailed) => "failed",
            Some(OrchestrationDispatchStatus::Stalled) => "stalled",
            _ if spawn_failure.is_some() => "failed",
            _ if presence.is_some_and(|entry| entry.state == AgentPresenceState::Done) => {
                "agent_ready"
            }
            _ if presence.is_some() => "agent_detected",
            _ => "process_started",
        };
        Ok(json!({
            "handle": handle,
            "workspaceId": session.workspace_id,
            "tabId": session.tab_id,
            "running": session.running(),
            "builtinMcpStatus": self.mcp_ui.connection_status(&handle, session.running()),
            "agentType": agent_type,
            "agentState": presence.map(|entry| entry.state.as_str()),
            "startupState": startup_state,
            "startupError": dispatch
                .as_ref()
                .and_then(|value| value.startup_error.clone())
                .or(spawn_failure),
            "dispatch": dispatch,
        }))
    }

    pub(super) async fn orchestration_terminal_prune(
        &mut self,
        payload: &Value,
    ) -> HostResult<Value> {
        let workspace = optional_string(payload, "workspace");
        let apply = payload
            .get("apply")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut handles: BTreeSet<String> = self
            .sessions
            .iter()
            .filter(|(_, session)| {
                !session.running()
                    && workspace
                        .as_ref()
                        .is_none_or(|workspace| &session.workspace_id == workspace)
            })
            .map(|(handle, _)| handle.clone())
            .collect();
        handles.extend(
            self.retained_failed_spawn_handles(workspace.as_deref())
                .await?,
        );
        let handles: Vec<String> = handles.into_iter().collect();
        let mut removed = Vec::new();
        if apply {
            for handle in &handles {
                if self.prune_stopped_terminal(handle).await? {
                    removed.push(handle.clone());
                }
            }
            self.schedule_shutdown_if_idle();
        }
        Ok(json!({
            "dryRun": !apply,
            "workspaceId": workspace,
            "candidates": handles,
            "removed": removed,
        }))
    }

    async fn prune_stopped_terminal(&mut self, handle: &str) -> HostResult<bool> {
        if self
            .sessions
            .get(handle)
            .is_some_and(|session| session.running())
        {
            return Ok(false);
        }
        if !self.sessions.contains_key(handle) {
            let Some(tab) = self
                .runtime_store
                .find_workspace_tab(handle)
                .await
                .map_err(state_error)?
            else {
                return Ok(false);
            };
            let retained = tab
                .payload
                .pointer("/orchestrationSpawn/retainedAfterFailure")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if tab.kind != "terminal" || !retained {
                return Ok(false);
            }
            self.runtime_store
                .remove_workspace_tab(&tab.id)
                .await
                .map_err(state_error)?;
            self.history.queue_delete(handle.to_string());
            self.remove_dispatch_context(handle);
            self.broadcast_workspace_tabs_changed(Some(&tab.workspace_id));
            return Ok(true);
        }
        self.queue_terminal_exit_push(handle, None).await;
        self.cleanup_orchestration_for_closed_session(handle, "terminal pruned")
            .await;
        if self.remove_terminal_session_tab(handle).await? {
            return Ok(true);
        }

        self.flush_all_output(handle);
        let Some(mut session) = self.sessions.remove(handle) else {
            return Ok(false);
        };
        session
            .terminate(true, &self.history, self.config.scrollback_bytes as usize)
            .await;
        Ok(true)
    }

    async fn retained_failed_spawn_handles(
        &self,
        workspace: Option<&str>,
    ) -> HostResult<Vec<String>> {
        let workspace_ids = match workspace {
            Some(workspace) => vec![workspace.to_string()],
            None => self
                .runtime_store
                .list_all_workspaces()
                .await
                .map_err(state_error)?
                .into_iter()
                .map(|workspace| workspace.id)
                .collect(),
        };
        let mut handles = Vec::new();
        for workspace_id in workspace_ids {
            let tabs = self
                .runtime_store
                .list_workspace_tabs(&workspace_id)
                .await
                .map_err(state_error)?;
            handles.extend(tabs.into_iter().filter_map(|tab| {
                let retained = tab
                    .payload
                    .pointer("/orchestrationSpawn/retainedAfterFailure")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if tab.kind != "terminal" || !retained {
                    return None;
                }
                let handle = tab
                    .payload
                    .get("terminalSessionId")
                    .and_then(Value::as_str)
                    .unwrap_or(&tab.id)
                    .to_string();
                (!self
                    .sessions
                    .get(&handle)
                    .is_some_and(|session| session.running()))
                .then_some(handle)
            }));
        }
        Ok(handles)
    }
}
