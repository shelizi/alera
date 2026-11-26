//! Mixed workbench tab order and focus, independent from native UI rendering.
//!
//! Host terminals (including Host-backed agents) and editor buffers retain
//! their existing owners. This projection owns only tab-strip order/focus.
//! Closing a preview never becomes a Runtime Host PTY termination request.

use crate::terminal::{AgentActivityState, TerminalSession, TerminalTabContextTarget};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkbenchTabIdentity {
    Host(TerminalTabContextTarget),
    Editor {
        editor_epoch: u64,
        instance: u64,
    },
    /// Workspace-local, read-only Git History view. Never a Host or editor owner.
    GitHistory,
    /// A distinct immutable comparison snapshot, not an editor or Host owner.
    GitFileCompare(u64),
    /// A pinned, immutable Base/Ours/Theirs conflict view. Never a Git action,
    /// editor document, or Host-owned terminal.
    GitConflictCompare(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkbenchTabKind {
    Terminal,
    Agent,
    Editor,
    GitHistory,
    GitFileCompare,
    GitConflictCompare,
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

impl WorkbenchTab {
    /// Brand the Tab only from a current authenticated Host session matching
    /// the full stable identity. A recycled handle, stale Workspace/Tab ID,
    /// closed PTY or generic Terminal must never inherit another Agent logo.
    /// This is a display-only lookup and does not alter any Host ownership.
    pub fn verified_agent_brand<'a>(&self, sessions: &'a [TerminalSession]) -> Option<&'a str> {
        if self.kind != WorkbenchTabKind::Agent || !self.running {
            return None;
        }
        let WorkbenchTabIdentity::Host(target) = &self.identity else {
            return None;
        };
        sessions
            .iter()
            .find(|session| {
                session.running
                    && session.handle == target.handle
                    && session.workspace_id == target.workspace_id()
                    && session.tab_id == target.tab_id()
            })
            .and_then(|session| session.agent_type.as_deref())
            .filter(|kind| !kind.trim().is_empty())
    }

    /// Only a current, matching live Host Terminal may supply an MCP indicator.
    pub fn verified_builtin_mcp_status<'a>(
        &self,
        sessions: &'a [TerminalSession],
    ) -> Option<&'a str> {
        if !self.running {
            return None;
        }
        let WorkbenchTabIdentity::Host(target) = &self.identity else {
            return None;
        };
        sessions
            .iter()
            .find(|session| {
                session.running
                    && session.handle == target.handle
                    && session.workspace_id == target.workspace_id()
                    && session.tab_id == target.tab_id()
            })
            .and_then(|session| session.builtin_mcp_status.as_deref())
    }

    /// Human-readable status anchored to the verified Host identity. The
    /// tooltip never reports a Hook for a stopped PTY, generic terminal or
    /// Editor; labels contain no Agent prompt, tool input or credentials.
    pub fn agent_status_tooltip(&self) -> Option<String> {
        if self.kind != WorkbenchTabKind::Agent || !self.running {
            return None;
        }
        let state = self.agent_state?;
        let WorkbenchTabIdentity::Host(target) = &self.identity else {
            return None;
        };
        let state_label = match (state, self.interrupted) {
            (AgentActivityState::Working, _) => "Agent working",
            (AgentActivityState::Waiting, _) => "Agent waiting",
            (AgentActivityState::Blocked, _) => "Agent blocked",
            (AgentActivityState::Done, true) => "Agent interrupted",
            (AgentActivityState::Done, false) => "Agent done",
        };
        Some(format!(
            "{state_label} · {}\nHost session: {} · Workspace: {} · Tab: {}",
            self.title,
            target.handle,
            target.workspace_id(),
            target.tab_id()
        ))
    }

    /// A stopped Host preview retains its last screen but must not look live.
    /// Editors are not Host PTYs, even if stale projection says otherwise.
    pub fn stopped_label(&self) -> Option<&'static str> {
        if !self.running && matches!(&self.identity, WorkbenchTabIdentity::Host(_)) {
            Some("Stopped")
        } else {
            None
        }
    }
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

    /// Drop a live Tab onto the empty trailing strip space. This changes
    /// presentation order only: the selected identity and Host/Editor owners
    /// remain unchanged. Unknown or already-last tabs are no-ops.
    pub fn move_to_end(&mut self, id: &WorkbenchTabIdentity) -> bool {
        let Some(index) = self.tabs.iter().position(|tab| &tab.identity == id) else {
            return false;
        };
        if index + 1 == self.tabs.len() {
            return false;
        }
        let entry = self.tabs.remove(index);
        self.tabs.push(entry);
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
            agent_state_started_at: None,
            agent_updated_at: None,
            agent_context: None,
            builtin_mcp_status: None,
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
    fn git_history_is_one_view_only_tab_and_pin_does_not_survive_close() {
        let graph = WorkbenchTab {
            identity: WorkbenchTabIdentity::GitHistory,
            kind: WorkbenchTabKind::GitHistory,
            title: "Git Graph".into(),
            dirty: false,
            running: true,
            agent_state: None,
            interrupted: false,
        };
        let editor = editor(7, 1);
        let mut tabs = WorkbenchTabs::for_workspace(7);
        tabs.reconcile(7, vec![editor.clone(), graph.clone()], None);
        assert!(tabs.select(&graph.identity));
        assert!(tabs.set_pinned(&graph.identity, true));
        assert!(tabs.move_to(&graph.identity, &editor.identity));
        assert!(tabs.is_pinned(&graph.identity));
        tabs.reconcile(7, vec![editor.clone(), graph.clone()], None);
        assert_eq!(tabs.active(), Some(&graph.identity));
        assert!(tabs.is_pinned(&graph.identity));
        tabs.reconcile(7, vec![editor.clone()], None);
        assert!(!tabs.contains(&graph.identity));
        assert!(!tabs.is_pinned(&graph.identity));
        tabs.reconcile(8, vec![editor, graph.clone()], None);
        assert!(!tabs.is_pinned(&graph.identity));
    }

    #[test]
    fn brand_icon_requires_exact_live_agent_host_identity() {
        let (tab, dock) = host("pty-agent", true);
        assert_eq!(tab.verified_agent_brand(&dock.sessions), Some("Codex"));
        let mut stale = dock.sessions[0].clone();
        stale.workspace_id = "different-workspace".into();
        assert_eq!(tab.verified_agent_brand(&[stale.clone()]), None);
        stale = dock.sessions[0].clone();
        stale.tab_id = "reused-tab".into();
        assert_eq!(tab.verified_agent_brand(&[stale.clone()]), None);
        stale = dock.sessions[0].clone();
        stale.handle = "another-session".into();
        assert_eq!(tab.verified_agent_brand(&[stale.clone()]), None);
        stale = dock.sessions[0].clone();
        stale.running = false;
        assert_eq!(tab.verified_agent_brand(&[stale.clone()]), None);
        stale = dock.sessions[0].clone();
        stale.agent_type = None;
        assert_eq!(tab.verified_agent_brand(&[stale]), None);
        let mut stopped_tab = tab.clone();
        stopped_tab.running = false;
        assert_eq!(stopped_tab.verified_agent_brand(&dock.sessions), None);
        let (terminal, _) = host("pty-agent", false);
        assert_eq!(terminal.verified_agent_brand(&dock.sessions), None);
        assert_eq!(editor(7, 1).verified_agent_brand(&dock.sessions), None);
    }

    #[test]
    fn builtin_mcp_indicator_requires_live_exact_session_identity() {
        let (tab, dock) = host("pty-agent", true);
        let mut live = dock.sessions[0].clone();
        live.builtin_mcp_status = Some("connected".into());
        assert_eq!(tab.verified_builtin_mcp_status(&[live.clone()]), Some("connected"));
        let mut stale = live.clone();
        stale.workspace_id = "other".into();
        assert_eq!(tab.verified_builtin_mcp_status(&[stale]), None);
        let mut stale = live.clone();
        stale.tab_id = "reused".into();
        assert_eq!(tab.verified_builtin_mcp_status(&[stale]), None);
        let mut stale = live.clone();
        stale.running = false;
        assert_eq!(tab.verified_builtin_mcp_status(&[stale]), None);
        assert_eq!(editor(7, 1).verified_builtin_mcp_status(&[live]), None);
        assert_eq!(tab.verified_builtin_mcp_status(&dock.sessions), None);
    }

    #[test]
    fn brand_icon_ignores_stale_session_even_when_a_live_match_follows() {
        let (tab, dock) = host("pty-agent", true);
        let mut reused = dock.sessions[0].clone();
        reused.workspace_id = "other".into();
        reused.agent_type = Some("claude".into());
        assert_eq!(
            tab.verified_agent_brand(&[reused, dock.sessions[0].clone()]),
            Some("Codex")
        );
    }

    #[test]
    fn stopped_host_tab_has_explicit_label_but_editor_and_live_pty_do_not() {
        let (mut agent, _) = host("crashed-agent", true);
        assert_eq!(agent.stopped_label(), None);
        agent.running = false;
        assert_eq!(agent.stopped_label(), Some("Stopped"));
        let (mut terminal, _) = host("terminal", false);
        terminal.running = false;
        assert_eq!(terminal.stopped_label(), Some("Stopped"));
        let mut file = editor(7, 3);
        file.running = false;
        assert_eq!(file.stopped_label(), None);
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
    fn h01_agent_tooltip_reports_four_host_states_and_exact_tab_without_claiming_stopped_pty() {
        let (mut agent, _) = host("pty-42", true);
        assert!(agent.agent_status_tooltip().is_none());
        for (state, label) in [
            (AgentActivityState::Working, "Agent working"),
            (AgentActivityState::Waiting, "Agent waiting"),
            (AgentActivityState::Blocked, "Agent blocked"),
            (AgentActivityState::Done, "Agent done"),
        ] {
            agent.agent_state = Some(state);
            let tooltip = agent
                .agent_status_tooltip()
                .expect("Host-confirmed tooltip");
            assert!(tooltip.contains(label), "{tooltip}");
            assert!(tooltip.contains("pty-42"), "{tooltip}");
            assert!(tooltip.contains("tab-pty-42"), "{tooltip}");
            assert!(tooltip.contains("ws"), "{tooltip}");
        }
        agent.interrupted = true;
        assert!(agent
            .agent_status_tooltip()
            .unwrap()
            .contains("interrupted"));
        agent.running = false;
        assert!(
            agent.agent_status_tooltip().is_none(),
            "a stopped PTY cannot show a live Hook state"
        );
        let (mut shell, _) = host("pty-shell", false);
        shell.agent_state = Some(AgentActivityState::Waiting);
        assert!(
            shell.agent_status_tooltip().is_none(),
            "a plain terminal never claims an Agent Hook"
        );
        let mut editor = editor(7, 9);
        editor.agent_state = Some(AgentActivityState::Waiting);
        assert!(
            editor.agent_status_tooltip().is_none(),
            "editor Tabs never own Hook presence"
        );
    }

    #[test]
    fn h01_four_host_states_remain_in_original_split_pane_without_focus_or_pin_changes() {
        use crate::workbench_panes::{PaneSlot, WorkbenchPaneLayout};
        let (primary, primary_dock) = host("pty-primary", true);
        let (secondary, secondary_dock) = host("pty-secondary", true);
        let mut tabs = WorkbenchTabs::for_workspace(7);
        let identities = vec![primary.identity.clone(), secondary.identity.clone()];
        tabs.reconcile(
            7,
            vec![primary.clone(), secondary.clone()],
            Some(primary.identity.clone()),
        );
        let mut panes = WorkbenchPaneLayout::new();
        panes.reconcile(&identities);
        assert!(panes.split_with(&secondary.identity));
        assert!(tabs.set_pinned(&primary.identity, true));
        let active = tabs.active().cloned();
        for (state, interrupted) in [
            (AgentActivityState::Working, false),
            (AgentActivityState::Waiting, false),
            (AgentActivityState::Blocked, false),
            (AgentActivityState::Done, true),
        ] {
            let mut one = primary_dock.sessions[0].clone();
            let mut two = secondary_dock.sessions[0].clone();
            one.agent_state = Some(AgentActivityState::Working);
            two.agent_state = Some(state);
            two.interrupted = interrupted;
            let _ = tabs.refresh_agent_statuses(&[one, two]);
            let primary_visible = panes.visible_tabs_in(PaneSlot::Primary, tabs.entries());
            let secondary_visible = panes.visible_tabs_in(PaneSlot::Secondary, tabs.entries());
            assert_eq!(primary_visible.len(), 1);
            assert_eq!(secondary_visible.len(), 1);
            assert_eq!(primary_visible[0].identity, primary.identity);
            assert_eq!(secondary_visible[0].identity, secondary.identity);
            assert_eq!(
                primary_visible[0].agent_state,
                Some(AgentActivityState::Working)
            );
            assert_eq!(secondary_visible[0].agent_state, Some(state));
            assert_eq!(secondary_visible[0].interrupted, interrupted);
            assert!(secondary_visible[0]
                .agent_status_tooltip()
                .unwrap()
                .contains("pty-secondary"));
            assert!(!secondary_visible[0]
                .agent_status_tooltip()
                .unwrap()
                .contains("pty-primary"));
            assert_eq!(tabs.active(), active.as_ref());
            assert_eq!(
                panes.active_in(PaneSlot::Secondary),
                Some(&secondary.identity)
            );
            assert!(tabs.is_pinned(&primary.identity));
        }
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
    fn blank_tail_drop_changes_order_not_selected_tab_or_pins() {
        let first = editor(3, 11);
        let second = editor(3, 12);
        let graph = WorkbenchTab {
            identity: WorkbenchTabIdentity::GitHistory,
            kind: WorkbenchTabKind::GitHistory,
            title: "Git Graph".into(),
            dirty: false,
            running: true,
            agent_state: None,
            interrupted: false,
        };
        let mut tabs = WorkbenchTabs::for_workspace(4);
        tabs.reconcile(
            4,
            vec![first.clone(), graph.clone(), second.clone()],
            Some(first.identity.clone()),
        );
        assert!(tabs.move_to_end(&first.identity));
        assert_eq!(tabs.entries()[2].identity, first.identity);
        assert_eq!(tabs.active(), Some(&first.identity));
        assert!(!tabs.move_to_end(&first.identity));
        assert!(!tabs.move_to_end(&WorkbenchTabIdentity::Editor {
            editor_epoch: 99,
            instance: 99
        }));
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
