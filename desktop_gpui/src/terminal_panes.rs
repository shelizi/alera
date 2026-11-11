//! Two native GPUI terminal preview panes share Host-owned sessions. Changing
//! the split only moves local preview focus; it never kills, detaches, resizes
//! or duplicates a Runtime Host PTY.
use crate::terminal::TerminalDock;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TerminalSplitDirection {
    #[default]
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalPaneIdentity {
    pub handle: String,
    pub workspace_id: String,
    pub tab_id: String,
}

#[derive(Debug, Default)]
pub struct TerminalPaneLayout {
    pub secondary: Option<TerminalPaneIdentity>,
    pub direction: TerminalSplitDirection,
}

impl TerminalPaneLayout {
    /// Open a second preview for an already attached, different live tab.
    /// Host tab identity is pinned to prevent reused handles from appearing.
    pub fn split(&mut self, dock: &TerminalDock) -> bool {
        if self.secondary.is_some() {
            return true;
        }
        let Some(active) = dock.active() else {
            return false;
        };
        let Some(next) = dock
            .tabs
            .iter()
            .find(|tab| tab.handle != active.handle && tab.running)
        else {
            return false;
        };
        self.secondary = Some(TerminalPaneIdentity {
            handle: next.handle.clone(),
            workspace_id: next.workspace_id.clone(),
            tab_id: next.tab_id.clone(),
        });
        true
    }

    pub fn secondary_index(&self, dock: &TerminalDock) -> Option<usize> {
        let secondary = self.secondary.as_ref()?;
        dock.tabs.iter().position(|tab| {
            tab.handle == secondary.handle
                && tab.workspace_id == secondary.workspace_id
                && tab.tab_id == secondary.tab_id
        })
    }

    /// Assign another attached, still-live Host tab to the second preview.
    /// Dragging the focused tab swaps the current and secondary panes.
    pub fn drop_tab_on_secondary(&mut self, dock: &mut TerminalDock, handle: &str) -> bool {
        let Some(active) = dock.active() else {
            return false;
        };
        if active.handle == handle {
            return self.switch_focus(dock);
        }
        let Some(tab) = dock.tabs.iter().find(|tab| {
            tab.handle == handle
                && tab.running
                && dock.sessions.iter().any(|live| {
                    live.handle == tab.handle
                        && live.workspace_id == tab.workspace_id
                        && live.tab_id == tab.tab_id
                })
        }) else {
            return false;
        };
        self.secondary = Some(TerminalPaneIdentity {
            handle: tab.handle.clone(),
            workspace_id: tab.workspace_id.clone(),
            tab_id: tab.tab_id.clone(),
        });
        true
    }

    pub fn switch_focus(&mut self, dock: &mut TerminalDock) -> bool {
        let Some(second) = self.secondary_index(dock) else {
            self.secondary = None;
            return false;
        };
        let Some(active) = dock.active() else {
            return false;
        };
        let previous = TerminalPaneIdentity {
            handle: active.handle.clone(),
            workspace_id: active.workspace_id.clone(),
            tab_id: active.tab_id.clone(),
        };
        if !dock.activate(second) {
            return false;
        }
        self.secondary = Some(previous);
        true
    }

    pub fn toggle_direction(&mut self) -> TerminalSplitDirection {
        self.direction = match self.direction {
            TerminalSplitDirection::Horizontal => TerminalSplitDirection::Vertical,
            TerminalSplitDirection::Vertical => TerminalSplitDirection::Horizontal,
        };
        self.direction
    }

    pub fn close_secondary(&mut self) {
        self.secondary = None;
    }

    pub fn reset(&mut self) {
        self.secondary = None;
        self.direction = TerminalSplitDirection::Horizontal;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::TerminalSession;

    fn session(handle: &str) -> TerminalSession {
        TerminalSession {
            handle: handle.to_string(),
            workspace_id: "ws".into(),
            tab_id: format!("tab-{handle}"),
            running: true,
            agent_type: None,
            agent_state: None,
            interrupted: false,
            agent_state_started_at: None,
            agent_updated_at: None,
        }
    }

    #[test]
    fn split_requires_two_real_tabs_and_swap_preserves_session_ownership() {
        let mut dock = TerminalDock::default();
        dock.update_sessions(vec![session("one"), session("two")]);
        dock.open("one");
        let mut panes = TerminalPaneLayout::default();
        assert!(!panes.split(&dock));
        dock.open("two");
        dock.open("one");
        assert!(panes.split(&dock));
        assert_eq!(panes.secondary_index(&dock), Some(1));
        assert!(panes.switch_focus(&mut dock));
        assert_eq!(dock.active().unwrap().handle, "two");
        assert_eq!(panes.secondary.as_ref().unwrap().handle, "one");
        assert_eq!(dock.sessions.len(), 2);
        panes.close_secondary();
        assert!(panes.secondary.is_none());
        assert_eq!(
            dock.sessions.len(),
            2,
            "closing split never terminates PTYs"
        );
    }

    #[test]
    fn drag_to_secondary_reassigns_and_swaps_without_duplicate_pty() {
        let mut dock = TerminalDock::default();
        dock.update_sessions(vec![session("one"), session("two"), session("three")]);
        dock.open("one");
        dock.open("two");
        dock.open("three");
        dock.open("one");
        let mut panes = TerminalPaneLayout::default();
        assert!(panes.split(&dock));
        assert_eq!(panes.secondary.as_ref().unwrap().handle, "two");
        assert!(panes.drop_tab_on_secondary(&mut dock, "three"));
        assert_eq!(panes.secondary.as_ref().unwrap().handle, "three");
        assert_eq!(dock.active().unwrap().handle, "one");
        assert!(panes.drop_tab_on_secondary(&mut dock, "one"));
        assert_eq!(dock.active().unwrap().handle, "three");
        assert_eq!(panes.secondary.as_ref().unwrap().handle, "one");
        assert!(!panes.drop_tab_on_secondary(&mut dock, "absent"));
        assert_eq!(
            dock.sessions.len(),
            3,
            "dragging tabs never kills Host PTYs"
        );
    }

    #[test]
    fn stale_or_reused_session_cannot_occupy_secondary_pane() {
        let mut dock = TerminalDock::default();
        dock.update_sessions(vec![session("one"), session("two")]);
        dock.open("two");
        dock.open("one");
        let mut panes = TerminalPaneLayout::default();
        assert!(panes.split(&dock));
        dock.sessions = vec![session("one"), session("two")];
        dock.sessions[1].tab_id = "reused".into();
        dock.open("two");
        assert!(panes.secondary_index(&dock).is_none());
    }
}
