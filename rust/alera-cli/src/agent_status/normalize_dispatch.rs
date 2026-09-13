use alera_core::agent_descriptor::{AgentStatusStrategy, agent_descriptor};

use crate::terminal_host::orchestration::agent_presence::{AgentPresence, AgentPresenceState};

use super::super::AgentHookEvent;

pub(super) fn normalize_state(
    event: &AgentHookEvent,
    name: &str,
    tool_name: Option<&str>,
    previous: Option<&AgentPresence>,
) -> Option<AgentPresenceState> {
    let descriptor = agent_descriptor(&event.agent_type)?;
    match descriptor.status_strategy {
        AgentStatusStrategy::HerdrSocket => match name {
            "Working" => Some(AgentPresenceState::Working),
            "Blocked" => Some(AgentPresenceState::Blocked),
            "Idle" => Some(AgentPresenceState::Done),
            _ => None,
        },
        AgentStatusStrategy::HookEvents | AgentStatusStrategy::HookEventsWithTranscriptWatch => {
            normalize_hook_event_state(event, name, tool_name, previous)
        }
    }
}

fn normalize_hook_event_state(
    event: &AgentHookEvent,
    name: &str,
    tool_name: Option<&str>,
    previous: Option<&AgentPresence>,
) -> Option<AgentPresenceState> {
    let human_input = tool_name.is_some_and(super::is_human_input_tool);
    match event.agent_type.as_str() {
        "codex" => match name {
            "SessionStart" | "UserPromptSubmit" | "PostToolUse" => {
                Some(AgentPresenceState::Working)
            }
            "PreToolUse" if human_input => Some(AgentPresenceState::Waiting),
            "PreToolUse" => Some(AgentPresenceState::Working),
            "PermissionRequest" => Some(AgentPresenceState::Waiting),
            "Stop" => Some(AgentPresenceState::Done),
            _ => None,
        },
        "claude" => match name {
            "UserPromptSubmit" | "PostToolUse" | "PostToolUseFailure" => {
                Some(AgentPresenceState::Working)
            }
            "PreToolUse" if human_input => Some(AgentPresenceState::Waiting),
            "PreToolUse" => Some(AgentPresenceState::Working),
            "PermissionRequest" | "AskUserQuestion" => Some(AgentPresenceState::Waiting),
            "Stop" => Some(AgentPresenceState::Done),
            _ => None,
        },
        "copilot" => super::normalize_copilot(event, name, human_input),
        "cursor" => match name {
            "beforeSubmitPrompt" | "sessionStart" | "preToolUse" | "postToolUse"
            | "postToolUseFailure" => Some(AgentPresenceState::Working),
            // Cursor fires these before every execution, approval prompt or
            // not, and never tells the hook which it was. The matching `after`
            // event is what ends the wait, so a long command does not sit
            // marked as needing attention for its whole run.
            "beforeShellExecution" | "beforeMCPExecution" => Some(AgentPresenceState::Waiting),
            "afterShellExecution" | "afterMCPExecution" => Some(AgentPresenceState::Working),
            "afterAgentResponse"
                if previous.is_some_and(|entry| entry.state == AgentPresenceState::Done) =>
            {
                Some(AgentPresenceState::Done)
            }
            "afterAgentResponse" => Some(AgentPresenceState::Working),
            "stop" | "sessionEnd" => Some(AgentPresenceState::Done),
            _ => None,
        },
        // Alera never installs `PreToolUse` for agy: Antigravity requires a
        // `decision` there, which an observational hook cannot give without
        // taking over the permission policy. Those arms serve user-written hooks.
        "agy" => match name {
            "PreInvocation" | "PostInvocation" | "PostToolUse" => Some(AgentPresenceState::Working),
            "PreToolUse" if human_input => Some(AgentPresenceState::Waiting),
            "PreToolUse" => Some(AgentPresenceState::Working),
            "Stop"
                if super::bool_field(&event.payload, "fullyIdle") == Some(false)
                    || super::bool_field(&event.payload, "fully_idle") == Some(false) =>
            {
                Some(AgentPresenceState::Working)
            }
            "Stop" => Some(AgentPresenceState::Done),
            _ => None,
        },
        "opencode" | "opencode2" => match name {
            "SessionBusy" | "MessagePart" => Some(AgentPresenceState::Working),
            "PermissionRequest" | "AskUserQuestion" => Some(AgentPresenceState::Waiting),
            "SessionIdle" => Some(AgentPresenceState::Done),
            _ => None,
        },
        "pi" => match name {
            "before_agent_start"
            | "agent_start"
            | "tool_call"
            | "tool_execution_start"
            | "tool_execution_end"
            | "message_end" => Some(AgentPresenceState::Working),
            "agent_end" | "session_shutdown" => Some(AgentPresenceState::Done),
            _ => None,
        },
        "amp" => match name {
            "session.start" | "agent.start" | "tool.call" | "tool.result" => {
                Some(AgentPresenceState::Working)
            }
            "agent.end" => Some(AgentPresenceState::Done),
            _ => None,
        },
        "grok" => super::normalize_grok(event, name),
        "devin" => match name {
            "SessionStart" | "UserPromptSubmit" | "PreToolUse" | "PostToolUse" => {
                Some(AgentPresenceState::Working)
            }
            "PermissionRequest" => Some(AgentPresenceState::Blocked),
            "Stop" | "SessionEnd" => Some(AgentPresenceState::Done),
            _ => None,
        },
        _ => None,
    }
}
