use serde_json::json;

use crate::agent_status::{
    hook_event_closes_session, hook_event_resets_session, normalize_hook_event,
    resolve_agent_status_identity, AgentHookEvent, AGENT_STATUS_IDENTITY_STALE_THRESHOLD,
};
use crate::terminal_host::orchestration::agent_presence::AgentPresenceState;

use super::{agent_hook_settings::AgentHookSettingsQuery, ServerActor};

impl ServerActor {
    pub(super) async fn handle_agent_hook_event(&mut self, mut event: AgentHookEvent) {
        let Ok(settings) = AgentHookSettingsQuery::new(&self.runtime_store)
            .load()
            .await
        else {
            return;
        };
        let Some(session) = self.sessions.get(&event.terminal_session_id) else {
            return;
        };
        let display_workspace_id = session.workspace_id.clone();
        let tab_id = session.tab_id.clone();
        if event.agent_type == "fx" && event.workspace_id.is_empty() && event.tab_id.is_empty() {
            event.workspace_id = display_workspace_id.clone();
            event.tab_id = tab_id.clone();
        }
        if tab_id != event.tab_id {
            return;
        }
        if display_workspace_id != event.workspace_id {
            // A shell started in an earlier workspace retains the original
            // ALERA_WORKSPACE_ID even when its live PTY is reparented. Accept
            // this *exact* recorded origin, never arbitrary hook identities.
            let recorded_origin = self
                .runtime_store
                .find_workspace_tab(&tab_id)
                .await
                .ok()
                .flatten()
                .and_then(|tab| {
                    tab.payload
                        .get("terminalExecutionWorkspaceId")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned)
                });
            if recorded_origin.as_deref() != Some(event.workspace_id.as_str()) {
                return;
            }
        }
        // Agent status is displayed with the tab's *new* workspace, while
        // the running process and original shell environment stay unchanged.
        event.workspace_id = display_workspace_id;
        let pending_prompt = self
            .runtime_store
            .find_workspace_tab(&session.tab_id)
            .await
            .ok()
            .flatten()
            .and_then(|tab| tab.payload.get("pendingAgentPrompt").cloned())
            .is_some_and(|pending| {
                pending.get("agent").and_then(serde_json::Value::as_str)
                    == Some(event.agent_type.as_str())
            });
        if !settings.is_enabled(&event.agent_type) && !pending_prompt {
            return;
        }
        self.observe_hook_title(&event).await;
        let now = chrono::Utc::now();
        if hook_event_resets_session(&event) {
            if self
                .agent_presence
                .get(&event.terminal_session_id)
                .is_none()
            {
                return;
            }
            let _ = self
                .orchestration_agent_status(&json!({
                    "entries": [{
                        "terminalSessionId": event.terminal_session_id,
                        "removed": true,
                    }],
                }))
                .await;
            return;
        }
        if hook_event_closes_session(&event) {
            let previous = self.agent_presence.get(&event.terminal_session_id);
            let identity = resolve_agent_status_identity(
                previous,
                &event.agent_type,
                AgentPresenceState::Done,
                now,
                AGENT_STATUS_IDENTITY_STALE_THRESHOLD,
            );
            if identity.should_ignore_event || previous.is_none() {
                return;
            }
            let _ = self
                .orchestration_agent_status(&json!({
                    "entries": [{
                        "terminalSessionId": event.terminal_session_id,
                        "removed": true,
                    }],
                }))
                .await;
            return;
        }
        let previous = self.agent_presence.get(&event.terminal_session_id);
        let Some(normalized) = normalize_hook_event(&event, previous) else {
            return;
        };
        let identity = resolve_agent_status_identity(
            previous,
            &event.agent_type,
            normalized.state,
            now,
            AGENT_STATUS_IDENTITY_STALE_THRESHOLD,
        );
        if identity.should_ignore_event {
            return;
        }
        let state_started_at = previous
            .filter(|entry| entry.state == normalized.state)
            .map(|entry| entry.state_started_at)
            .unwrap_or(now);
        let _ = self
            .orchestration_agent_status(&json!({
                "entries": [{
                    "terminalSessionId": event.terminal_session_id,
                    "workspaceId": event.workspace_id,
                    "tabId": event.tab_id,
                    "agentType": identity.effective_agent_type,
                    "state": normalized.state.as_str(),
                    "stateStartedAt": state_started_at,
                    "updatedAt": now,
                    "prompt": normalized.prompt,
                    "toolName": normalized.tool_name,
                    "toolInput": normalized.tool_input,
                    "lastAssistantMessage": normalized.last_assistant_message,
                    "interrupted": normalized.interrupted,
                }],
            }))
            .await;
        self.deliver_pending_agent_prompt(&event.terminal_session_id)
            .await;
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;

    use crate::agent_status::AgentHookEvent;
    use crate::terminal_host::orchestration::agent_presence::AgentPresenceState;
    use crate::terminal_host::server::actor_test_harness::test_actor;
    use crate::terminal_host::session::Session;

    fn native_hook(name: &str) -> AgentHookEvent {
        AgentHookEvent {
            terminal_session_id: "recipient".into(),
            workspace_id: "workspace".into(),
            tab_id: "tab-recipient".into(),
            agent_type: "claude".into(),
            event_name: Some(name.into()),
            payload: json!({
                "hook_event_name": name,
                "session_id": "disposable-native-claude",
            }),
        }
    }

    #[tokio::test]
    async fn native_hook_updates_host_presence_only_when_enabled_and_scoped() {
        let dir = tempfile::tempdir().unwrap();
        let mut actor = test_actor(
            &dir,
            HashMap::new(),
            HashMap::from([(
                "recipient".to_string(),
                Session::driver_test_stub("recipient", 80, 24),
            )]),
        )
        .await;

        // The Host receives a real Hook event without requiring a Flutter
        // status adapter, but must ignore disabled Agent types by default.
        actor
            .handle_agent_hook_event(native_hook("UserPromptSubmit"))
            .await;
        assert!(actor.agent_presence.get("recipient").is_none());

        actor
            .runtime_store
            .set_metadata("settings.agents.agentStatusHooks", r#"{"claude":true}"#)
            .await
            .unwrap();
        actor
            .handle_agent_hook_event(native_hook("UserPromptSubmit"))
            .await;
        assert_eq!(
            actor.agent_presence.get("recipient").unwrap().state,
            AgentPresenceState::Working,
        );

        let mut forged_tab = native_hook("Stop");
        forged_tab.tab_id = "foreign-tab".into();
        actor.handle_agent_hook_event(forged_tab).await;
        let mut forged_workspace = native_hook("Stop");
        forged_workspace.workspace_id = "foreign-workspace".into();
        actor.handle_agent_hook_event(forged_workspace).await;
        assert_eq!(
            actor.agent_presence.get("recipient").unwrap().state,
            AgentPresenceState::Working,
            "an unrelated tab/workspace must never mark an Agent injection-ready",
        );

        // The same native Stop event that a real Claude Code turn emits
        // changes the Host's own state to injection-ready. No synthetic
        // orchestration.agentStatus request is used in this test.
        actor.handle_agent_hook_event(native_hook("Stop")).await;
        assert_eq!(
            actor.agent_presence.get("recipient").unwrap().state,
            AgentPresenceState::Done,
        );
        assert!(actor.agent_presence.is_injection_ready("recipient"));

        // Disabling Claude Hook processing after a completed turn must not
        // allow any new event to corrupt the terminal's last recorded state.
        actor
            .runtime_store
            .set_metadata("settings.agents.agentStatusHooks", r#"{"claude":false}"#)
            .await
            .unwrap();
        actor
            .handle_agent_hook_event(native_hook("UserPromptSubmit"))
            .await;
        assert_eq!(
            actor.agent_presence.get("recipient").unwrap().state,
            AgentPresenceState::Done,
        );
    }
}
