//! Durable, raw Agent Context belongs to the Host's exact native conversation,
//! not to a live-presence entry. Never synthesize a Done state on restore.
use chrono::{DateTime, Utc};
use serde_json::{json, Value};

use crate::agent_status::AgentHookEvent;
use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::orchestration::agent_context::AgentContextOverview;

use super::agent_title_state::AgentTitleState;
use super::workspace_tab_requests::WorkspaceTabStoreHandler;
use super::ServerActor;

/// A selected panel may be delayed behind a tab rebind. Reject the stale
/// selection instead of returning content from another conversation.
pub(super) fn validate_context_owner(
    tab: &alera_core::runtime::WorkspaceTabRecord,
    payload: &Value,
) -> HostResult<()> {
    if tab.payload["automationOwned"] == true {
        return Err(HostError::state("Agent Context is not a user-owned tab"));
    }
    if let Some(expected) = payload["conversationId"].as_str().filter(|s| !s.is_empty()) {
        if tab.payload["agentTitleConversationId"].as_str() != Some(expected) {
            return Err(HostError::state("Agent Context conversation changed"));
        }
    }
    if let Some(expected) = payload["sessionId"].as_str().filter(|s| !s.is_empty()) {
        if super::requests::terminal_session_id_from_tab(tab).unwrap_or_else(|| tab.id.clone())
            != expected
        {
            return Err(HostError::state("Agent Context terminal changed"));
        }
    }
    Ok(())
}

fn observe_checkpoint(
    title: &mut AgentTitleState,
    event: &AgentHookEvent,
    prompt: &str,
    report: Option<&str>,
    at: DateTime<Utc>,
) -> bool {
    if title.closed || title.agent.as_deref() != Some(event.agent_type.as_str()) {
        return false;
    }
    if ["parent_session_id", "parentSessionId", "parentThreadId"]
        .iter()
        .any(|key| {
            event
                .payload
                .get(key)
                .and_then(Value::as_str)
                .is_some_and(|id| !id.is_empty())
        })
    {
        return false;
    }
    let incoming = if event.agent_type == "codex" {
        super::codex_resume_identity::from_hook(&event.payload)
            .filter(|proof| {
                title
                    .codex_transcript_path
                    .as_deref()
                    .is_none_or(|expected| {
                        super::codex_resume_identity::same_rollout_source(
                            expected,
                            proof.transcript_path,
                        )
                    })
            })
            .map(|proof| proof.id)
    } else {
        super::agent_title_events::conversation_id(&event.payload)
    };
    // Fail closed if the current native conversation cannot be identified.
    // A different (including retired) conversation must never write its report.
    if title.native_id.as_deref().is_none() || incoming != title.native_id.as_deref() {
        return false;
    }
    if prompt.trim().is_empty() && report.is_none_or(|text| text.trim().is_empty()) {
        return false;
    }
    let old = title.context_overview.as_ref();
    let current_prompt = if !prompt.trim().is_empty() {
        prompt
    } else if old.is_none() {
        title.initial_prompt.as_str()
    } else {
        ""
    };
    let next = AgentContextOverview::observe(
        old,
        None,
        old.map(|value| value.progress.as_str()),
        current_prompt,
        report,
        at,
    );
    if old == Some(&next) {
        return false;
    }
    // Retain the exact original reply of each newly recorded milestone.
    // Bound source length and keep only the visible retained timeline.
    if let Some(raw) = report.filter(|text| !text.trim().is_empty()) {
        let report_at = at.to_rfc3339();
        if next.timeline.iter().any(|event| event.kind == "report" && event.at == report_at) {
            title.report_sources.insert(report_at, raw.chars().take(8000).collect());
        }
    }
    title.report_sources.retain(|at, _| next.timeline.iter().any(|event| event.kind == "report" && &event.at == at));
    title.context_overview = Some(next);
    true
}

impl ServerActor {
    pub(super) async fn checkpoint_native_agent_context(
        &self,
        event: &AgentHookEvent,
        prompt: &str,
        report: Option<&str>,
        at: DateTime<Utc>,
    ) {
        if prompt.trim().is_empty() && report.is_none_or(|text| text.trim().is_empty()) {
            return;
        }
        let Ok(Some(mut tab)) = WorkspaceTabStoreHandler::new(&self.runtime_store)
            .find(&event.tab_id)
            .await
        else {
            return;
        };
        if tab.workspace_id != event.workspace_id
            || tab.kind != "terminal"
            || tab.payload["automationOwned"] == true
            || event.payload["agentTitleIgnore"] == true
            || super::requests::terminal_session_id_from_tab(&tab).unwrap_or_else(|| tab.id.clone())
                != event.terminal_session_id
        {
            return;
        }
        let Some(mut title) = AgentTitleState::read(&tab) else {
            return;
        };
        if !observe_checkpoint(&mut title, event, prompt, report, at) {
            return;
        }
        title.write(&mut tab);
        if WorkspaceTabStoreHandler::new(&self.runtime_store)
            .upsert(tab)
            .await
            .is_err()
        {
            tracing::warn!(provider = %event.agent_type, "Agent Context checkpoint write failed");
        }
    }

    /// Local authenticated UI lookup, by exact registered tab owner only.
    /// No caller paths, file scan, model invocation, or terminal mutation.
    pub(super) async fn read_agent_context_checkpoint(&self, payload: &Value) -> HostResult<Value> {
        let workspace = super::requests::require_string(payload, "workspaceId")?;
        let id = super::requests::require_string(payload, "tabId")?;
        let tab = WorkspaceTabStoreHandler::new(&self.runtime_store)
            .find(&id)
            .await?
            .ok_or_else(|| HostError::state("Agent Context Tab not found"))?;
        if tab.workspace_id != workspace || tab.kind != "terminal" {
            return Err(HostError::state("Agent Context Tab owner mismatch"));
        }
        validate_context_owner(&tab, payload)?;
        let Some(title) = AgentTitleState::read(&tab) else {
            return Ok(json!({"contextOverview":null,"readOnly":true}));
        };
        let mut context = title.context_overview.clone().unwrap_or_default();
        // Older tabs already have a Host-owned initial prompt but no checkpoint.
        // Do not invent a report or timestamp for that prompt.
        if context.goal.is_empty() {
            context.goal = title.initial_prompt.trim().chars().take(600).collect();
            context.initial_goal = context.goal.clone();
        }
        // Existing AGY tabs predate the checkpoint. The authenticated title
        // state already pins their native UUID, so an unambiguous known CLI
        // history root can recover the last final reply without an AI call.
        let mut from_history = false;
        if context.progress.is_empty()
            && title.agent.as_deref() == Some("agy")
            && super::terminal_startup_commands::tab_agent_type(&tab) == Some("agy")
        {
            if let Some(report) = super::agent_context_agy::recover_saved_agy_report(
                title.native_id.as_deref(),
                title.agy_transcript_path.as_deref(),
            )
            .await
            {
                if let Some(at) = report.at {
                    context = AgentContextOverview::observe(
                        Some(&context),
                        None,
                        None,
                        "",
                        Some(&report.text),
                        at,
                    );
                    from_history = !context.progress.is_empty();
                }
            }
        }
        Ok(json!({"contextOverview":context,"readOnly":true,"fromHistory":from_history,
            "summaryJobs":self.agent_context_pending_reports(&id, &title.conversation_id)}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn completed_hook_report_is_readable_after_reopening_runtime_database() {
        use crate::terminal_host::server::actor_test_harness::test_actor;
        use crate::terminal_host::session::Session;
        use alera_core::runtime::{RuntimeAiAssistSettings, WorkspaceTabRecord};
        use std::collections::HashMap;

        let dir = tempfile::tempdir().unwrap();
        let mut actor = test_actor(
            &dir,
            HashMap::new(),
            HashMap::from([(
                "recipient".into(),
                Session::driver_test_stub("recipient", 80, 24),
            )]),
        )
        .await;
        actor
            .runtime_store
            .set_metadata("settings.agents.agentStatusHooks", r#"{"claude":true}"#)
            .await
            .unwrap();
        actor
            .runtime_store
            .set_ai_assist_settings(RuntimeAiAssistSettings {
                auto_generate_agent_titles: false,
                auto_summarize_agent_reports: false,
                ..Default::default()
            })
            .await
            .unwrap();
        let now = Utc::now();
        let mut tab = WorkspaceTabRecord {
            id: "tab-recipient".into(),
            workspace_id: "workspace".into(),
            kind: "terminal".into(),
            title: "Agent".into(),
            created_at: now,
            updated_at: now,
            payload: json!({"agentType":"claude","terminalSessionId":"recipient"}),
        };
        super::super::agent_title_state::initialize(&mut tab, "");
        actor.runtime_store.upsert_workspace_tab(tab).await.unwrap();
        let mut event = AgentHookEvent {
            agent_type: "claude".into(),
            terminal_session_id: "recipient".into(),
            workspace_id: "workspace".into(),
            tab_id: "tab-recipient".into(),
            event_name: Some("UserPromptSubmit".into()),
            payload: json!({"session_id":"550e8400-e29b-41d4-a716-446655440000","prompt":"Original task"}),
        };
        actor.handle_agent_hook_event(event.clone()).await;
        event.event_name = Some("Stop".into());
        event.payload = json!({"session_id":"550e8400-e29b-41d4-a716-446655440000","last_assistant_message":"Persist this raw report"});
        actor.handle_agent_hook_event(event.clone()).await;
        assert_eq!(
            actor
                .agent_presence
                .get("recipient")
                .unwrap()
                .context
                .progress,
            "Persist this raw report"
        );
        assert!(actor.agent_report_jobs.is_empty());
        let tab = actor
            .runtime_store
            .find_workspace_tab("tab-recipient")
            .await
            .unwrap()
            .unwrap();
        let owner = tab.payload["agentTitleConversationId"].clone();
        let request = json!({"workspaceId":"workspace","tabId":"tab-recipient","sessionId":"recipient","conversationId":owner});
        let first = actor.read_agent_context_checkpoint(&request).await.unwrap();
        assert_eq!(
            first["contextOverview"]["progress"],
            "Persist this raw report"
        );
        assert_eq!(first["contextOverview"]["goal"], "Original task");
        let mut projected = tab.clone();
        super::super::tab_compatibility::redact_private_tab_payload(&mut projected);
        assert!(!projected
            .payload
            .to_string()
            .contains("Persist this raw report"));
        event.event_name = Some("SessionEnd".into());
        event.payload = json!({"session_id":"550e8400-e29b-41d4-a716-446655440000"});
        actor.handle_agent_hook_event(event).await;
        assert!(actor.agent_presence.get("recipient").is_none());
        drop(actor);
        let restored = test_actor(&dir, HashMap::new(), HashMap::new()).await;
        assert!(restored.agent_presence.get("recipient").is_none());
        let report = restored
            .read_agent_context_checkpoint(&request)
            .await
            .unwrap();
        assert_eq!(
            report["contextOverview"]["progress"],
            "Persist this raw report"
        );
        assert_eq!(
            report["contextOverview"]["timeline"],
            first["contextOverview"]["timeline"]
        );
        let mut stale = request.clone();
        stale["conversationId"] = json!("stale-conversation");
        assert!(restored
            .read_agent_context_checkpoint(&stale)
            .await
            .is_err());
        let mut foreign = request.clone();
        foreign["workspaceId"] = json!("foreign-workspace");
        assert!(restored
            .read_agent_context_checkpoint(&foreign)
            .await
            .is_err());
    }

    #[test]
    fn checkpoint_roundtrips_raw_report_and_rejects_foreign_conversations() {
        let mut title = AgentTitleState::new(true);
        title.observe("claude", Some("native"), "Initial task", 0);
        let mut event = AgentHookEvent {
            agent_type: "claude".into(),
            terminal_session_id: "terminal".into(),
            workspace_id: "workspace".into(),
            tab_id: "tab".into(),
            event_name: Some("Stop".into()),
            payload: json!({"session_id":"native"}),
        };
        let now = Utc::now();
        assert!(observe_checkpoint(
            &mut title,
            &event,
            "",
            Some("Raw final reply"),
            now
        ));
        let saved = serde_json::to_string(&title).unwrap();
        let mut restored: AgentTitleState = serde_json::from_str(&saved).unwrap();
        let context = restored.context_overview.as_ref().unwrap();
        assert_eq!(context.progress, "Raw final reply");
        assert_eq!(context.goal, "Initial task");
        assert!(context.report_summary.is_none());
        event.payload["session_id"] = json!("foreign");
        assert!(!observe_checkpoint(
            &mut restored,
            &event,
            "",
            Some("Wrong reply"),
            now
        ));
        assert_eq!(restored.context_overview, title.context_overview);
        restored.closed = true;
        assert_eq!(restored.context_overview, title.context_overview);
        restored.observe("claude", Some("next"), "New task", 10);
        assert!(restored.context_overview.is_none());
    }
}
