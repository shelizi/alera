//! Explicit navigation intent for an already-existing, Host-verified Agent Tab.
//!
//! A historical lookup, sidebar row, Windows toast, and Dashboard card can all
//! schedule navigation on a later GPUI render. The user may switch workspaces
//! or focus another tab meanwhile. Deferred handlers MUST check this epoch
//! before changing panes; Host status refresh does not grant UI focus.

use crate::agent_status_notifications::AgentNavigationTarget;
use crate::terminal::TerminalSession;

const MAX_HOST_RECHECKS: u8 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentNavigationSource {
    History,
    Dashboard,
    Sidebar,
    Toast,
}

#[derive(Debug, Default)]
pub struct AgentNavigationQueue {
    epoch: u64,
    pending: Option<AgentNavigationTarget>,
    source: Option<AgentNavigationSource>,
    retries: u8,
}

impl AgentNavigationQueue {
    /// A user-initiated navigation replaces older queued/deferred work.
    pub fn begin(&mut self, target: AgentNavigationTarget, source: AgentNavigationSource) {
        self.epoch = self.epoch.wrapping_add(1);
        self.pending = Some(target);
        self.source = Some(source);
        self.retries = 0;
    }

    /// An explicit workspace/tab selection invalidates even a deferred
    /// callback already taken out of the queue.
    pub fn cancel(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        self.pending = None;
        self.source = None;
        self.retries = 0;
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn is_current(&self, expected_epoch: u64) -> bool {
        self.source.is_some() && expected_epoch == self.epoch
    }

    pub fn source(&self) -> Option<AgentNavigationSource> {
        self.source
    }

    pub fn take_deferred(&mut self) -> Option<(u64, AgentNavigationTarget)> {
        self.pending.take().map(|target| (self.epoch, target))
    }

    /// Wait for the already-running, authenticated Host read. No new task,
    /// process, Session or mutation ID is created by re-queuing UI focus.
    pub fn hold(&mut self, target: AgentNavigationTarget) {
        self.pending = Some(target);
    }

    /// Count actual Host verification attempts rather than render frames.
    pub fn retry(&mut self, target: AgentNavigationTarget) -> bool {
        if self.retries >= MAX_HOST_RECHECKS {
            return false;
        }
        self.retries += 1;
        self.pending = Some(target);
        true
    }

    pub fn reset_retries(&mut self) {
        self.retries = 0;
    }
}

/// Close historical search only after the exact running Host Tab actually
/// owns the active Terminal and its original pane. A matching handle alone
/// or a local preview for a stopped/recycled tab is not a successful focus.
pub fn exact_navigation_focus_matches(
    target: &AgentNavigationTarget,
    selected: Option<&TerminalSession>,
    original_pane_active: bool,
) -> bool {
    original_pane_active
        && selected.is_some_and(|session| {
            session.running
                && session.handle == target.session_id
                && session.workspace_id == target.workspace_id
                && session.tab_id == target.tab_id
                && session.agent_type.as_deref() == Some(target.agent_type.as_str())
        })
}
#[cfg(test)]
mod tests {
    use super::*;

    fn target(id: &str) -> AgentNavigationTarget {
        AgentNavigationTarget {
            session_id: id.to_owned(),
            workspace_id: "workspace-original".into(),
            tab_id: "tab-original".into(),
            agent_type: "claude".into(),
        }
    }

    fn running_session(id: &str) -> TerminalSession {
        TerminalSession {
            handle: id.into(),
            workspace_id: "workspace-original".into(),
            tab_id: "tab-original".into(),
            running: true,
            agent_type: Some("claude".into()),
            agent_state: None,
            interrupted: false,
            agent_state_started_at: None,
            agent_updated_at: None,
            agent_context: None,
            builtin_mcp_status: None,
        }
    }

    #[test]
    fn history_closes_only_when_original_running_host_tab_owns_selected_pane() {
        let exact = target("agent-1");
        let live = running_session("agent-1");
        assert!(exact_navigation_focus_matches(&exact, Some(&live), true));
        assert!(!exact_navigation_focus_matches(&exact, Some(&live), false));
        assert!(!exact_navigation_focus_matches(&exact, None, true));
        for mismatched in [
            TerminalSession {
                handle: "recycled".into(),
                ..live.clone()
            },
            TerminalSession {
                workspace_id: "different-worktree".into(),
                ..live.clone()
            },
            TerminalSession {
                tab_id: "different-tab".into(),
                ..live.clone()
            },
            TerminalSession {
                agent_type: Some("codex".into()),
                ..live.clone()
            },
            TerminalSession {
                running: false,
                ..live.clone()
            },
        ] {
            assert!(!exact_navigation_focus_matches(
                &exact,
                Some(&mismatched),
                true
            ));
        }
    }
    #[test]
    fn manual_workspace_switch_invalidates_already_deferred_host_navigation() {
        let mut queue = AgentNavigationQueue::default();
        queue.begin(target("original"), AgentNavigationSource::History);
        let (epoch, deferred) = queue.take_deferred().expect("queued navigation");
        queue.cancel();
        assert!(!queue.is_current(epoch));
        assert!(queue.take_deferred().is_none());
        assert!(queue.source().is_none());
        assert_eq!(deferred.session_id, "original");
    }

    #[test]
    fn newer_explicit_agent_navigation_supersedes_old_render_callback() {
        let mut queue = AgentNavigationQueue::default();
        queue.begin(target("old"), AgentNavigationSource::Toast);
        let (old_epoch, _) = queue.take_deferred().unwrap();
        queue.begin(target("new"), AgentNavigationSource::History);
        assert!(!queue.is_current(old_epoch));
        let (new_epoch, deferred) = queue.take_deferred().unwrap();
        assert!(queue.is_current(new_epoch));
        assert_eq!(deferred.session_id, "new");
        assert_eq!(queue.source(), Some(AgentNavigationSource::History));
    }

    #[test]
    fn sidebar_click_replaces_dashboard_focus_and_keeps_exact_host_identity() {
        let mut queue = AgentNavigationQueue::default();
        queue.begin(target("dashboard"), AgentNavigationSource::Dashboard);
        let (old_epoch, _) = queue.take_deferred().expect("old dashboard focus");

        // Clicking the project sidebar is a newer explicit Tab selection.
        queue.begin(target("sidebar"), AgentNavigationSource::Sidebar);
        assert!(!queue.is_current(old_epoch));
        assert_eq!(queue.source(), Some(AgentNavigationSource::Sidebar));
        let (epoch, selected) = queue.take_deferred().expect("sidebar focus");
        assert!(queue.is_current(epoch));
        assert_eq!(selected.session_id, "sidebar");
        assert!(exact_navigation_focus_matches(
            &selected,
            Some(&running_session("sidebar")),
            true,
        ));
        assert!(!exact_navigation_focus_matches(
            &selected,
            Some(&running_session("sidebar")),
            false,
        ));

        // A later manual Workspace/Tab action must cancel this deferred click.
        queue.cancel();
        assert!(!queue.is_current(epoch));
        assert!(queue.take_deferred().is_none());
    }

    #[test]
    fn post_workspace_switch_requeues_only_original_verified_target() {
        let mut queue = AgentNavigationQueue::default();
        queue.begin(target("exact"), AgentNavigationSource::History);
        let (before, verified_target) = queue.take_deferred().unwrap();
        queue.cancel(); // switch_workspace completed
        queue.begin(verified_target, AgentNavigationSource::History);
        assert!(!queue.is_current(before));
        let (after, same) = queue.take_deferred().unwrap();
        assert!(queue.is_current(after));
        assert_eq!(same, target("exact"));
    }

    #[test]
    fn bounded_host_retry_reuses_identity_and_ignores_render_frames() {
        let mut queue = AgentNavigationQueue::default();
        queue.begin(target("exact"), AgentNavigationSource::Dashboard);
        let (epoch, exact) = queue.take_deferred().unwrap();
        queue.hold(exact.clone());
        let (held_epoch, held) = queue.take_deferred().unwrap();
        assert_eq!(epoch, held_epoch);
        assert_eq!(held, exact);
        for _ in 0..MAX_HOST_RECHECKS {
            assert!(queue.retry(exact.clone()));
            assert_eq!(queue.take_deferred().unwrap().1, exact);
        }
        assert!(!queue.retry(exact));
        queue.cancel();
        assert!(!queue.is_current(epoch));
    }

    #[test]
    fn earlier_history_recheck_cannot_queue_after_user_changed_focus() {
        let mut queue = AgentNavigationQueue::default();
        let observed = queue.epoch();
        queue.cancel(); // user focused an unrelated tab while Host rechecked
        assert_ne!(observed, queue.epoch());
        queue.begin(target("later"), AgentNavigationSource::Toast);
        assert!(!queue.is_current(observed));
    }
    #[test]
    fn leaving_history_without_switching_tabs_invalidates_read_and_deferred_focus() {
        let mut queue = AgentNavigationQueue::default();
        let in_flight_history_epoch = queue.epoch();
        queue.cancel(); // Back to dashboard during a read-only Host recheck
        assert_ne!(queue.epoch(), in_flight_history_epoch);

        queue.begin(target("original"), AgentNavigationSource::History);
        let (deferred_epoch, _) = queue.take_deferred().unwrap();
        queue.cancel(); // Back before the deferred Window callback
        assert!(!queue.is_current(deferred_epoch));
        assert!(queue.take_deferred().is_none());
    }
}
