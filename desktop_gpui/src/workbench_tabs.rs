//! Mixed workbench tab order and focus, independent from native UI rendering.
//!
//! Host terminals (including Host-backed agents) and editor buffers retain
//! their existing owners. This projection owns only tab-strip order/focus.
//! Closing a preview never becomes a Runtime Host PTY termination request.

use crate::terminal::{AgentActivityState, TerminalSession, TerminalTabContextTarget};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkbenchTabIdentity {
    Host(TerminalTabContextTarget),
    Editor { editor_epoch: u64, instance: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkbenchTabKind {
    Terminal,
    Agent,
    Editor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkbenchTab {
    pub identity: WorkbenchTabIdentity,
    pub kind: WorkbenchTabKind,
    pub title: String,
    pub dirty: bool,
    pub running: bool,
    pub agent_state: Option<AgentActivityState>,
    pub interrupted: bool,
}

/// A single ordered strip for all three kinds of tabs. Reconciliation takes
/// observed identities from the original owners; it cannot create, close or
/// move an underlying editor buffer or PTY.
#[derive(Debug, Default)]
pub struct WorkbenchTabs {
    workspace_generation: u64,
    tabs: Vec<WorkbenchTab>,
    active: Option<WorkbenchTabIdentity>,
    /// Native view-only pin projection. Never owns/terminates a Host PTY.
    pinned: Vec<WorkbenchTabIdentity>,
}

impl WorkbenchTabs {
    pub fn for_workspace(generation: u64) -> Self {
        Self {
            workspace_generation: generation,
            ..Self::default()
        }
    }

    pub fn entries(&self) -> &[WorkbenchTab] {
        &self.tabs
    }

    pub fn active(&self) -> Option<&WorkbenchTabIdentity> {
        self.active.as_ref()
    }

    pub fn pinned(&self) -> &[WorkbenchTabIdentity] {
        &self.pinned
    }

    /// Refresh only Hook-derived presentation fields after an authenticated
    /// Host session list. Unlike full reconciliation this preserves tab order,
    /// pane allocation, selection, pinning, and all Host/PTY ownership.
    ///
    /// A recycled handle is not a match unless its workspace and Host Tab ID
    /// also match the original identity. On a lost Host connection, passing an
    /// empty list clears unverified status instead of showing stale "working".
    pub fn refresh_agent_statuses(&mut self, sessions: &[TerminalSession]) -> bool {
        let mut changed = false;
        for tab in &mut self.tabs {
            let WorkbenchTabIdentity::Host(target) = &tab.identity else {
                continue;
            };
            let exact = sessions.iter().find(|session| {
                session.running
                    && session.agent_type.is_some()
                    && session.handle == target.handle
                    && session.workspace_id == target.workspace_id()
                    && session.tab_id == target.tab_id()
            });
            let state = exact.and_then(|session| session.agent_state);
            let interrupted = exact.is_some_and(|session| {
                session.interrupted && state == Some(AgentActivityState::Done)
            });
            if tab.agent_state != state || tab.interrupted != interrupted {
                tab.agent_state = state;
                tab.interrupted = interrupted;
                changed = true;
            }
        }
        changed
    }

    pub fn is_pinned(&self, id: &WorkbenchTabIdentity) -> bool {
        self.pinned.contains(id)
    }

    /// Stale Editor epochs and recycled Host identities cannot be pinned.
    pub fn set_pinned(&mut self, id: &WorkbenchTabIdentity, pinned: bool) -> bool {
        if !self.contains(id) || self.is_pinned(id) == pinned {
            return false;
        }
        if pinned {
            self.pinned.push(id.clone());
        } else {
            self.pinned.retain(|candidate| candidate != id);
        }
        true
    }

    /// Stable, pane-ordered groups; no tab can appear in both visual rows.
    pub fn partition_pins(
        &self,
        tabs: Vec<WorkbenchTab>,
    ) -> (Vec<WorkbenchTab>, Vec<WorkbenchTab>) {
        tabs.into_iter()
            .partition(|tab| self.is_pinned(&tab.identity))
    }

    pub fn reset(&mut self, generation: u64) {
        self.workspace_generation = generation;
        self.tabs.clear();
        self.active = None;
        self.pinned.clear();
    }

    /// Preserve mixed order for surviving identities and append new tabs.
    /// The selected underlying editor/Host session determines focus so that
    /// clicks in either legacy local strip remain synchronized.
    pub fn reconcile(
        &mut self,
        generation: u64,
        observed: Vec<WorkbenchTab>,
        preferred: Option<WorkbenchTabIdentity>,
    ) {
        if self.workspace_generation != generation {
            self.reset(generation);
        }
        self.tabs
            .retain(|tab| observed.iter().any(|item| item.identity == tab.identity));
        self.pinned
            .retain(|id| observed.iter().any(|tab| &tab.identity == id));
        for item in observed {
            if let Some(existing) = self
                .tabs
                .iter_mut()
                .find(|tab| tab.identity == item.identity)
            {
                *existing = item;
            } else {
                self.tabs.push(item);
            }
        }
        if let Some(preferred) = preferred.filter(|id| self.contains(id)) {
            self.active = Some(preferred);
        } else if !self.active.as_ref().is_some_and(|id| self.contains(id)) {
            self.active = self.tabs.first().map(|tab| tab.identity.clone());
        }
    }

    pub fn contains(&self, id: &WorkbenchTabIdentity) -> bool {
        self.tabs.iter().any(|tab| &tab.identity == id)
    }

    /// Keep the mixed strip's highlighted tab in step with actual pane focus.
    /// Selecting another tab never changes the underlying Host/Editor owner.
    pub fn select(&mut self, id: &WorkbenchTabIdentity) -> bool {
        if !self.contains(id) || self.active.as_ref() == Some(id) {
            return false;
        }
        self.active = Some(id.clone());
        true
    }

    /// A stale drag source or target is ignored, never redirected to a tab
    /// which happens to occupy the same index after a close/reopen.
    /// Move a tab into the target's slot (the same drop semantics as the
    /// existing native Editor/Terminal strips).
    pub fn move_to(
        &mut self,
        source: &WorkbenchTabIdentity,
        target: &WorkbenchTabIdentity,
    ) -> bool {
        let Some(from) = self.tabs.iter().position(|item| &item.identity == source) else {
            return false;
        };
        let Some(to) = self.tabs.iter().position(|item| &item.identity == target) else {
            return false;
        };
        if from == to {
            return false;
        }
        let item = self.tabs.remove(from);
        self.tabs.insert(to, item);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::{AgentActivityState, TerminalDock, TerminalSession};

    fn host(handle: &str, agent: bool) -> (WorkbenchTab, TerminalDock) {
        let mut dock = TerminalDock::for_workspace(7);
        dock.update_sessions(vec![TerminalSession {
            handle: handle.into(),
            workspace_id: "ws".into(),
            tab_id: format!("tab-{handle}"),
            running: true,
            agent_type: agent.then(|| "Codex".to_string()),
            agent_state: None,
            interrupted: false,
        }]);
        dock.open(handle);
        (
            WorkbenchTab {
                identity: WorkbenchTabIdentity::Host(dock.context_target_at(0).unwrap()),
                kind: if agent {
                    WorkbenchTabKind::Agent
                } else {
                    WorkbenchTabKind::Terminal
                },
                title: handle.into(),
                dirty: false,
                running: true,
                agent_state: None,
                interrupted: false,
            },
            dock,
        )
    }

    fn editor(epoch: u64, instance: u64) -> WorkbenchTab {
        WorkbenchTab {
            identity: WorkbenchTabIdentity::Editor {
                editor_epoch: epoch,
                instance,
            },
            kind: WorkbenchTabKind::Editor,
            title: format!("file-{instance}"),
            dirty: false,
            running: true,
            agent_state: None,
            interrupted: false,
        }
    }

    #[test]
    fn pin_state_survives_metadata_refresh_but_not_close_or_workspace_switch() {
        let first = editor(3, 1);
        let second = editor(3, 2);
        let (host, _) = host("pty-1", false);
        let mut tabs = WorkbenchTabs::for_workspace(7);
        tabs.reconcile(7, vec![first.clone(), host.clone(), second.clone()], None);
        assert!(tabs.set_pinned(&host.identity, true));
        assert!(tabs.set_pinned(&second.identity, true));
        assert!(!tabs.set_pinned(&second.identity, true));
        assert!(!tabs.set_pinned(&editor(9, 1).identity, true));
        let (pinned, regular) = tabs.partition_pins(tabs.entries().to_vec());
        assert_eq!(
            pinned.iter().map(|tab| &tab.identity).collect::<Vec<_>>(),
            vec![&host.identity, &second.identity]
        );
        assert_eq!(regular[0].identity, first.identity);
        let mut changed = host.clone();
        changed.title = "Updated title".into();
        tabs.reconcile(7, vec![changed, second.clone(), first.clone()], None);
        assert!(tabs.is_pinned(&host.identity));
        tabs.reconcile(7, vec![second.clone(), first.clone()], None);
        assert!(!tabs.is_pinned(&host.identity));
        assert!(tabs.set_pinned(&second.identity, false));
        assert!(tabs.set_pinned(&first.identity, true));
        tabs.reconcile(8, vec![first.clone()], None);
        assert!(!tabs.is_pinned(&first.identity));
    }

    #[test]
    fn mixed_terminal_agent_editor_reconcile_keeps_user_order_and_host_ownership() {
        let (terminal, original_host) = host("pty-1", false);
        let (agent, agent_host) = host("pty-agent", true);
        let file = editor(2, 1);
        let mut tabs = WorkbenchTabs::for_workspace(7);
        tabs.reconcile(
            7,
            vec![terminal.clone(), file.clone(), agent.clone()],
            Some(file.identity.clone()),
        );
        assert_eq!(tabs.active(), Some(&file.identity));
        assert!(tabs.move_to(&agent.identity, &terminal.identity));
        tabs.reconcile(
            7,
            vec![terminal.clone(), file.clone(), agent.clone()],
            Some(agent.identity.clone()),
        );
        assert_eq!(tabs.entries()[0].kind, WorkbenchTabKind::Agent);
        assert_eq!(tabs.active(), Some(&agent.identity));
        assert_eq!(original_host.sessions.len(), 1);
        assert_eq!(agent_host.sessions.len(), 1);
    }

    #[test]
    fn agent_hook_status_changes_update_the_same_tab_without_claiming_pty_ownership() {
        let (mut agent, host) = host("pty-agent-1", true);
        let id = agent.identity.clone();
        let mut tabs = WorkbenchTabs::for_workspace(7);
        agent.agent_state = Some(AgentActivityState::Waiting);
        tabs.reconcile(7, vec![agent.clone()], Some(id.clone()));
        assert_eq!(
            tabs.entries()[0].agent_state,
            Some(AgentActivityState::Waiting)
        );
        assert!(tabs.entries()[0].running);
        agent.agent_state = Some(AgentActivityState::Blocked);
        tabs.reconcile(7, vec![agent.clone()], Some(id.clone()));
        assert_eq!(tabs.entries().len(), 1);
        assert_eq!(
            tabs.entries()[0].agent_state,
            Some(AgentActivityState::Blocked)
        );
        agent.agent_state = None;
        tabs.reconcile(7, vec![agent], Some(id.clone()));
        assert_eq!(tabs.entries()[0].agent_state, None);
        assert_eq!(tabs.active(), Some(&id));
        assert_eq!(
            host.sessions.len(),
            1,
            "status refresh must not close a Host PTY"
        );
    }

    #[test]
    fn agent_hook_refresh_only_updates_exact_host_metadata_without_stealing_tab_focus() {
        let (agent, dock) = host("pty-agent-1", true);
        let file = editor(9, 42);
        let mut tabs = WorkbenchTabs::for_workspace(7);
        tabs.reconcile(
            7,
            vec![agent.clone(), file.clone()],
            Some(file.identity.clone()),
        );
        assert!(tabs.set_pinned(&agent.identity, true));
        let mut live = dock.sessions[0].clone();
        live.agent_state = Some(AgentActivityState::Waiting);
        assert!(tabs.refresh_agent_statuses(&[live.clone()]));
        assert_eq!(
            tabs.entries()[0].agent_state,
            Some(AgentActivityState::Waiting)
        );
        assert_eq!(tabs.active(), Some(&file.identity));
        assert_eq!(tabs.entries()[0].identity, agent.identity);
        assert_eq!(tabs.entries()[1].identity, file.identity);
        assert!(tabs.is_pinned(&agent.identity));
        assert!(!tabs.refresh_agent_statuses(&[live.clone()]));
        live.agent_state = Some(AgentActivityState::Blocked);
        assert!(tabs.refresh_agent_statuses(&[live.clone()]));
        assert_eq!(
            tabs.entries()[0].agent_state,
            Some(AgentActivityState::Blocked)
        );
        // The handle is the same but its Host Tab identity changed: reject it.
        live.tab_id = "recycled-tab".into();
        assert!(tabs.refresh_agent_statuses(&[live]));
        assert_eq!(tabs.entries()[0].agent_state, None);
        assert_eq!(tabs.active(), Some(&file.identity));
        assert!(tabs.is_pinned(&agent.identity));
        assert!(!tabs.refresh_agent_statuses(&[]));
        assert_eq!(
            dock.sessions.len(),
            1,
            "status changes never terminate PTYs"
        );
    }

    #[test]
    fn stale_editor_mount_or_host_generation_cannot_reorder_another_tab() {
        let (old_host, _) = host("pty-1", false);
        let old_editor = editor(2, 1);
        let new_editor = editor(3, 1);
        let mut tabs = WorkbenchTabs::for_workspace(7);
        tabs.reconcile(
            7,
            vec![old_host.clone(), old_editor.clone()],
            Some(old_editor.identity.clone()),
        );
        tabs.reconcile(
            7,
            vec![old_host.clone(), new_editor.clone()],
            Some(new_editor.identity.clone()),
        );
        assert!(!tabs.move_to(&old_editor.identity, &old_host.identity));
        let (recycled_host, _) = host("pty-1", true);
        tabs.reconcile(
            8,
            vec![recycled_host.clone(), new_editor.clone()],
            Some(recycled_host.identity.clone()),
        );
        assert!(tabs
            .entries()
            .iter()
            .any(|item| item.kind == WorkbenchTabKind::Agent));
        assert!(!tabs.contains(&old_editor.identity));
    }

    #[test]
    fn selecting_a_tab_updates_the_mixed_active_indicator() {
        let first = editor(1, 1);
        let second = editor(1, 2);
        let mut tabs = WorkbenchTabs::for_workspace(2);
        tabs.reconcile(2, vec![first.clone(), second.clone()], None);
        assert_eq!(tabs.active(), Some(&first.identity));
        assert!(tabs.select(&second.identity));
        assert_eq!(tabs.active(), Some(&second.identity));
        assert!(!tabs.select(&second.identity));
        assert!(!tabs.select(&editor(1, 99).identity));
        assert_eq!(tabs.active(), Some(&second.identity));
    }

    #[test]
    fn stale_focus_after_last_close_falls_back_to_surviving_tab() {
        let first = editor(1, 1);
        let second = editor(1, 2);
        let mut tabs = WorkbenchTabs::for_workspace(1);
        tabs.reconcile(
            1,
            vec![first.clone(), second.clone()],
            Some(first.identity.clone()),
        );
        tabs.reconcile(1, vec![second.clone()], None);
        assert_eq!(tabs.active(), Some(&second.identity));
        tabs.reconcile(1, Vec::new(), None);
        assert_eq!(tabs.active(), None);
    }

    #[test]
    fn moving_to_the_next_slot_reorders_and_preserves_selected_identity() {
        let first = editor(1, 1);
        let second = editor(1, 2);
        let third = editor(1, 3);
        let mut tabs = WorkbenchTabs::for_workspace(4);
        tabs.reconcile(
            4,
            vec![first.clone(), second.clone(), third.clone()],
            Some(first.identity.clone()),
        );
        assert!(tabs.move_to(&first.identity, &second.identity));
        assert_eq!(tabs.entries()[0].identity, second.identity);
        assert_eq!(tabs.entries()[1].identity, first.identity);
        assert_eq!(tabs.active(), Some(&first.identity));
        assert!(tabs.move_to(&first.identity, &third.identity));
        assert_eq!(tabs.entries()[2].identity, first.identity);
        assert!(tabs.move_to(&first.identity, &second.identity));
        assert_eq!(tabs.entries()[0].identity, first.identity);
        assert!(!tabs.move_to(&first.identity, &first.identity));
    }
}
