use std::collections::HashMap;

use alera_core::runtime::{RuntimeAiAssistSettings, RuntimeStore, WorkspaceTabRecord};
use chrono::Utc;
use serde_json::{json, Value};

use super::actor_test_harness::test_actor;
use super::agent_title_generation::{job_matches, AgentTitleSettingsQuery};
use super::agent_title_state::{initialize, is_manual, AgentTitleState, PRIVATE_KEY};
use super::tab_compatibility::{preserve_host_owned_tab_payload, redact_private_tab_payload};

fn tab() -> WorkspaceTabRecord {
    WorkspaceTabRecord {
        id: "tab".into(),
        workspace_id: "workspace".into(),
        kind: "terminal".into(),
        title: "Terminal".into(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        payload: json!({}),
    }
}

fn codex_rollout_fixture(dir: &std::path::Path, session_id: &str) -> String {
    let sessions = dir.join("sessions/2026/11/21");
    std::fs::create_dir_all(&sessions).unwrap();
    let path = sessions.join(format!("rollout-2026-11-21T10-00-00-{session_id}.jsonl"));
    std::fs::write(
        &path,
        format!(
            "{}\n",
            json!({"type": "session_meta", "payload": {"id": session_id, "session_id": session_id}})
        ),
    )
    .unwrap();
    path.to_str().unwrap().to_owned()
}

#[tokio::test]
async fn agent_title_settings_query_can_be_tested_without_server_actor() {
    let dir = tempfile::tempdir().unwrap();
    let store = RuntimeStore::open(dir.path()).await.unwrap();
    store
        .set_ai_assist_settings(RuntimeAiAssistSettings {
            enabled: false,
            auto_generate_agent_titles: false,
            timeout_seconds: 17,
            ..Default::default()
        })
        .await
        .unwrap();

    let settings = AgentTitleSettingsQuery::new(&store).load().await.unwrap();
    assert!(!settings.enabled);
    assert!(!settings.auto_generate_agent_titles);
    assert_eq!(settings.timeout_seconds, 17);
}

#[tokio::test]
async fn manual_generation_rejects_when_ai_assist_is_disabled() {
    let dir = tempfile::tempdir().unwrap();
    let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    let mut tab = tab();
    initialize(&mut tab, "Initial task");
    let request = json!({
        "tabId": "tab",
        "expectedConversationId": tab.payload["agentTitleConversationId"],
        "expectedRevision": tab.payload["agentTitleRevision"],
    });
    actor.runtime_store.upsert_workspace_tab(tab).await.unwrap();
    actor
        .runtime_store
        .set_ai_assist_settings(RuntimeAiAssistSettings {
            enabled: false,
            ..Default::default()
        })
        .await
        .unwrap();

    let error = actor.request_agent_title(1, 1, &request).await.unwrap_err();
    assert_eq!(error.to_string(), "AI Assist is disabled.");
    assert!(actor.agent_title_jobs.is_empty());
}

#[tokio::test]
async fn automatic_generation_disabled_marks_the_conversation_attempted_without_starting_a_job() {
    let dir = tempfile::tempdir().unwrap();
    let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    let mut tab = tab();
    initialize(&mut tab, "First task");
    actor.runtime_store.upsert_workspace_tab(tab).await.unwrap();
    actor
        .runtime_store
        .set_ai_assist_settings(RuntimeAiAssistSettings {
            auto_generate_agent_titles: false,
            ..Default::default()
        })
        .await
        .unwrap();

    actor
        .observe_agent_title("tab", "pi", Some("one"), "First task", true)
        .await;

    assert!(actor.agent_title_jobs.is_empty());
    let saved = actor
        .runtime_store
        .find_workspace_tab("tab")
        .await
        .unwrap()
        .unwrap();
    assert!(AgentTitleState::read(&saved).unwrap().attempted);
    assert_ne!(saved.payload["agentTitleStatus"], "generating");
}

#[path = "agent_title_hook_tests.rs"]
mod hook_regressions;

#[tokio::test]
async fn existing_conversations_are_baselined_until_a_confirmed_change() {
    let dir = tempfile::tempdir().unwrap();
    let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    actor
        .runtime_store
        .upsert_workspace_tab(tab())
        .await
        .unwrap();
    actor
        .observe_agent_title("tab", "codex", Some("old"), "Existing task", true)
        .await;
    assert!(actor.agent_title_jobs.is_empty());
    actor
        .observe_agent_title("tab", "codex", Some("old"), "Next turn", true)
        .await;
    assert!(actor.agent_title_jobs.is_empty());
    actor
        .observe_agent_title("tab", "codex", Some("new"), "New task", true)
        .await;
    assert_eq!(actor.agent_title_jobs.len(), 1);
    let current = actor
        .runtime_store
        .find_workspace_tab("tab")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        AgentTitleState::read(&current).unwrap().initial_prompt,
        "New task"
    );
}

#[tokio::test]
async fn session_start_waits_for_the_first_prompt_and_deduplicates_turns() {
    let dir = tempfile::tempdir().unwrap();
    let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    let mut tab = tab();
    initialize(&mut tab, "");
    actor.runtime_store.upsert_workspace_tab(tab).await.unwrap();
    actor
        .observe_agent_title("tab", "codex", Some("thread"), "", false)
        .await;
    assert!(actor.agent_title_jobs.is_empty());
    actor
        .observe_agent_title("tab", "codex", Some("thread"), "Fix login", true)
        .await;
    let generation = actor.agent_title_jobs["tab"].id.clone();
    actor
        .observe_agent_title("tab", "codex", Some("thread"), "Fix login", true)
        .await;
    actor
        .observe_agent_title("tab", "codex", Some("thread"), "More work", true)
        .await;
    assert_eq!(actor.agent_title_jobs["tab"].id, generation);
    let current = actor
        .runtime_store
        .find_workspace_tab("tab")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        AgentTitleState::read(&current).unwrap().initial_prompt,
        "Fix login"
    );
    actor
        .finish_agent_title("tab".into(), generation, Ok("Fix Login With Google".into()))
        .await;
    let saved = actor
        .runtime_store
        .find_workspace_tab("tab")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(saved.title, "Fix Login With Google");
    assert_eq!(saved.payload["manualTitle"], true);
    assert!(!is_manual(&saved));
    assert_eq!(saved.payload["agentTitleStatus"], "idle");
}

#[tokio::test]
async fn manual_rename_invalidates_even_an_identical_title() {
    for source in ["generated", "manual"] {
        let dir = tempfile::tempdir().unwrap();
        let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
        let mut tab = tab();
        initialize(&mut tab, "Fix login");
        tab.payload["manualTitle"] = json!(true);
        tab.payload["agentTitleSource"] = json!(source);
        actor
            .runtime_store
            .upsert_workspace_tab(tab.clone())
            .await
            .unwrap();
        actor
            .queue_agent_title(
                tab.clone(),
                AgentTitleState::read(&tab).unwrap(),
                Some((1, 1)),
            )
            .await
            .unwrap();
        let generation = actor.agent_title_jobs["tab"].id.clone();
        let renamed = actor
            .runtime_store
            .rename_workspace_tab("tab", "Terminal")
            .await
            .unwrap();
        assert!(!job_matches(&actor.agent_title_jobs["tab"], &renamed));
        actor
            .finish_agent_title("tab".into(), generation, Ok("Unwanted Replacement".into()))
            .await;
        let saved = actor
            .runtime_store
            .find_workspace_tab("tab")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.title, "Terminal");
        assert!(is_manual(&saved));
    }
}

#[tokio::test]
async fn explicit_regeneration_can_replace_manual_title_without_changing_the_prompt() {
    let dir = tempfile::tempdir().unwrap();
    let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    let mut tab = tab();
    initialize(&mut tab, "Initial task");
    actor.runtime_store.upsert_workspace_tab(tab).await.unwrap();
    let tab = actor
        .runtime_store
        .rename_workspace_tab("tab", "My name")
        .await
        .unwrap();
    let request = json!({"tabId": "tab", "expectedConversationId": tab.payload["agentTitleConversationId"], "expectedRevision": tab.payload["agentTitleRevision"]});
    actor.request_agent_title(1, 1, &request).await.unwrap();
    assert!(actor.request_agent_title(2, 2, &request).await.is_err());
    let generation = actor.agent_title_jobs["tab"].id.clone();
    actor
        .finish_agent_title("tab".into(), generation, Ok("Updated Task Title".into()))
        .await;
    let saved = actor
        .runtime_store
        .find_workspace_tab("tab")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(saved.title, "Updated Task Title");
    assert!(!is_manual(&saved));
    assert_eq!(
        AgentTitleState::read(&saved).unwrap().initial_prompt,
        "Initial task"
    );
    assert!(actor.request_agent_title(1, 3, &request).await.is_err());
}

#[tokio::test]
async fn late_result_cannot_rename_a_new_conversation_or_closed_tab() {
    let dir = tempfile::tempdir().unwrap();
    let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    let mut tab = tab();
    initialize(&mut tab, "");
    actor.runtime_store.upsert_workspace_tab(tab).await.unwrap();
    actor
        .observe_agent_title("tab", "claude", Some("one"), "First task", true)
        .await;
    let first = actor.agent_title_jobs["tab"].id.clone();
    actor
        .observe_agent_title("tab", "claude", Some("two"), "Second task", true)
        .await;
    let second = actor.agent_title_jobs["tab"].id.clone();
    assert_ne!(first, second);
    actor
        .finish_agent_title("tab".into(), first, Ok("Old Task Title".into()))
        .await;
    assert_eq!(actor.agent_title_jobs["tab"].id, second);
    actor
        .runtime_store
        .remove_workspace_tab("tab")
        .await
        .unwrap();
    actor
        .finish_agent_title("tab".into(), second, Ok("Deleted Task Title".into()))
        .await;
    assert!(actor
        .runtime_store
        .find_workspace_tab("tab")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn disabling_generation_discards_pending_results_and_failures_do_not_retry() {
    let dir = tempfile::tempdir().unwrap();
    let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    let mut tab = tab();
    initialize(&mut tab, "First task");
    actor.runtime_store.upsert_workspace_tab(tab).await.unwrap();
    actor
        .observe_agent_title("tab", "pi", Some("one"), "First task", true)
        .await;
    let generation = actor.agent_title_jobs["tab"].id.clone();
    actor
        .runtime_store
        .set_ai_assist_settings(RuntimeAiAssistSettings {
            auto_generate_agent_titles: false,
            ..Default::default()
        })
        .await
        .unwrap();
    actor
        .finish_agent_title("tab".into(), generation, Ok("Do Not Apply".into()))
        .await;
    let saved = actor
        .runtime_store
        .find_workspace_tab("tab")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(saved.title, "Terminal");
    assert_eq!(saved.payload["agentTitleStatus"], "failed");
    actor
        .runtime_store
        .set_ai_assist_settings(RuntimeAiAssistSettings::default())
        .await
        .unwrap();
    actor
        .observe_agent_title("tab", "pi", Some("one"), "Another turn", true)
        .await;
    assert!(actor.agent_title_jobs.is_empty());
}

#[tokio::test]
async fn persisted_attempts_survive_host_restart_without_replay() {
    let dir = tempfile::tempdir().unwrap();
    let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    let mut tab = tab();
    initialize(&mut tab, "First task");
    actor.runtime_store.upsert_workspace_tab(tab).await.unwrap();
    actor
        .observe_agent_title("tab", "pi", Some("one"), "First task", true)
        .await;
    drop(actor);
    let mut resumed = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    resumed
        .observe_agent_title("tab", "pi", Some("one"), "Another turn", true)
        .await;
    assert!(resumed.agent_title_jobs.is_empty());
    let saved = resumed
        .runtime_store
        .find_workspace_tab("tab")
        .await
        .unwrap()
        .unwrap();
    assert!(AgentTitleState::read(&saved).unwrap().attempted);
}

#[test]
fn private_prompt_is_redacted_and_client_updates_cannot_forge_title_identity() {
    let mut stored = tab();
    initialize(&mut stored, "Private task");
    let mut incoming = stored.clone();
    redact_private_tab_payload(&mut incoming);
    assert!(incoming.payload.get(PRIVATE_KEY).is_none());
    incoming.payload["agentTitleConversationId"] = json!("forged");
    incoming.payload[PRIVATE_KEY] = json!({"initialPrompt": "forged"});
    preserve_host_owned_tab_payload(&stored, &mut incoming);
    assert_eq!(incoming.payload[PRIVATE_KEY], stored.payload[PRIVATE_KEY]);
    assert_eq!(
        incoming.payload["agentTitleConversationId"],
        stored.payload["agentTitleConversationId"]
    );
}

#[test]
fn agent_title_capability_is_advertised_and_mobile_request_is_allowed() {
    assert!(
        super::mobile_gateway_surface::MOBILE_HELLO_CAPABILITIES.contains(&"aiTextAgentTitleV1")
    );
    assert!(super::mobile_gateway_surface::mobile_request_allowed(
        "aiText.agentTitle.generate"
    ));
    let settings: RuntimeAiAssistSettings = serde_json::from_value(json!({})).unwrap();
    assert!(settings.auto_generate_agent_titles);
    assert_eq!(
        serde_json::to_value(settings).unwrap()["autoGenerateAgentTitles"],
        Value::Bool(true)
    );
}

#[tokio::test]
async fn hook_identity_is_independent_of_activity_and_late_closure_is_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    let mut tab = tab();
    initialize(&mut tab, "");
    actor.runtime_store.upsert_workspace_tab(tab).await.unwrap();
    let event = |name: &str, id: &str, extra: Value| crate::agent_status::AgentHookEvent {
        terminal_session_id: "session".into(),
        workspace_id: "workspace".into(),
        tab_id: "tab".into(),
        agent_type: "grok".into(),
        event_name: Some(name.into()),
        payload: {
            let mut payload = extra;
            payload["session_id"] = json!(id);
            payload
        },
    };
    actor
        .observe_hook_title(&event("SessionStart", "one", json!({})))
        .await;
    assert!(actor.agent_title_jobs.is_empty());
    actor
        .observe_hook_title(&event(
            "UserPromptSubmit",
            "one",
            json!({"prompt": "First prompt"}),
        ))
        .await;
    actor
        .observe_hook_title(&event("SessionStart", "two", json!({})))
        .await;
    assert!(actor.agent_title_jobs.is_empty());
    actor
        .observe_hook_title(&event("SessionEnd", "one", json!({})))
        .await;
    actor
        .observe_hook_title(&event(
            "UserPromptSubmit",
            "two",
            json!({"prompt": "Second prompt"}),
        ))
        .await;
    let saved = actor
        .runtime_store
        .find_workspace_tab("tab")
        .await
        .unwrap()
        .unwrap();
    let state = AgentTitleState::read(&saved).unwrap();
    assert_eq!(state.initial_prompt, "Second prompt");
    assert!(!state.closed);
    assert_eq!(actor.agent_title_jobs.len(), 1);
}

#[tokio::test]
async fn empty_terminal_fallback_fails_once_and_retains_the_title() {
    let dir = tempfile::tempdir().unwrap();
    let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    let (inbox, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    actor.inbox = inbox;
    let mut tab = tab();
    initialize(&mut tab, "");
    actor.runtime_store.upsert_workspace_tab(tab).await.unwrap();
    actor.observe_agent_title("tab", "pi", None, "", true).await;
    let id = actor.agent_title_jobs["tab"].id.clone();
    actor.start_agent_title("tab".into(), id).await;
    let completion = tokio::time::timeout(std::time::Duration::from_secs(3), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    let super::ServerCommand::AgentTitleFinished { tab_id, id, result } = completion else {
        panic!("expected completion");
    };
    assert!(result.is_err());
    actor.finish_agent_title(tab_id, id, result).await;
    let saved = actor
        .runtime_store
        .find_workspace_tab("tab")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(saved.title, "Terminal");
    assert_eq!(saved.payload["agentTitleStatus"], "failed");
    actor
        .observe_agent_title("tab", "pi", None, "Later prompt", true)
        .await;
    assert!(actor.agent_title_jobs.is_empty());
}

#[tokio::test]
async fn unauthenticated_clients_cannot_start_title_generation() {
    let dir = tempfile::tempdir().unwrap();
    let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    assert!(actor
        .try_start_deferred_request(
            999,
            1,
            "aiText.agentTitle.generate",
            &json!({"tabId": "tab", "expectedConversationId": null, "expectedRevision": null})
        )
        .await
        .is_err());
    assert!(actor.agent_title_jobs.is_empty());
}

#[tokio::test]
async fn final_provider_hook_recovers_native_id_without_session_start() {
    for agent in ["codex", "claude"] {
        let dir = tempfile::tempdir().unwrap();
        let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
        let mut saved_tab = tab();
        saved_tab.payload["agentType"] = json!(agent);
        initialize(&mut saved_tab, "");
        actor
            .runtime_store
            .upsert_workspace_tab(saved_tab)
            .await
            .unwrap();

        let id = "550e8400-e29b-41d4-a716-446655440077";
        let payload = if agent == "codex" {
            json!({"session_id": id, "transcript_path": codex_rollout_fixture(dir.path(), id)})
        } else {
            json!({"session_id": id})
        };
        let last = crate::agent_status::AgentHookEvent {
            terminal_session_id: "session".into(),
            workspace_id: "workspace".into(),
            tab_id: "tab".into(),
            agent_type: agent.into(),
            event_name: Some("SessionEnd".into()),
            payload,
        };
        actor.observe_hook_title(&last).await;
        let saved = actor
            .runtime_store
            .find_workspace_tab("tab")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            super::terminal_startup_commands::provider_resume_id(&saved),
            Some((agent, id.to_owned()))
        );
        assert!(AgentTitleState::read(&saved).unwrap().closed);
        actor.observe_hook_title(&last).await;
        let duplicate = actor
            .runtime_store
            .find_workspace_tab("tab")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.payload[PRIVATE_KEY], duplicate.payload[PRIVATE_KEY]);
    }
}

#[tokio::test]
async fn disabled_presence_notifications_do_not_suppress_native_session_checkpoint() {
    let dir = tempfile::tempdir().unwrap();
    let session = crate::terminal_host::session::Session::driver_test_stub("session", 80, 24);
    let mut actor = test_actor(
        &dir,
        HashMap::new(),
        HashMap::from([("session".to_string(), session)]),
    )
    .await;
    let mut saved_tab = tab();
    saved_tab.id = "tab-session".into();
    saved_tab.payload["agentType"] = json!("codex");
    initialize(&mut saved_tab, "");
    actor
        .runtime_store
        .upsert_workspace_tab(saved_tab)
        .await
        .unwrap();

    // Runtime settings default to presence notifications off.
    assert!(!actor
        .runtime_store
        .agent_status_hook_settings()
        .await
        .unwrap()
        .is_enabled("codex"));
    let id = "550e8400-e29b-41d4-a716-446655440082";
    actor
        .handle_agent_hook_event(crate::agent_status::AgentHookEvent {
            terminal_session_id: "session".into(),
            workspace_id: "workspace".into(),
            tab_id: "tab-session".into(),
            agent_type: "codex".into(),
            event_name: Some("SessionStart".into()),
            payload: json!({
                "session_id": id,
                "transcript_path": codex_rollout_fixture(dir.path(), id)
            }),
        })
        .await;
    let saved = actor
        .runtime_store
        .find_workspace_tab("tab-session")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        super::terminal_startup_commands::provider_resume_id(&saved),
        Some(("codex", id.to_owned()))
    );
}

#[tokio::test]
async fn codex_session_start_uses_verified_rollout_id_not_conflicting_conversation_fields() {
    let dir = tempfile::tempdir().unwrap();
    let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    let mut record = tab();
    record.payload["agentType"] = json!("codex");
    initialize(&mut record, "");
    actor
        .runtime_store
        .upsert_workspace_tab(record)
        .await
        .unwrap();

    let native_id = "550e8400-e29b-41d4-a716-446655440100";
    let wrong_id = "550e8400-e29b-41d4-a716-446655440200";
    let transcript_path = codex_rollout_fixture(dir.path(), native_id);
    let event = crate::agent_status::AgentHookEvent {
        terminal_session_id: "session".into(),
        workspace_id: "workspace".into(),
        tab_id: "tab".into(),
        agent_type: "codex".into(),
        event_name: Some("SessionStart".into()),
        payload: json!({
            "session_id": native_id,
            "transcript_path": transcript_path,
            "conversation_id": wrong_id,
            "thread_id": wrong_id,
            "turn_id": wrong_id,
        }),
    };
    actor.observe_hook_title(&event).await;
    let current = actor
        .runtime_store
        .find_workspace_tab("tab")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        super::terminal_startup_commands::provider_resume_id(&current),
        Some(("codex", native_id.to_owned()))
    );
    let state = AgentTitleState::read(&current).unwrap();
    assert_eq!(
        state.codex_transcript_path.as_deref(),
        Some(transcript_path.as_str())
    );
    assert!(super::terminal_startup_commands::validate_codex_resume_checkpoint(&current).is_ok());

    let mut mismatched = event.clone();
    mismatched.payload["session_id"] = json!(wrong_id);
    actor.observe_hook_title(&mismatched).await;
    let after_mismatch = actor
        .runtime_store
        .find_workspace_tab("tab")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        after_mismatch.payload[PRIVATE_KEY],
        current.payload[PRIVATE_KEY]
    );

    // An unrelated verified prompt (e.g. child Codex with inherited tab env)
    // cannot replace the running parent's session without SessionStart.
    let mut late_child = event.clone();
    late_child.event_name = Some("UserPromptSubmit".into());
    late_child.payload["session_id"] = json!(wrong_id);
    late_child.payload["transcript_path"] = json!(codex_rollout_fixture(dir.path(), wrong_id));
    late_child.payload["prompt"] = json!("Unrelated work");
    actor.observe_hook_title(&late_child).await;
    let after_late = actor
        .runtime_store
        .find_workspace_tab("tab")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        after_late.payload[PRIVATE_KEY],
        current.payload[PRIVATE_KEY]
    );

    let mut bad_end = late_child.clone();
    bad_end.event_name = Some("SessionEnd".into());
    actor.observe_hook_title(&bad_end).await;
    let after_end = actor
        .runtime_store
        .find_workspace_tab("tab")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after_end.payload[PRIVATE_KEY], current.payload[PRIVATE_KEY]);

    let mut valid_end = event.clone();
    valid_end.event_name = Some("SessionEnd".into());
    actor.observe_hook_title(&valid_end).await;
    let closed = actor
        .runtime_store
        .find_workspace_tab("tab")
        .await
        .unwrap()
        .unwrap();
    assert!(AgentTitleState::read(&closed).unwrap().closed);
    assert!(super::terminal_startup_commands::validate_codex_resume_checkpoint(&closed).is_ok());

    // The transcript must still exist when Host restarts; leave the tab
    // stored and refuse auto-spawn if the exact file was removed.
    std::fs::remove_file(&transcript_path).unwrap();
    assert!(super::terminal_startup_commands::validate_codex_resume_checkpoint(&closed).is_err());
    assert_eq!(
        super::terminal_startup_commands::provider_resume_id(&closed)
            .unwrap()
            .1,
        native_id
    );
}

#[tokio::test]
async fn codex_legacy_unverified_hook_does_not_capture_turn_or_thread_uuid() {
    let dir = tempfile::tempdir().unwrap();
    let mut actor = test_actor(&dir, HashMap::new(), HashMap::new()).await;
    let mut record = tab();
    record.payload["agentType"] = json!("codex");
    initialize(&mut record, "");
    actor
        .runtime_store
        .upsert_workspace_tab(record)
        .await
        .unwrap();
    let wrong_id = "550e8400-e29b-41d4-a716-446655440300";
    let event = crate::agent_status::AgentHookEvent {
        terminal_session_id: "session".into(),
        workspace_id: "workspace".into(),
        tab_id: "tab".into(),
        agent_type: "codex".into(),
        event_name: Some("SessionStart".into()),
        payload: json!({"thread_id": wrong_id, "conversation_id": wrong_id}),
    };
    actor.observe_hook_title(&event).await;
    let saved = actor
        .runtime_store
        .find_workspace_tab("tab")
        .await
        .unwrap()
        .unwrap();
    assert!(super::terminal_startup_commands::provider_resume_id(&saved).is_none());
    let mut unverified = event;
    unverified.payload["session_id"] = json!(wrong_id);
    actor.observe_hook_title(&unverified).await;
    let saved = actor
        .runtime_store
        .find_workspace_tab("tab")
        .await
        .unwrap()
        .unwrap();
    assert!(super::terminal_startup_commands::provider_resume_id(&saved).is_none());
}
