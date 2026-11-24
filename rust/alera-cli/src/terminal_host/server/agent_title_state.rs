use alera_core::runtime::WorkspaceTabRecord;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use uuid::Uuid;

pub(super) const PRIVATE_KEY: &str = "agentTitleStateV1";
const RETIRED_CONTEXT_LIMIT: usize = 16;

type ContextOverview = crate::terminal_host::orchestration::agent_context::AgentContextOverview;

/// Bounded archive inside the exact owning Tab's private state.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct RetiredAgentContext {
    pub overview: ContextOverview,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex_transcript_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agy_transcript_path: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct AgentTitleState {
    pub conversation_id: String,
    pub agent: Option<String>,
    pub native_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex_transcript_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agy_transcript_path: Option<String>,
    pub retired: Vec<String>,
    #[serde(default)]
    pub retired_prompts: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub retired_contexts: BTreeMap<String, RetiredAgentContext>,
    pub initial_prompt: String,
    pub cursor: Option<u64>,
    pub eligible: bool,
    pub attempted: bool,
    pub closed: bool,
    /// Raw, bounded report checkpoint for this exact provider conversation.
    /// Private tab state, never serialized through the public tab projection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_overview:
        Option<crate::terminal_host::orchestration::agent_context::AgentContextOverview>,
}

impl AgentTitleState {
    pub fn new(eligible: bool) -> Self {
        Self {
            conversation_id: Uuid::new_v4().to_string(),
            agent: None,
            native_id: None,
            codex_transcript_path: None,
            agy_transcript_path: None,
            retired: Vec::new(),
            retired_prompts: BTreeMap::new(),
            retired_contexts: BTreeMap::new(),
            initial_prompt: String::new(),
            cursor: None,
            eligible,
            attempted: false,
            closed: false,
            context_overview: None,
        }
    }

    pub fn read(tab: &WorkspaceTabRecord) -> Option<Self> {
        serde_json::from_value(tab.payload.get(PRIVATE_KEY)?.clone()).ok()
    }

    pub fn write(&self, tab: &mut WorkspaceTabRecord) {
        if !tab.payload.is_object() {
            tab.payload = json!({});
        }
        tab.payload[PRIVATE_KEY] = serde_json::to_value(self).expect("serializable title state");
        tab.payload["agentTitleConversationId"] = json!(self.conversation_id);
        if tab.payload.get("agentTitleRevision").is_none() {
            tab.payload["agentTitleRevision"] = json!(Uuid::new_v4().to_string());
        }
    }

    /// Unknown identities bind once; activity alone never proves a new conversation.
    pub fn observe(
        &mut self,
        agent: &str,
        native_id: Option<&str>,
        prompt: &str,
        cursor: u64,
    ) -> bool {
        let incoming = native_id.map(|id| format!("{agent}:{id}"));
        if incoming
            .as_ref()
            .is_some_and(|id| self.retired.contains(id))
        {
            return false;
        }
        let changed = self.agent.as_deref().is_some_and(|old| old != agent)
            || matches!((self.native_id.as_deref(), native_id), (Some(old), Some(new)) if old != new)
            || (self.closed && native_id.is_some() && native_id != self.native_id.as_deref());
        if changed {
            let mut retired = std::mem::take(&mut self.retired);
            let mut prompts = std::mem::take(&mut self.retired_prompts);
            let mut contexts = std::mem::take(&mut self.retired_contexts);
            if let (Some(old_agent), Some(old_id)) = (&self.agent, &self.native_id) {
                let key = format!("{old_agent}:{old_id}");
                prompts.insert(key.clone(), self.initial_prompt.clone());
                if let Some(mut overview) = self.context_overview.take() {
                    overview.report_summary = None;
                    contexts.insert(
                        key.clone(),
                        RetiredAgentContext {
                            overview,
                            codex_transcript_path: self.codex_transcript_path.clone(),
                            agy_transcript_path: self.agy_transcript_path.clone(),
                        },
                    );
                }
                retired.push(key);
            }
            // Keep recent first prompts, never terminal transcripts, in private recovery state.
            prompts.retain(|key, _| retired.iter().rev().take(16).any(|id| id == key));
            contexts.retain(|key, _| {
                retired
                    .iter()
                    .rev()
                    .take(RETIRED_CONTEXT_LIMIT)
                    .any(|id| id == key)
            });
            *self = Self::new(true);
            self.retired = retired;
            self.retired_prompts = prompts;
            self.retired_contexts = contexts;
            self.cursor = Some(cursor);
        }
        if self.closed && !changed {
            return false;
        }
        self.agent = Some(agent.to_string());
        if let Some(id) = native_id {
            self.native_id = Some(id.to_string());
        }
        if self.eligible && self.initial_prompt.is_empty() && !prompt.trim().is_empty() {
            self.initial_prompt =
                super::agent_title_context::prefix(prompt.trim(), 4096).to_string();
        }
        true
    }

    pub fn resume(&mut self, agent: &str, native_id: Option<&str>, cursor: u64) {
        let Some(id) = native_id else { return };
        let key = format!("{agent}:{id}");
        let was_retired = self.retired.contains(&key);
        if !(was_retired
            || (self.closed
                && self.agent.as_deref() == Some(agent)
                && self.native_id.as_deref() == Some(id)))
        {
            return;
        }
        let prompt = if was_retired {
            self.retired_prompts.remove(&key).unwrap_or_default()
        } else {
            self.initial_prompt.clone()
        };
        let archived = if was_retired {
            self.retired_contexts.remove(&key)
        } else {
            None
        };
        self.retired.retain(|old| old != &key);
        self.closed = false;
        self.observe(agent, native_id, "", cursor);
        if let Some(archived) = archived {
            self.context_overview = Some(archived.overview);
            self.codex_transcript_path = archived.codex_transcript_path;
            self.agy_transcript_path = archived.agy_transcript_path;
        }
        // A resume is not a new automatic attempt; use a new token to reject older jobs.
        self.conversation_id = Uuid::new_v4().to_string();
        self.eligible = false;
        self.attempted = true;
        self.initial_prompt = prompt;
        self.cursor = Some(cursor);
    }
}

pub(super) fn hook_starts_conversation(event: &crate::agent_status::AgentHookEvent) -> bool {
    event
        .event_name
        .as_deref()
        .or_else(|| {
            ["hook_event_name", "hookEventName", "hook_type", "hookType"]
                .iter()
                .find_map(|key| event.payload.get(key).and_then(Value::as_str))
        })
        .is_some_and(|name| {
            name.chars()
                .filter(char::is_ascii_alphanumeric)
                .collect::<String>()
                .eq_ignore_ascii_case("sessionstart")
        })
}

pub(super) fn is_manual(tab: &WorkspaceTabRecord) -> bool {
    tab.payload.get("manualTitle").and_then(Value::as_bool) == Some(true)
        && tab.payload.get("agentTitleSource").and_then(Value::as_str) != Some("generated")
}

pub(super) fn initialize(tab: &mut WorkspaceTabRecord, prompt: &str) {
    if !matches!(tab.kind.as_str(), "terminal" | "codex") || AgentTitleState::read(tab).is_some() {
        return;
    }
    let mut state = AgentTitleState::new(true);
    state.initial_prompt = super::agent_title_context::prefix(prompt, 4096).to_string();
    state.cursor = Some(0);
    state.write(tab);
}

pub(super) fn invalidate(tab: &mut WorkspaceTabRecord) {
    if !tab.payload.is_object() {
        tab.payload = json!({});
    }
    tab.payload["agentTitleRevision"] = json!(Uuid::new_v4().to_string());
    tab.payload["agentTitleSource"] = json!("manual");
    tab.payload["agentTitleStatus"] = json!("idle");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turns_and_resume_do_not_change_identity_or_first_prompt() {
        let mut state = AgentTitleState::new(true);
        state.observe("codex", Some("one"), "First", 0);
        let id = state.conversation_id.clone();
        state.attempted = true;
        state.observe("codex", Some("one"), "Second", 42);
        assert_eq!(id, state.conversation_id);
        assert_eq!(state.initial_prompt, "First");
        assert!(state.attempted);
    }

    #[test]
    fn new_conversation_rearms_but_late_events_do_not_restore_old_identity() {
        let mut state = AgentTitleState::new(false);
        state.observe("codex", Some("one"), "Old", 0);
        state.observe("codex", Some("two"), "New", 42);
        assert!(state.eligible);
        assert_eq!(state.cursor, Some(42));
        assert!(!state.observe("codex", Some("one"), "Late", 80));
        assert_eq!(state.initial_prompt, "New");
    }

    #[test]
    fn native_context_archive_is_bounded_and_survives_serialization() {
        let mut state = AgentTitleState::new(true);
        for index in 0..20 {
            let id = index.to_string();
            state.observe("agy", Some(&id), &format!("Task {index}"), index);
            state.agy_transcript_path = Some(format!("account-A/{id}/transcript.jsonl"));
            state.context_overview = Some(ContextOverview {
                goal: format!("Task {index}"),
                progress: format!("Report {index}"),
                report_summary: Some("Transient AI summary".into()),
                ..Default::default()
            });
        }
        assert_eq!(state.retired_contexts.len(), RETIRED_CONTEXT_LIMIT);
        assert!(!state.retired_contexts.contains_key("agy:0"));
        let serialized = serde_json::to_string(&state).unwrap();
        let mut restored: AgentTitleState = serde_json::from_str(&serialized).unwrap();
        restored.resume("agy", Some("18"), 100);
        let context = restored.context_overview.unwrap();
        assert_eq!(context.goal, "Task 18");
        assert_eq!(context.progress, "Report 18");
        assert!(context.report_summary.is_none());
        assert_eq!(
            restored.agy_transcript_path.as_deref(),
            Some("account-A/18/transcript.jsonl")
        );
        assert_eq!(restored.retired_contexts.len(), RETIRED_CONTEXT_LIMIT);
    }

    #[test]
    fn legacy_and_unidentified_sessions_are_conservative() {
        let mut state = AgentTitleState::new(false);
        state.observe("pi", None, "First", 0);
        state.observe("pi", None, "Another task", 100);
        assert!(!state.eligible);
        state.observe("pi", Some("native"), "Bind", 120);
        assert!(!state.eligible);
    }
}
