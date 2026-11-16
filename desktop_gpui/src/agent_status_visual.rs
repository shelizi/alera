//! Display-only projection of authenticated Host Agent states.
//!
//! All Native Agent state badges, sidebar counts and footer counts share
//! this mapping. No PTY creation, ownership or unread acknowledgment lives
//! here. Stale inventories must never display "currently working" counts.

use crate::terminal::{AgentActivityState, TerminalSession};
use crate::visual_tokens::{AGENT_BLOCKED, AGENT_DONE, AGENT_WAITING, AGENT_WORKING};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentVisualState {
    Working,
    Waiting,
    Blocked,
    Interrupted,
    Done,
}

impl AgentVisualState {
    pub const fn from_hook(state: AgentActivityState, interrupted: bool) -> Self {
        match state {
            AgentActivityState::Working => Self::Working,
            AgentActivityState::Waiting => Self::Waiting,
            AgentActivityState::Blocked => Self::Blocked,
            AgentActivityState::Done if interrupted => Self::Interrupted,
            AgentActivityState::Done => Self::Done,
        }
    }

    pub fn for_session(session: &TerminalSession, inventory_current: bool) -> Option<Self> {
        if !inventory_current
            || !session.running
            || !session
                .agent_type
                .as_deref()
                .is_some_and(|name| !name.trim().is_empty())
        {
            return None;
        }
        Some(Self::from_hook(session.agent_state?, session.interrupted))
    }

    pub const fn color(self) -> u32 {
        match self {
            Self::Working => AGENT_WORKING,
            Self::Waiting => AGENT_WAITING,
            Self::Blocked | Self::Interrupted => AGENT_BLOCKED,
            Self::Done => AGENT_DONE,
        }
    }

    /// Translation key from Classic's AgentRunStateIndicator terminology.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Working => "Working",
            Self::Waiting => "Waiting for input",
            Self::Blocked => "Blocked",
            Self::Interrupted => "Interrupted",
            Self::Done => "Done",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AgentStateSummary {
    pub working: usize,
    pub waiting: usize,
    pub blocked: usize,
    pub interrupted: usize,
    pub done: usize,
}

impl AgentStateSummary {
    pub fn from_sessions(sessions: &[TerminalSession], inventory_current: bool) -> Self {
        let mut summary = Self::default();
        if !inventory_current {
            return summary;
        }
        let mut seen = HashSet::new();
        for session in sessions {
            let Some(state) = AgentVisualState::for_session(session, inventory_current) else {
                continue;
            };
            // Host sessions can span workspaces. Never double count two
            // copies of the same verified Host identity in one inventory.
            if !seen.insert((
                session.handle.as_str(),
                session.workspace_id.as_str(),
                session.tab_id.as_str(),
            )) {
                continue;
            }
            match state {
                AgentVisualState::Working => summary.working += 1,
                AgentVisualState::Waiting => summary.waiting += 1,
                AgentVisualState::Blocked => summary.blocked += 1,
                AgentVisualState::Interrupted => summary.interrupted += 1,
                AgentVisualState::Done => summary.done += 1,
            }
        }
        summary
    }

    pub fn total(self) -> usize {
        self.working + self.waiting + self.blocked + self.interrupted + self.done
    }

    pub fn groups(self) -> [(AgentVisualState, usize); 5] {
        [
            (AgentVisualState::Working, self.working),
            (AgentVisualState::Waiting, self.waiting),
            (AgentVisualState::Blocked, self.blocked),
            (AgentVisualState::Interrupted, self.interrupted),
            (AgentVisualState::Done, self.done),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(handle: &str, state: Option<AgentActivityState>) -> TerminalSession {
        TerminalSession {
            handle: handle.into(),
            workspace_id: "workspace".into(),
            tab_id: format!("tab-{handle}"),
            running: true,
            agent_type: Some("codex".into()),
            agent_state: state,
            interrupted: false,
            agent_state_started_at: None,
            agent_updated_at: None,
        }
    }

    #[test]
    fn state_colors_and_labels_match_classic_tab_semantics() {
        let cases = [
            (
                AgentActivityState::Working,
                AgentVisualState::Working,
                AGENT_WORKING,
                "Working",
            ),
            (
                AgentActivityState::Waiting,
                AgentVisualState::Waiting,
                AGENT_WAITING,
                "Waiting for input",
            ),
            (
                AgentActivityState::Blocked,
                AgentVisualState::Blocked,
                AGENT_BLOCKED,
                "Blocked",
            ),
            (
                AgentActivityState::Done,
                AgentVisualState::Done,
                AGENT_DONE,
                "Done",
            ),
        ];
        for (state, expected, color, label) in cases {
            let current = session("one", Some(state));
            assert_eq!(
                AgentVisualState::for_session(&current, true),
                Some(expected)
            );
            assert_eq!(expected.color(), color);
            assert_eq!(expected.label(), label);
        }
        let mut interrupted = session("i", Some(AgentActivityState::Done));
        interrupted.interrupted = true;
        assert_eq!(
            AgentVisualState::for_session(&interrupted, true),
            Some(AgentVisualState::Interrupted)
        );
        assert_eq!(AgentVisualState::Interrupted.color(), AGENT_BLOCKED);
        assert_eq!(AgentVisualState::Interrupted.label(), "Interrupted");
    }

    #[test]
    fn stale_stopped_generic_or_unconfirmed_agent_is_never_counted() {
        let working = session("one", Some(AgentActivityState::Working));
        assert_eq!(AgentVisualState::for_session(&working, false), None);
        assert_eq!(
            AgentVisualState::for_session(&session("none", None), true),
            None
        );
        let mut stopped = working.clone();
        stopped.running = false;
        assert_eq!(AgentVisualState::for_session(&stopped, true), None);
        let mut generic = working.clone();
        generic.agent_type = None;
        assert_eq!(AgentVisualState::for_session(&generic, true), None);
        assert_eq!(
            AgentStateSummary::from_sessions(&[working, stopped, generic], false).total(),
            0
        );
    }

    #[test]
    fn counts_distinct_host_triples_and_separates_interruptions() {
        let working = session("one", Some(AgentActivityState::Working));
        let mut other_workspace = working.clone();
        other_workspace.workspace_id = "other-workspace".into();
        let mut interrupted = session("two", Some(AgentActivityState::Done));
        interrupted.interrupted = true;
        let summary = AgentStateSummary::from_sessions(
            &[
                working.clone(),
                working,
                other_workspace,
                session("waiting", Some(AgentActivityState::Waiting)),
                session("blocked", Some(AgentActivityState::Blocked)),
                interrupted,
                session("done", Some(AgentActivityState::Done)),
                session("unknown", None),
            ],
            true,
        );
        assert_eq!(
            summary.groups(),
            [
                (AgentVisualState::Working, 2),
                (AgentVisualState::Waiting, 1),
                (AgentVisualState::Blocked, 1),
                (AgentVisualState::Interrupted, 1),
                (AgentVisualState::Done, 1),
            ]
        );
        assert_eq!(summary.total(), 6);
    }
}
