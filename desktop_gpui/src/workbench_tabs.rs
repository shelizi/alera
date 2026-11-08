//! Mixed workbench tab order and focus, independent from native UI rendering.
//!
//! Host terminals (including Host-backed agents) and editor buffers retain
//! their existing owners. This projection owns only tab-strip order/focus.
//! Closing a preview never becomes a Runtime Host PTY termination request.

use crate::terminal::TerminalTabContextTarget;

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
}

/// A single ordered strip for all three kinds of tabs. Reconciliation takes
/// observed identities from the original owners; it cannot create, close or
/// move an underlying editor buffer or PTY.
#[derive(Debug, Default)]
pub struct WorkbenchTabs {
    workspace_generation: u64,
    tabs: Vec<WorkbenchTab>,
    active: Option<WorkbenchTabIdentity>,
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

    pub fn reset(&mut self, generation: u64) {
        self.workspace_generation = generation;
        self.tabs.clear();
        self.active = None;
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
    use crate::terminal::{TerminalDock, TerminalSession};

    fn host(handle: &str, agent: bool) -> (WorkbenchTab, TerminalDock) {
        let mut dock = TerminalDock::for_workspace(7);
        dock.update_sessions(vec![TerminalSession {
            handle: handle.into(),
            workspace_id: "ws".into(),
            tab_id: format!("tab-{handle}"),
            running: true,
            agent_type: agent.then(|| "Codex".to_string()),
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
        }
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
