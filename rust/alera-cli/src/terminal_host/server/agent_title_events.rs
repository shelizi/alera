use alera_core::runtime::WorkspaceTabRecord;
use serde_json::Value;

use crate::agent_status::AgentHookEvent;
use crate::terminal_host::host_error::HostResult;
use crate::terminal_host::orchestration::agent_context::AgentContextOverview;
use crate::terminal_host::orchestration::agent_presence::AgentPresenceState;

use super::agent_title_state::AgentTitleState;
use super::codex_resume_identity::same_rollout_source;
use super::workspace_tab_requests::WorkspaceTabStoreHandler;
use super::ServerActor;

#[derive(Default)]
struct TitleActivity {
    work_started: bool,
    session_started: bool,
    codex_transcript_path: Option<String>,
    agy_transcript_path: Option<String>,
    hook: bool,
    status_only: bool,
    prompt_switch: bool,
}

pub(super) enum HookContextBinding {
    Legacy,
    Ignore,
    Active {
        conversation_id: String,
        switched: bool,
        seed: Option<AgentContextOverview>,
    },
}

fn hook_name(event: &AgentHookEvent) -> String {
    event
        .event_name
        .as_deref()
        .or_else(|| {
            ["hook_event_name", "hookEventName", "hook_type", "hookType"]
                .iter()
                .find_map(|key| event.payload[*key].as_str())
        })
        .unwrap_or_default()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase()
}

fn starts_prompt(event: &AgentHookEvent) -> bool {
    matches!(
        hook_name(event).as_str(),
        "userpromptsubmit"
            | "userpromptsubmitted"
            | "beforesubmitprompt"
            | "preinvocation"
            | "beforeagentstart"
            | "agentstart"
            | "working"
    ) || (hook_name(event) == "messagepart" && event.payload["role"] == "user")
}

pub(super) fn conversation_id(payload: &Value) -> Option<&str> {
    [
        "conversation_id",
        "conversationId",
        "session_id",
        "sessionId",
        "sessionID",
        "thread_id",
        "threadId",
    ]
    .iter()
    .find_map(|key| {
        payload
            .get(key)
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
    })
}

/// Codex's resumable identity comes only from its native session_id plus the
/// exact transcript_path metadata. Other UUID fields can identify turns or
/// child conversations and must never become an automatic resume target.
fn verified_codex_event(
    event: &AgentHookEvent,
) -> Option<super::codex_resume_identity::VerifiedCodexSession<'_>> {
    if event.agent_type != "codex"
        || !matches!(
            hook_name(event).as_str(),
            "sessionstart" | "userpromptsubmit" | "stop" | "sessionend"
        )
    {
        return None;
    }
    super::codex_resume_identity::from_hook(&event.payload)
}

impl ServerActor {
    pub(super) async fn initialize_agent_title_if_new(
        &self,
        tab: &mut WorkspaceTabRecord,
    ) -> HostResult<()> {
        if WorkspaceTabStoreHandler::new(&self.runtime_store)
            .find(&tab.id)
            .await?
            .is_none()
        {
            super::agent_title_state::initialize(tab, "");
        }
        Ok(())
    }

    pub(super) async fn observe_hook_title(
        &mut self,
        event: &AgentHookEvent,
    ) -> HookContextBinding {
        if event.payload["agentTitleIgnore"] == true {
            return HookContextBinding::Ignore;
        }
        let normalized = crate::agent_status::normalize_hook_event(event, None);
        let identity = crate::agent_status::resolve_agent_status_identity(
            self.agent_presence.get(&event.terminal_session_id),
            &event.agent_type,
            normalized
                .as_ref()
                .map_or(AgentPresenceState::Done, |s| s.state),
            chrono::Utc::now(),
            crate::agent_status::AGENT_STATUS_IDENTITY_STALE_THRESHOLD,
        );
        // Compatibility hooks can carry a different native ID as well as an alias.
        if identity.should_ignore_event || identity.effective_agent_type != event.agent_type {
            return HookContextBinding::Ignore;
        }
        // Child-agent hooks can share the parent's PTY identity.
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
            return HookContextBinding::Ignore;
        }
        if crate::agent_status::hook_event_closes_session(event) {
            return self.close_agent_title_conversation(event).await;
        }
        let codex_identity = verified_codex_event(event);
        // An unverified native ID must not overwrite an existing checkpoint.
        // The generic conversation_id / thread_id aliases are not Codex IDs.
        let status_only = event.agent_type == "codex" && codex_identity.is_none();
        let native_id = if event.agent_type == "codex" {
            codex_identity
                .as_ref()
                .map(|identity| identity.id)
                .or_else(|| event.payload["session_id"].as_str())
        } else {
            conversation_id(&event.payload)
        };
        // Do not inherit an activity prompt from a previous turn or conversation.
        let prompt = normalized
            .as_ref()
            .map(|status| status.prompt.as_str())
            .unwrap_or("");
        let agy_source = if event.agent_type == "agy" && hook_name(event) == "preinvocation" {
            super::agent_context_agy::verified_agy_activation_source(&self.runtime_store, event)
                .await
        } else {
            None
        };
        let session_started =
            super::agent_title_state::hook_starts_conversation(event) || agy_source.is_some();
        let work_started = normalized
            .as_ref()
            .is_some_and(|status| status.state == AgentPresenceState::Working)
            && !session_started;
        self.observe_agent_title_event(
            &event.tab_id,
            &event.agent_type,
            native_id,
            prompt,
            TitleActivity {
                work_started: work_started || !prompt.is_empty(),
                session_started,
                codex_transcript_path: codex_identity
                    .map(|identity| identity.transcript_path.to_owned())
                    .or_else(|| {
                        status_only
                            .then(|| event.payload["transcript_path"].as_str().map(str::to_owned))
                            .flatten()
                    }),
                agy_transcript_path: agy_source,
                hook: true,
                status_only,
                prompt_switch: starts_prompt(event),
            },
        )
        .await
    }

    pub(super) async fn observe_agent_title(
        &mut self,
        tab_id: &str,
        agent: &str,
        native_id: Option<&str>,
        prompt: &str,
        work_started: bool,
    ) {
        self.observe_agent_title_event(
            tab_id,
            agent,
            native_id,
            prompt,
            TitleActivity {
                work_started,
                ..Default::default()
            },
        )
        .await;
    }

    async fn observe_agent_title_event(
        &mut self,
        tab_id: &str,
        agent: &str,
        native_id: Option<&str>,
        prompt: &str,
        activity: TitleActivity,
    ) -> HookContextBinding {
        let Ok(Some(mut tab)) = WorkspaceTabStoreHandler::new(&self.runtime_store)
            .find(tab_id)
            .await
        else {
            return HookContextBinding::Legacy;
        };
        let session_id =
            super::requests::terminal_session_id_from_tab(&tab).unwrap_or_else(|| tab.id.clone());
        let cursor = self
            .sessions
            .get(&session_id)
            .map(|s| s.output_stream_range().1)
            .unwrap_or(0);
        let mut state = AgentTitleState::read(&tab).unwrap_or_else(|| AgentTitleState::new(false));
        // Non-lifecycle Codex events may update the already-proven thread,
        // but cannot bind a new one. Never let a stale Stop/tool event change
        // the active conversation or reuse another thread's context.
        if activity.status_only {
            if state.native_id.is_none() {
                return HookContextBinding::Legacy;
            }
            if state.closed
                || state.agent.as_deref() != Some(agent)
                || native_id != state.native_id.as_deref()
                || matches!((state.codex_transcript_path.as_deref(), activity.codex_transcript_path.as_deref()),
                    (Some(expected), Some(incoming)) if !same_rollout_source(expected, incoming))
            {
                return HookContextBinding::Ignore;
            }
            return HookContextBinding::Active {
                conversation_id: state.conversation_id.clone(),
                switched: false,
                seed: if self.agent_presence.get(&session_id).is_none_or(|old| {
                    old.conversation_id.as_deref() != Some(state.conversation_id.as_str())
                }) {
                    state.context_overview
                } else {
                    None
                },
            };
        }
        if activity.hook && state.native_id.is_some() {
            if native_id.is_none() && matches!(agent, "codex" | "claude" | "agy") {
                return HookContextBinding::Ignore;
            }
            if matches!((state.native_id.as_deref(), native_id), (Some(old), Some(new)) if old != new)
                && !activity.session_started
                && !activity.prompt_switch
            {
                return HookContextBinding::Ignore;
            }
        }
        if agent == "codex" && activity.codex_transcript_path.is_some() {
            if let (Some(existing), Some(incoming)) = (state.native_id.as_deref(), native_id) {
                // Only a verified SessionStart (or a closed session) can
                // switch an active tab to a different Codex conversation.
                // Late/child UserPromptSubmit events must not retire it.
                if existing != incoming && !activity.session_started && !state.closed {
                    return HookContextBinding::Ignore;
                }
                if existing == incoming
                    && matches!((state.codex_transcript_path.as_deref(), activity.codex_transcript_path.as_deref()),
                        (Some(expected), Some(incoming)) if !same_rollout_source(expected, incoming))
                {
                    return HookContextBinding::Ignore;
                }
            }
            if let Some(incoming) = native_id {
                let key = format!("{agent}:{incoming}");
                if let Some(archived) = state.retired_contexts.get(&key) {
                    if !matches!((archived.codex_transcript_path.as_deref(), activity.codex_transcript_path.as_deref()),
                        (Some(expected), Some(incoming)) if same_rollout_source(expected, incoming))
                    {
                        return HookContextBinding::Ignore;
                    }
                }
            }
        }
        if agent == "agy" {
            if let Some(source) = activity.agy_transcript_path.as_deref() {
                if state.native_id.as_deref() == native_id
                    && state
                        .agy_transcript_path
                        .as_deref()
                        .is_some_and(|old| old != source)
                {
                    return HookContextBinding::Ignore;
                }
                if let Some(native) = native_id {
                    let key = format!("agy:{native}");
                    let archived_source = state
                        .retired_contexts
                        .get(&key)
                        .and_then(|archived| archived.agy_transcript_path.as_deref());
                    if archived_source.is_some_and(|old| old != source) {
                        return HookContextBinding::Ignore;
                    }
                    // Legacy archives did not pin an account/source. Rebuild
                    // from the newly verified transcript rather than risk
                    // returning a different account's saved text.
                    if archived_source.is_none() {
                        state.retired_contexts.remove(&key);
                        state.retired_prompts.remove(&key);
                    }
                }
            }
        }
        let previous = state.clone();
        let previous_id = state.conversation_id.clone();
        if activity.session_started {
            state.resume(agent, native_id, cursor);
        }
        if !state.observe(agent, native_id, prompt, cursor) {
            return HookContextBinding::Ignore;
        }
        if agent == "codex" && state.native_id.as_deref() == native_id {
            if let Some(path) = activity.codex_transcript_path {
                state.codex_transcript_path = Some(path);
            }
        }
        if agent == "agy" && state.native_id.as_deref() == native_id {
            if let Some(path) = activity.agy_transcript_path {
                state.agy_transcript_path = Some(path);
            }
        }
        if previous_id != state.conversation_id {
            self.cancel_agent_title_job(tab_id);
            tab.payload["agentTitleStatus"] = Value::String("idle".into());
        } else if tab.payload["agentTitleStatus"] == "generating"
            && !self.agent_title_jobs.contains_key(tab_id)
        {
            // A host restart must not replay an already-attempted generation.
            tab.payload["agentTitleStatus"] = Value::String("failed".into());
        }
        let switched = previous_id != state.conversation_id;
        let binding = if state.native_id.is_some() {
            HookContextBinding::Active {
                conversation_id: state.conversation_id.clone(),
                switched,
                seed: if switched
                    || self.agent_presence.get(&session_id).is_none_or(|old| {
                        old.conversation_id.as_deref() != Some(state.conversation_id.as_str())
                    }) {
                    state.context_overview.clone()
                } else {
                    None
                },
            }
        } else {
            HookContextBinding::Legacy
        };
        if state == previous && !(activity.work_started && state.eligible && !state.attempted) {
            return binding;
        }
        state.write(&mut tab);
        if WorkspaceTabStoreHandler::new(&self.runtime_store)
            .upsert(tab.clone())
            .await
            .is_err()
        {
            return HookContextBinding::Ignore;
        }
        self.broadcast_workspace_tabs_changed(Some(&tab.workspace_id));
        if activity.work_started {
            let _ = self.queue_agent_title(tab, state, None).await;
        }
        binding
    }

    async fn close_agent_title_conversation(
        &mut self,
        event: &AgentHookEvent,
    ) -> HookContextBinding {
        let tab_id = &event.tab_id;
        let Ok(Some(mut tab)) = WorkspaceTabStoreHandler::new(&self.runtime_store)
            .find(tab_id)
            .await
        else {
            return HookContextBinding::Legacy;
        };
        let Some(mut state) = AgentTitleState::read(&tab) else {
            return HookContextBinding::Legacy;
        };
        let codex_identity = verified_codex_event(event);
        let native_id = if event.agent_type == "codex" {
            codex_identity.as_ref().map(|identity| identity.id)
        } else {
            conversation_id(&event.payload)
        };
        if event.agent_type == "codex" && native_id.is_none() {
            return if state.native_id.is_some() {
                HookContextBinding::Ignore
            } else {
                HookContextBinding::Legacy
            };
        }
        // SessionEnd can be the first successful Hook. Only its authenticated
        // tab identity and native provider UUID may authorize a checkpoint.
        if !state.closed
            && state.native_id.is_none()
            && matches!(event.agent_type.as_str(), "codex" | "claude")
            && super::terminal_startup_commands::tab_agent_type(&tab)
                == Some(event.agent_type.as_str())
            && state
                .agent
                .as_deref()
                .is_none_or(|agent| agent == event.agent_type)
        {
            if let Some(id) = native_id.filter(|id| uuid::Uuid::parse_str(id).is_ok()) {
                state.observe(&event.agent_type, Some(id), "", 0);
                if let Some(identity) = codex_identity.as_ref() {
                    state.codex_transcript_path = Some(identity.transcript_path.to_owned());
                }
            }
        }
        if state.closed
            || (event.agent_type == "codex"
                && state.codex_transcript_path.is_some()
                && !matches!((state.codex_transcript_path.as_deref(), codex_identity.as_ref()),
                    (Some(expected), Some(identity)) if same_rollout_source(expected, identity.transcript_path)))
            || state.agent.as_deref() != Some(event.agent_type.as_str())
            || (state.native_id.is_some() && native_id != state.native_id.as_deref())
        {
            return HookContextBinding::Ignore;
        }
        self.cancel_agent_title_job(tab_id);
        tab.payload["agentTitleStatus"] = Value::String("idle".into());
        state.closed = true;
        state.write(&mut tab);
        let workspace_id = tab.workspace_id.clone();
        if WorkspaceTabStoreHandler::new(&self.runtime_store)
            .upsert(tab)
            .await
            .is_ok()
        {
            self.broadcast_workspace_tabs_changed(Some(&workspace_id));
            HookContextBinding::Active {
                conversation_id: state.conversation_id,
                switched: false,
                seed: None,
            }
        } else {
            HookContextBinding::Ignore
        }
    }
}
