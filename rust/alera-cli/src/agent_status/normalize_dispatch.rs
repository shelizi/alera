use alera_core::agent_descriptor::{agent_descriptor, AgentStatusState, AgentStatusStrategy};

use crate::terminal_host::orchestration::agent_presence::{AgentPresence, AgentPresenceState};

use super::super::AgentHookEvent;

pub(super) fn normalize_state(
    event: &AgentHookEvent,
    name: &str,
    tool_name: Option<&str>,
    previous: Option<&AgentPresence>,
) -> Option<AgentPresenceState> {
    let descriptor = agent_descriptor(&event.agent_type)?;
    let human_input = tool_name.is_some_and(super::is_human_input_tool);
    let state = match descriptor.status_strategy {
        AgentStatusStrategy::HerdrSocket
        | AgentStatusStrategy::HookEvents
        | AgentStatusStrategy::HookEventsWithTranscriptWatch => {
            descriptor.status_normalization.state_for(name, human_input)
        }
    }
    .map(to_presence_state);
    let state = match event.agent_type.as_str() {
        "copilot" => super::normalize_copilot(event, name, human_input).or(state),
        "cursor"
            if name == "afterAgentResponse"
                && previous.is_some_and(|entry| entry.state == AgentPresenceState::Done) =>
        {
            { Some(AgentPresenceState::Done) }.or(state)
        }
        "agy"
            if name == "Stop"
                && (super::bool_field(&event.payload, "fullyIdle") == Some(false)
                    || super::bool_field(&event.payload, "fully_idle") == Some(false)) =>
        {
            { Some(AgentPresenceState::Working) }.or(state)
        }
        "grok" => super::normalize_grok(event, name).or(state),
        _ => state,
    }?;
    Some(state)
}

fn to_presence_state(state: AgentStatusState) -> AgentPresenceState {
    match state {
        AgentStatusState::Working => AgentPresenceState::Working,
        AgentStatusState::Waiting => AgentPresenceState::Waiting,
        AgentStatusState::Blocked => AgentPresenceState::Blocked,
        AgentStatusState::Done => AgentPresenceState::Done,
    }
}
