//! Display-only projection of authenticated Host Agent states.
//!
//! All Native Agent state badges, sidebar counts and footer counts share
//! this mapping. No PTY creation, ownership or unread acknowledgment lives
//! here. Stale inventories must never display "currently working" counts.

use crate::native_i18n::{tr, UiLocale};
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

/// Stable Dashboard section order. One verified Agent card belongs to exactly
/// one state section; interrupted takes precedence over a stale Hook state.
pub fn dashboard_status_group(
    state: Option<AgentActivityState>,
    interrupted: bool,
) -> (u8, &'static str) {
    if interrupted {
        return (3, "Interrupted");
    }
    match state {
        Some(AgentActivityState::Blocked) => (0, "Blocked"),
        Some(AgentActivityState::Waiting) => (1, "Waiting"),
        Some(AgentActivityState::Working) => (2, "Working"),
        Some(AgentActivityState::Done) => (4, "Done"),
        None => (5, "Unknown"),
    }
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

    /// Dashboard review queue: waiting/blocked Host statuses only. A
    /// completed or interrupted turn must not be mistaken for user approval.
    pub const fn needs_review(self) -> bool {
        matches!(self, Self::Waiting | Self::Blocked)
    }

    /// Requires an active Host inventory and excludes explicitly interrupted
    /// sessions even when an older Hook still reports waiting/blocked.
    pub fn needs_host_review(session: &TerminalSession, inventory_current: bool) -> bool {
        !session.interrupted
            && Self::for_session(session, inventory_current).is_some_and(Self::needs_review)
    }

    pub const fn color(self) -> u32 {
        match self {
            Self::Working => AGENT_WORKING,
            Self::Waiting => AGENT_WAITING,
            Self::Blocked | Self::Interrupted => AGENT_BLOCKED,
            Self::Done => AGENT_DONE,
        }
    }

    /// Classic uses the blue info dot in Tab strips, but an amber
    /// spinner in sidebar/run summaries. Both denote the SAME Host state.
    pub const fn run_indicator_color(self) -> u32 {
        match self {
            Self::Working | Self::Waiting => AGENT_WAITING,
            Self::Blocked | Self::Interrupted => AGENT_BLOCKED,
            Self::Done => AGENT_DONE,
        }
    }

    /// Classic's compact status tooltip, without low-level Host IDs.
    /// The Session details view remains the source for those identifiers.
    pub fn tooltip(self, agent_name: &str, locale: UiLocale) -> String {
        let key = agent_name.trim();
        let name = if key.is_empty() {
            tr(locale, "Agents")
        } else {
            // Host sends wire IDs such as "claude" and "agy". Show the
            // same canonical display names as Classic's descriptor registry
            // without maintaining a second, divergent name table.
            alera_core::agent_descriptor::agent_descriptor(&key.to_ascii_lowercase())
                .map(|descriptor| descriptor.display_name.to_string())
                .unwrap_or_else(|| key.to_string())
        };
        format!("{} · {}", name, tr(locale, self.label()))
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
            agent_context: None,
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
    fn dashboard_sections_group_each_state_and_prioritize_interrupted() {
        assert_eq!(
            dashboard_status_group(Some(AgentActivityState::Blocked), false),
            (0, "Blocked")
        );
        assert_eq!(
            dashboard_status_group(Some(AgentActivityState::Waiting), false),
            (1, "Waiting")
        );
        assert_eq!(
            dashboard_status_group(Some(AgentActivityState::Working), false),
            (2, "Working")
        );
        assert_eq!(
            dashboard_status_group(Some(AgentActivityState::Blocked), true),
            (3, "Interrupted")
        );
        assert_eq!(
            dashboard_status_group(Some(AgentActivityState::Done), false),
            (4, "Done")
        );
        assert_eq!(dashboard_status_group(None, false), (5, "Unknown"));
    }

    #[test]
    fn dashboard_needs_review_respects_host_freshness_and_state() {
        for state in [AgentActivityState::Waiting, AgentActivityState::Blocked] {
            let current = session("review", Some(state));
            assert!(AgentVisualState::needs_host_review(&current, true));
            assert!(!AgentVisualState::needs_host_review(&current, false));
            let mut interrupted = current.clone();
            interrupted.interrupted = true;
            assert!(!AgentVisualState::needs_host_review(&interrupted, true));
            let mut stopped = current.clone();
            stopped.running = false;
            assert!(AgentVisualState::for_session(&stopped, true).is_none());
            let mut unverified = current;
            unverified.agent_type = None;
            assert!(AgentVisualState::for_session(&unverified, true).is_none());
        }
        for state in [AgentActivityState::Working, AgentActivityState::Done] {
            assert!(
                !AgentVisualState::for_session(&session("other", Some(state)), true)
                    .is_some_and(AgentVisualState::needs_review)
            );
        }
        let mut interrupted = session("i", Some(AgentActivityState::Done));
        interrupted.interrupted = true;
        assert!(!AgentVisualState::for_session(&interrupted, true)
            .is_some_and(AgentVisualState::needs_review));
    }

    #[test]
    fn classic_working_tab_dot_and_sidebar_spinner_have_distinct_colors() {
        assert_eq!(AgentVisualState::Working.color(), AGENT_WORKING);
        assert_eq!(
            AgentVisualState::Working.run_indicator_color(),
            AGENT_WAITING
        );
        for state in [
            AgentVisualState::Waiting,
            AgentVisualState::Blocked,
            AgentVisualState::Interrupted,
            AgentVisualState::Done,
        ] {
            assert_eq!(state.color(), state.run_indicator_color());
        }
    }

    #[test]
    fn tooltip_is_localized_compact_and_does_not_expose_host_identifiers() {
        let working = AgentVisualState::Working;
        assert_eq!(
            working.tooltip("Codex", UiLocale::TraditionalChinese),
            "Codex · 工作中"
        );
        assert_eq!(
            AgentVisualState::Waiting.tooltip("Claude Code", UiLocale::Japanese),
            "Claude Code · 入力待ち"
        );
        assert_eq!(
            AgentVisualState::Interrupted.tooltip(" ", UiLocale::English),
            "Agents · Interrupted"
        );
        let tooltip = working.tooltip("Codex", UiLocale::SimplifiedChinese);
        assert_eq!(
            AgentVisualState::Waiting.tooltip("claude", UiLocale::TraditionalChinese),
            "Claude Code · 等待輸入"
        );
        assert_eq!(
            AgentVisualState::Done.tooltip("agy", UiLocale::English),
            "Antigravity · Done"
        );
        assert_eq!(
            AgentVisualState::Blocked.tooltip("copilot", UiLocale::English),
            "GitHub Copilot · Blocked"
        );
        assert_eq!(
            working.tooltip("unknown-adapter", UiLocale::English),
            "unknown-adapter · Working"
        );
        assert!(!tooltip.contains("Host session"));
        assert!(!tooltip.contains("Workspace:"));
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
