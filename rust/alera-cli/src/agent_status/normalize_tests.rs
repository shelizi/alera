use serde_json::{json, Value};

use super::*;

fn event(agent_type: &str, event_name: &str, payload: Value) -> AgentHookEvent {
    AgentHookEvent {
        terminal_session_id: "session-1".into(),
        workspace_id: "workspace-1".into(),
        tab_id: "tab-1".into(),
        agent_type: agent_type.into(),
        payload,
        event_name: Some(event_name.into()),
    }
}

#[test]
fn every_supported_agent_reports_working() {
    for (agent_type, event_name) in [
        ("codex", "UserPromptSubmit"),
        ("claude", "UserPromptSubmit"),
        ("copilot", "userPromptSubmitted"),
        ("cursor", "beforeSubmitPrompt"),
        ("agy", "PreInvocation"),
        ("opencode", "SessionBusy"),
        ("opencode2", "SessionBusy"),
        ("pi", "agent_start"),
        ("amp", "session.start"),
        ("grok", "UserPromptSubmit"),
        ("devin", "UserPromptSubmit"),
        ("fx", "Working"),
    ] {
        let status = normalize_hook_event(&event(agent_type, event_name, json!({})), None)
            .unwrap_or_else(|| panic!("{agent_type} event was not normalized"));
        assert_eq!(status.state, AgentPresenceState::Working, "{agent_type}");
    }
}

// Cursor emits the `before` event whether or not the user is asked, so the
// `after` event is the only thing that ends the wait for a long command.
#[test]
fn cursor_execution_events_close_the_approval_wait() {
    for event_name in ["beforeShellExecution", "beforeMCPExecution"] {
        let status = normalize_hook_event(&event("cursor", event_name, json!({})), None).unwrap();
        assert_eq!(status.state, AgentPresenceState::Waiting, "{event_name}");
    }
    for event_name in ["afterShellExecution", "afterMCPExecution"] {
        let status = normalize_hook_event(
            &event("cursor", event_name, json!({ "command": "sleep 30" })),
            None,
        )
        .unwrap();
        assert_eq!(status.state, AgentPresenceState::Working, "{event_name}");
        assert_eq!(status.tool_input.as_deref(), Some("sleep 30"));
    }
}

#[test]
fn codex_waiting_status_preserves_nested_tool_details() {
    let status = normalize_hook_event(
        &event(
            "codex",
            "PreToolUse",
            json!({
                "prompt": "Choose a deployment",
                "toolCall": {
                    "name": "request_user_input",
                    "arguments": {"environment": "production"}
                }
            }),
        ),
        None,
    )
    .expect("codex status");

    assert_eq!(status.state, AgentPresenceState::Waiting);
    assert_eq!(status.prompt, "Choose a deployment");
    assert_eq!(status.tool_name.as_deref(), Some("request_user_input"));
    assert!(status
        .tool_input
        .as_deref()
        .is_some_and(|value| { value.contains("environment") && value.contains("production") }));
}

#[test]
fn new_turn_clears_stale_tool_details() {
    let previous = AgentPresence {
        agent_type: "codex".into(),
        state: AgentPresenceState::Waiting,
        state_started_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        prompt: "Old prompt".into(),
        tool_name: Some("request_user_input".into()),
        tool_input: Some("old input".into()),
        last_assistant_message: None,
        interrupted: None,
    };
    let status = normalize_hook_event(
        &event("codex", "UserPromptSubmit", json!({"prompt": "New prompt"})),
        Some(&previous),
    )
    .expect("codex status");

    assert_eq!(status.prompt, "New prompt");
    assert_eq!(status.tool_name, None);
    assert_eq!(status.tool_input, None);
}

#[test]
fn lifecycle_events_are_detected_before_state_normalization() {
    assert!(hook_event_resets_session(&event(
        "grok",
        "session_start",
        json!({})
    )));
    assert!(hook_event_closes_session(&event(
        "pi",
        "session_shutdown",
        json!({})
    )));
    assert!(hook_event_closes_session(&event(
        "fx",
        "SessionEnd",
        json!({})
    )));
    assert!(hook_event_closes_session(&event(
        "devin",
        "SessionEnd",
        json!({})
    )));
}

#[test]
fn devin_hook_states_map_to_presence_states() {
    for (event_name, expected) in [
        ("UserPromptSubmit", AgentPresenceState::Working),
        ("PermissionRequest", AgentPresenceState::Blocked),
        ("Stop", AgentPresenceState::Done),
        ("SessionEnd", AgentPresenceState::Done),
    ] {
        let status = normalize_hook_event(&event("devin", event_name, json!({})), None)
            .unwrap_or_else(|| panic!("Devin {event_name} event was not normalized"));
        assert_eq!(status.state, expected, "{event_name}");
    }
}

#[test]
fn fx_herdr_states_map_to_presence_states() {
    for (event_name, expected) in [
        ("Working", AgentPresenceState::Working),
        ("Blocked", AgentPresenceState::Blocked),
        ("Idle", AgentPresenceState::Done),
    ] {
        let status =
            normalize_hook_event(&event("fx", event_name, json!({})), None).expect("fx status");
        assert_eq!(status.state, expected, "{event_name}");
    }
}

#[test]
fn copilot_can_infer_missing_event_name() {
    let mut event = event(
        "copilot",
        "ignored",
        json!({"prompt": "Implement the change"}),
    );
    event.event_name = None;
    let status = normalize_hook_event(&event, None).expect("copilot status");
    assert_eq!(status.state, AgentPresenceState::Working);
    assert_eq!(status.prompt, "Implement the change");
}

#[test]
fn descriptor_rules_normalize_common_events_for_each_agent() {
    for (agent_type, event_name, expected) in [
        ("codex", "SessionStart", AgentPresenceState::Working),
        ("claude", "PostToolUseFailure", AgentPresenceState::Working),
        ("copilot", "SessionEnd", AgentPresenceState::Done),
        (
            "cursor",
            "beforeShellExecution",
            AgentPresenceState::Waiting,
        ),
        ("antigravity", "PostInvocation", AgentPresenceState::Working),
        ("opencode", "SessionIdle", AgentPresenceState::Done),
        (
            "opencode2",
            "PermissionRequest",
            AgentPresenceState::Waiting,
        ),
        ("pi", "session_shutdown", AgentPresenceState::Done),
        ("amp", "tool.result", AgentPresenceState::Working),
        ("grok", "StopFailure", AgentPresenceState::Done),
        ("devin", "PermissionRequest", AgentPresenceState::Blocked),
        ("fx", "Idle", AgentPresenceState::Done),
    ] {
        let status = normalize_hook_event(&event(agent_type, event_name, json!({})), None)
            .unwrap_or_else(|| panic!("{agent_type} {event_name} was not normalized"));
        assert_eq!(status.state, expected, "{agent_type} {event_name}");
    }
}

#[test]
fn payload_sensitive_custom_handlers_keep_their_irregular_behavior() {
    let copilot = normalize_hook_event(
        &event(
            "copilot",
            "Notification",
            json!({"notification_type": "permission_prompt"}),
        ),
        None,
    )
    .expect("copilot status");
    assert_eq!(copilot.state, AgentPresenceState::Blocked);

    let cursor_previous = AgentPresence {
        agent_type: "cursor".into(),
        state: AgentPresenceState::Done,
        state_started_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        prompt: String::new(),
        tool_name: None,
        tool_input: None,
        last_assistant_message: None,
        interrupted: None,
    };
    let cursor = normalize_hook_event(
        &event("cursor", "afterAgentResponse", json!({})),
        Some(&cursor_previous),
    )
    .expect("cursor status");
    assert_eq!(cursor.state, AgentPresenceState::Done);

    let agy = normalize_hook_event(&event("agy", "Stop", json!({"fullyIdle": false})), None)
        .expect("agy status");
    assert_eq!(agy.state, AgentPresenceState::Working);

    let grok = normalize_hook_event(
        &event(
            "grok",
            "Notification",
            json!({"message": "approve this action"}),
        ),
        None,
    )
    .expect("grok status");
    assert_eq!(grok.state, AgentPresenceState::Waiting);
}
