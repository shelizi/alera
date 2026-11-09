//! Two-pane mixed workbench tab layout, independent from native UI rendering.
//!
//! Host terminals (including Host-backed agents) and editor buffers keep
//! their existing owners; [`WorkbenchTabs`](crate::workbench_tabs::WorkbenchTabs)
//! still owns strip metadata such as titles and kinds. This projection owns
//! only which pane each [`WorkbenchTabIdentity`] belongs to, the per-pane
//! strip order and selection, the focused pane, and the split axis. Moving a
//! tab between panes never creates, closes, duplicates or resizes a Runtime
//! Host PTY or editor buffer, and closing the secondary pane returns every
//! tab to the primary pane.

use crate::terminal::TerminalTabContextTarget;
use crate::workbench_tabs::WorkbenchTabIdentity;

/// Which of the two workbench panes a tab strip belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PaneSlot {
    /// The always-present main pane. Newly observed tabs are added here.
    #[default]
    Primary,
    /// The optional split pane. It exists exactly while it holds a tab.
    Secondary,
}

/// Split arrangement axis. This is a layout preference only; it never changes
/// tab ownership.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PaneAxis {
    #[default]
    Horizontal,
    Vertical,
}

/// Ordered tab identities for the primary and optional secondary pane, plus
/// per-pane selection, pane focus and the split axis.
///
/// Invariants maintained by every method:
///
/// - a [`WorkbenchTabIdentity`] appears in at most one pane;
/// - the secondary pane exists ([`Self::has_secondary`]) exactly while it is
///   non-empty;
/// - a non-empty pane always has an active identity that belongs to it, and an
///   empty pane has none.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct WorkbenchPaneLayout {
    primary: Vec<WorkbenchTabIdentity>,
    secondary: Vec<WorkbenchTabIdentity>,
    active_primary: Option<WorkbenchTabIdentity>,
    active_secondary: Option<WorkbenchTabIdentity>,
    focused: PaneSlot,
    axis: PaneAxis,
}

impl WorkbenchPaneLayout {
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop every pane assignment, selection and the split axis, e.g. when the
    /// workspace changes. The underlying tabs are owned elsewhere and are
    /// re-observed through [`Self::reconcile`].
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Reconcile pane membership against the identities the tab owners
    /// currently observe. Surviving tabs keep their pane and strip order,
    /// closed or stale identities are dropped, and newly observed identities
    /// are appended to the primary pane in observation order. Per-pane
    /// selections are preserved while their identity survives and repaired to
    /// the first surviving tab otherwise. A secondary pane left empty by the
    /// reconcile collapses and pane focus returns to the primary pane.
    pub fn reconcile(&mut self, observed: &[WorkbenchTabIdentity]) {
        self.primary.retain(|id| observed.contains(id));
        self.secondary.retain(|id| observed.contains(id));
        for id in observed {
            if !self.primary.contains(id) && !self.secondary.contains(id) {
                self.primary.push(id.clone());
            }
        }
        Self::repair(&self.primary, &mut self.active_primary);
        Self::repair(&self.secondary, &mut self.active_secondary);
        if self.secondary.is_empty() {
            self.active_secondary = None;
            if self.focused == PaneSlot::Secondary {
                self.focused = PaneSlot::Primary;
            }
        }
    }

    /// Split `id` off into the secondary pane, creating the split when needed.
    /// Equivalent to [`Self::move_to`] with [`PaneSlot::Secondary`]: the tab
    /// keeps its single identity — a Host PTY or editor buffer is never
    /// duplicated — becomes the secondary pane's active tab, and the
    /// secondary pane takes focus. Returns `false` for unknown identities and
    /// for tabs already in the secondary pane.
    pub fn split_with(&mut self, id: &WorkbenchTabIdentity) -> bool {
        self.move_to(id, PaneSlot::Secondary)
    }

    /// Move `id` into `slot`'s pane, creating the secondary pane on demand.
    /// The moved tab is appended to the target strip, becomes that pane's
    /// active tab, and the target pane takes focus. A secondary pane left
    /// empty collapses. Returns `false` for unknown identities and when the
    /// tab already sits in `slot`.
    pub fn move_to(&mut self, id: &WorkbenchTabIdentity, slot: PaneSlot) -> bool {
        let (source, index) = if let Some(index) = self.primary.iter().position(|tab| tab == id) {
            (PaneSlot::Primary, index)
        } else if let Some(index) = self.secondary.iter().position(|tab| tab == id) {
            (PaneSlot::Secondary, index)
        } else {
            return false;
        };
        if source == slot {
            return false;
        }
        match source {
            PaneSlot::Primary => {
                self.primary.remove(index);
                Self::repair(&self.primary, &mut self.active_primary);
            }
            PaneSlot::Secondary => {
                self.secondary.remove(index);
                Self::repair(&self.secondary, &mut self.active_secondary);
            }
        }
        match slot {
            PaneSlot::Primary => {
                self.primary.push(id.clone());
                self.active_primary = Some(id.clone());
            }
            PaneSlot::Secondary => {
                self.secondary.push(id.clone());
                self.active_secondary = Some(id.clone());
            }
        }
        self.focused = slot;
        true
    }

    /// Make `id` the active tab of its pane and focus that pane. Returns
    /// `false` for unknown identities and when the tab is already the active
    /// tab of the focused pane.
    pub fn activate(&mut self, id: &WorkbenchTabIdentity) -> bool {
        let (slot, active) = if self.primary.iter().any(|tab| tab == id) {
            (PaneSlot::Primary, &mut self.active_primary)
        } else if self.secondary.iter().any(|tab| tab == id) {
            (PaneSlot::Secondary, &mut self.active_secondary)
        } else {
            return false;
        };
        if active.as_ref() == Some(id) && self.focused == slot {
            return false;
        }
        *active = Some(id.clone());
        self.focused = slot;
        true
    }

    /// The active tab identity of `slot`'s pane, if the pane holds one.
    pub fn active_in(&self, slot: PaneSlot) -> Option<&WorkbenchTabIdentity> {
        match slot {
            PaneSlot::Primary => self.active_primary.as_ref(),
            PaneSlot::Secondary => self.active_secondary.as_ref(),
        }
    }

    /// Tab identities in `slot`'s strip order. The secondary pane reports an
    /// empty slice while no split exists.
    pub fn tabs_in(&self, slot: PaneSlot) -> &[WorkbenchTabIdentity] {
        match slot {
            PaneSlot::Primary => self.primary.as_slice(),
            PaneSlot::Secondary => self.secondary.as_slice(),
        }
    }

    /// The pane that currently owns keyboard focus.
    pub fn focused_pane(&self) -> PaneSlot {
        self.focused
    }

    /// The only Host ticket whose raw keyboard input is authorized from
    /// this pane. An unfocused pane or Editor tab can never inherit the
    /// globally selected TerminalDock keyboard destination.
    pub fn focused_host_target_for_input(
        &self,
        slot: PaneSlot,
    ) -> Option<&TerminalTabContextTarget> {
        if self.focused != slot {
            return None;
        }
        match self.active_in(slot) {
            Some(WorkbenchTabIdentity::Host(target)) => Some(target),
            _ => None,
        }
    }

    /// TextInput/IME commits can arrive after a mouse click moved focus.
    /// Recheck the *exact* Host identity before accepting any delayed text;
    /// a former pane or an Editor must never inherit the global PTY target.
    pub fn authorizes_terminal_input(
        &self,
        slot: PaneSlot,
        active_target: &TerminalTabContextTarget,
    ) -> bool {
        self.focused_host_target_for_input(slot) == Some(active_target)
    }

    /// Whether the optional secondary pane currently holds at least one tab.
    pub fn has_secondary(&self) -> bool {
        !self.secondary.is_empty()
    }

    /// Move pane focus to `slot`. Returns `false` for the secondary pane
    /// while no split exists and when `slot` already holds focus.
    pub fn focus_pane(&mut self, slot: PaneSlot) -> bool {
        if slot == PaneSlot::Secondary && !self.has_secondary() {
            return false;
        }
        if self.focused == slot {
            return false;
        }
        self.focused = slot;
        true
    }

    /// The non-focused mixed pane needs a separate Host poll. Only a live
    /// Host identity (not an Editor) may provide a terminal read target.
    pub fn background_host_target(&self) -> Option<&TerminalTabContextTarget> {
        if !self.has_secondary() {
            return None;
        }
        let background_slot = match self.focused {
            PaneSlot::Primary => PaneSlot::Secondary,
            PaneSlot::Secondary => PaneSlot::Primary,
        };
        match self.active_in(background_slot) {
            Some(WorkbenchTabIdentity::Host(target)) => Some(target),
            _ => None,
        }
    }

    /// Close the split: every secondary tab moves back to the primary pane,
    /// appended after the existing primary tabs in secondary strip order, and
    /// focus returns to the primary pane. No tab, Host PTY or editor buffer is
    /// closed. When the primary pane had no selection, the tab that was
    /// active in the secondary pane becomes the primary active tab.
    pub fn close_secondary(&mut self) {
        if self.secondary.is_empty() {
            return;
        }
        let secondary_active = self.active_secondary.take();
        self.primary.append(&mut self.secondary);
        if self.active_primary.is_none() {
            self.active_primary = secondary_active
                .filter(|id| self.primary.contains(id))
                .or_else(|| self.primary.first().cloned());
        }
        self.focused = PaneSlot::Primary;
    }

    /// Current split arrangement axis.
    pub fn axis(&self) -> PaneAxis {
        self.axis
    }

    /// Flip the split arrangement axis and return the new value. The axis is
    /// a layout preference; toggling never moves tabs.
    pub fn toggle_axis(&mut self) -> PaneAxis {
        self.axis = match self.axis {
            PaneAxis::Horizontal => PaneAxis::Vertical,
            PaneAxis::Vertical => PaneAxis::Horizontal,
        };
        self.axis
    }

    /// Keep `active` valid for `tabs`: preserve the selection while its
    /// identity survives, otherwise fall back to the first tab or `None`.
    fn repair(tabs: &[WorkbenchTabIdentity], active: &mut Option<WorkbenchTabIdentity>) {
        if !active.as_ref().is_some_and(|id| tabs.contains(id)) {
            *active = tabs.first().cloned();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::{TerminalDock, TerminalSession};
    use crate::workbench_tabs::WorkbenchTabIdentity;

    fn host(handle: &str) -> WorkbenchTabIdentity {
        let mut dock = TerminalDock::for_workspace(7);
        dock.update_sessions(vec![TerminalSession {
            handle: handle.into(),
            workspace_id: "ws".into(),
            tab_id: format!("tab-{handle}"),
            running: true,
            agent_type: None,
        }]);
        dock.open(handle);
        WorkbenchTabIdentity::Host(dock.context_target_at(0).unwrap())
    }

    fn editor(epoch: u64, instance: u64) -> WorkbenchTabIdentity {
        WorkbenchTabIdentity::Editor {
            editor_epoch: epoch,
            instance,
        }
    }

    fn expect(panes: &WorkbenchPaneLayout, slot: PaneSlot, expected: &[WorkbenchTabIdentity]) {
        assert_eq!(panes.tabs_in(slot), expected);
    }

    fn all_identities(panes: &WorkbenchPaneLayout) -> Vec<WorkbenchTabIdentity> {
        let mut ids = panes.tabs_in(PaneSlot::Primary).to_vec();
        ids.extend_from_slice(panes.tabs_in(PaneSlot::Secondary));
        ids
    }

    /// Every identity must live in exactly one pane; a Host identity value is
    /// the PTY's stable target, so equality here proves no PTY was duplicated.
    fn assert_unique(panes: &WorkbenchPaneLayout) {
        let ids = all_identities(panes);
        for (index, id) in ids.iter().enumerate() {
            assert!(!ids[..index].contains(id), "duplicated identity {id:?}");
        }
    }

    #[test]
    fn reconcile_orders_mixed_host_and_editor_tabs_in_primary() {
        let term = host("pty-1");
        let agent = host("pty-agent");
        let file_a = editor(2, 1);
        let file_b = editor(2, 2);
        let mut panes = WorkbenchPaneLayout::new();
        panes.reconcile(&[term.clone(), file_a.clone(), agent.clone(), file_b.clone()]);
        expect(
            &panes,
            PaneSlot::Primary,
            &[term.clone(), file_a.clone(), agent.clone(), file_b.clone()],
        );
        expect(&panes, PaneSlot::Secondary, &[]);
        assert_eq!(panes.active_in(PaneSlot::Primary), Some(&term));
        assert_eq!(panes.active_in(PaneSlot::Secondary), None);
        assert_eq!(panes.focused_pane(), PaneSlot::Primary);
        assert!(!panes.has_secondary());
        assert_unique(&panes);
    }

    #[test]
    fn split_moves_selected_identity_to_secondary_without_duplicating() {
        let term = host("pty-1");
        let file = editor(1, 1);
        let agent = host("pty-agent");
        let mut panes = WorkbenchPaneLayout::new();
        panes.reconcile(&[term.clone(), file.clone(), agent.clone()]);
        assert!(panes.split_with(&file));
        expect(&panes, PaneSlot::Primary, &[term.clone(), agent.clone()]);
        expect(&panes, PaneSlot::Secondary, &[file.clone()]);
        assert_eq!(panes.active_in(PaneSlot::Secondary), Some(&file));
        assert_eq!(panes.focused_pane(), PaneSlot::Secondary);
        assert!(panes.has_secondary());
        assert_unique(&panes);
        assert!(!panes.split_with(&file));
        assert!(!panes.split_with(&editor(9, 9)));
        expect(&panes, PaneSlot::Secondary, &[file.clone()]);
    }

    #[test]
    fn move_to_secondary_then_back_collapses_empty_split() {
        let a = editor(1, 1);
        let b = editor(1, 2);
        let c = host("pty-1");
        let mut panes = WorkbenchPaneLayout::new();
        panes.reconcile(&[a.clone(), b.clone(), c.clone()]);
        assert!(panes.move_to(&b, PaneSlot::Secondary));
        assert!(panes.move_to(&c, PaneSlot::Secondary));
        expect(&panes, PaneSlot::Primary, &[a.clone()]);
        expect(&panes, PaneSlot::Secondary, &[b.clone(), c.clone()]);
        assert!(!panes.move_to(&b, PaneSlot::Secondary));
        assert!(!panes.move_to(&a, PaneSlot::Primary));
        assert!(!panes.move_to(&editor(7, 7), PaneSlot::Secondary));
        assert!(panes.move_to(&b, PaneSlot::Primary));
        assert!(panes.move_to(&c, PaneSlot::Primary));
        assert!(!panes.has_secondary());
        expect(
            &panes,
            PaneSlot::Primary,
            &[a.clone(), b.clone(), c.clone()],
        );
        expect(&panes, PaneSlot::Secondary, &[]);
        assert_eq!(panes.active_in(PaneSlot::Primary), Some(&c));
        assert_eq!(panes.active_in(PaneSlot::Secondary), None);
        assert_eq!(panes.focused_pane(), PaneSlot::Primary);
        assert_unique(&panes);
    }

    #[test]
    fn reconcile_drops_stale_keeps_groups_and_appends_new_to_primary() {
        let term = host("pty-1");
        let file = editor(1, 1);
        let agent = host("pty-agent");
        let mut panes = WorkbenchPaneLayout::new();
        panes.reconcile(&[term.clone(), file.clone(), agent.clone()]);
        assert!(panes.split_with(&agent));
        let fresh = host("pty-2");
        panes.reconcile(&[term.clone(), fresh.clone(), agent.clone()]);
        expect(&panes, PaneSlot::Primary, &[term.clone(), fresh.clone()]);
        expect(&panes, PaneSlot::Secondary, &[agent.clone()]);
        assert_eq!(panes.active_in(PaneSlot::Secondary), Some(&agent));
        assert_eq!(panes.focused_pane(), PaneSlot::Secondary);
        panes.reconcile(&[term.clone(), fresh.clone()]);
        assert!(!panes.has_secondary());
        assert_eq!(panes.focused_pane(), PaneSlot::Primary);
        expect(&panes, PaneSlot::Primary, &[term.clone(), fresh.clone()]);
        assert_unique(&panes);
    }

    #[test]
    fn reconcile_repairs_selection_when_active_identity_goes_stale() {
        let a = editor(1, 1);
        let b = editor(1, 2);
        let mut panes = WorkbenchPaneLayout::new();
        panes.reconcile(&[a.clone(), b.clone()]);
        assert!(panes.activate(&b));
        assert_eq!(panes.active_in(PaneSlot::Primary), Some(&b));
        panes.reconcile(&[a.clone()]);
        assert_eq!(panes.active_in(PaneSlot::Primary), Some(&a));
        expect(&panes, PaneSlot::Primary, &[a.clone()]);
        panes.reconcile(&[]);
        assert_eq!(panes.active_in(PaneSlot::Primary), None);
        assert!(panes.tabs_in(PaneSlot::Primary).is_empty());
    }

    #[test]
    fn reconcile_deduplicates_repeated_identities() {
        let a = editor(1, 1);
        let b = host("pty-1");
        let mut panes = WorkbenchPaneLayout::new();
        panes.reconcile(&[a.clone(), a.clone(), b.clone(), a.clone()]);
        expect(&panes, PaneSlot::Primary, &[a.clone(), b.clone()]);
        assert_unique(&panes);
    }

    #[test]
    fn reconcile_with_empty_observation_empties_layout() {
        let a = editor(1, 1);
        let mut panes = WorkbenchPaneLayout::new();
        panes.reconcile(&[a.clone()]);
        assert!(panes.split_with(&a));
        assert!(panes.tabs_in(PaneSlot::Primary).is_empty());
        panes.reconcile(&[]);
        assert!(!panes.has_secondary());
        assert!(panes.tabs_in(PaneSlot::Primary).is_empty());
        assert!(panes.tabs_in(PaneSlot::Secondary).is_empty());
        assert_eq!(panes.active_in(PaneSlot::Primary), None);
        assert_eq!(panes.active_in(PaneSlot::Secondary), None);
        assert_eq!(panes.focused_pane(), PaneSlot::Primary);
    }

    #[test]
    fn activate_focuses_owning_pane_and_rejects_unknown() {
        let a = editor(1, 1);
        let b = host("pty-1");
        let mut panes = WorkbenchPaneLayout::new();
        panes.reconcile(&[a.clone(), b.clone()]);
        assert!(!panes.focus_pane(PaneSlot::Secondary));
        assert!(panes.activate(&b));
        assert_eq!(panes.active_in(PaneSlot::Primary), Some(&b));
        assert_eq!(panes.focused_pane(), PaneSlot::Primary);
        assert!(!panes.activate(&b));
        assert!(panes.split_with(&b));
        assert_eq!(panes.active_in(PaneSlot::Secondary), Some(&b));
        assert_eq!(panes.focused_pane(), PaneSlot::Secondary);
        assert!(panes.activate(&a));
        assert_eq!(panes.focused_pane(), PaneSlot::Primary);
        assert_eq!(panes.active_in(PaneSlot::Secondary), Some(&b));
        assert!(panes.focus_pane(PaneSlot::Secondary));
        assert_eq!(panes.focused_pane(), PaneSlot::Secondary);
        assert_eq!(panes.active_in(PaneSlot::Secondary), Some(&b));
        assert!(!panes.focus_pane(PaneSlot::Secondary));
        assert!(!panes.activate(&editor(3, 3)));
        assert_unique(&panes);
    }

    #[test]
    fn close_secondary_returns_all_tabs_to_primary_without_closing_any() {
        let a = editor(1, 1);
        let b = editor(1, 2);
        let c = host("pty-1");
        let d = host("pty-2");
        let mut panes = WorkbenchPaneLayout::new();
        panes.reconcile(&[a.clone(), b.clone(), c.clone(), d.clone()]);
        assert!(panes.move_to(&c, PaneSlot::Secondary));
        assert!(panes.move_to(&d, PaneSlot::Secondary));
        panes.close_secondary();
        assert!(!panes.has_secondary());
        expect(
            &panes,
            PaneSlot::Primary,
            &[a.clone(), b.clone(), c.clone(), d.clone()],
        );
        assert_eq!(panes.active_in(PaneSlot::Primary), Some(&a));
        assert_eq!(panes.active_in(PaneSlot::Secondary), None);
        assert_eq!(panes.focused_pane(), PaneSlot::Primary);
        assert!(!panes.focus_pane(PaneSlot::Secondary));
        assert_unique(&panes);
        panes.close_secondary();
        expect(
            &panes,
            PaneSlot::Primary,
            &[a.clone(), b.clone(), c.clone(), d.clone()],
        );
    }

    #[test]
    fn close_secondary_with_empty_primary_promotes_secondary_active() {
        let a = editor(1, 1);
        let b = editor(1, 2);
        let mut panes = WorkbenchPaneLayout::new();
        panes.reconcile(&[a.clone(), b.clone()]);
        assert!(panes.move_to(&a, PaneSlot::Secondary));
        assert!(panes.move_to(&b, PaneSlot::Secondary));
        assert!(panes.tabs_in(PaneSlot::Primary).is_empty());
        panes.close_secondary();
        expect(&panes, PaneSlot::Primary, &[a.clone(), b.clone()]);
        assert_eq!(panes.active_in(PaneSlot::Primary), Some(&b));
        assert_eq!(panes.focused_pane(), PaneSlot::Primary);
        assert_unique(&panes);
    }

    #[test]
    fn reset_returns_to_single_empty_primary_pane() {
        let a = editor(1, 1);
        let b = host("pty-1");
        let mut panes = WorkbenchPaneLayout::default();
        panes.reconcile(&[a.clone(), b.clone()]);
        assert!(panes.split_with(&b));
        panes.toggle_axis();
        panes.reset();
        assert!(!panes.has_secondary());
        assert!(panes.tabs_in(PaneSlot::Primary).is_empty());
        assert!(panes.tabs_in(PaneSlot::Secondary).is_empty());
        assert_eq!(panes.active_in(PaneSlot::Primary), None);
        assert_eq!(panes.active_in(PaneSlot::Secondary), None);
        assert_eq!(panes.focused_pane(), PaneSlot::Primary);
        assert_eq!(panes.axis(), PaneAxis::Horizontal);
        panes.reconcile(&[a.clone()]);
        expect(&panes, PaneSlot::Primary, &[a.clone()]);
    }

    #[test]
    fn toggle_axis_flips_and_reports_arrangement() {
        let a = editor(1, 1);
        let b = editor(1, 2);
        let mut panes = WorkbenchPaneLayout::new();
        assert_eq!(panes.axis(), PaneAxis::Horizontal);
        assert_eq!(panes.toggle_axis(), PaneAxis::Vertical);
        assert_eq!(panes.axis(), PaneAxis::Vertical);
        assert_eq!(panes.toggle_axis(), PaneAxis::Horizontal);
        panes.reconcile(&[a.clone(), b.clone()]);
        assert!(panes.split_with(&b));
        assert_eq!(panes.toggle_axis(), PaneAxis::Vertical);
        assert_eq!(panes.axis(), PaneAxis::Vertical);
        assert!(panes.has_secondary());
        expect(&panes, PaneSlot::Secondary, &[b.clone()]);
    }

    #[test]
    fn background_host_poll_tracks_other_pane_and_never_targets_editors() {
        let primary_host = host("pty-primary");
        let secondary_host = host("pty-secondary");
        let file = editor(3, 1);
        let mut panes = WorkbenchPaneLayout::new();
        panes.reconcile(&[primary_host.clone(), secondary_host.clone(), file.clone()]);
        assert_eq!(panes.background_host_target(), None);

        assert!(panes.split_with(&secondary_host));
        assert_eq!(
            panes.background_host_target(),
            match &primary_host {
                WorkbenchTabIdentity::Host(target) => Some(target),
                _ => None,
            }
        );
        assert!(panes.focus_pane(PaneSlot::Primary));
        assert_eq!(
            panes.background_host_target(),
            match &secondary_host {
                WorkbenchTabIdentity::Host(target) => Some(target),
                _ => None,
            }
        );
        assert!(panes.move_to(&secondary_host, PaneSlot::Primary));
        assert!(panes.split_with(&file));
        assert!(panes.focus_pane(PaneSlot::Primary));
        assert_eq!(panes.background_host_target(), None);
        panes.close_secondary();
        assert_eq!(panes.background_host_target(), None);
    }

    #[test]
    fn raw_keyboard_focus_allows_only_the_active_host_in_its_own_pane() {
        let a = host("pty-a");
        let b = host("pty-b");
        let file = editor(8, 1);
        let mut panes = WorkbenchPaneLayout::new();
        panes.reconcile(&[a.clone(), b.clone(), file.clone()]);
        fn host_target(id: &WorkbenchTabIdentity) -> &TerminalTabContextTarget {
            match id {
                WorkbenchTabIdentity::Host(target) => target,
                _ => unreachable!(),
            }
        }
        assert_eq!(
            panes.focused_host_target_for_input(PaneSlot::Primary),
            Some(host_target(&a))
        );
        assert!(panes.split_with(&b));
        assert_eq!(
            panes.focused_host_target_for_input(PaneSlot::Secondary),
            Some(host_target(&b))
        );
        assert_eq!(panes.focused_host_target_for_input(PaneSlot::Primary), None);
        assert!(panes.focus_pane(PaneSlot::Primary));
        assert_eq!(
            panes.focused_host_target_for_input(PaneSlot::Primary),
            Some(host_target(&a))
        );
        assert_eq!(
            panes.focused_host_target_for_input(PaneSlot::Secondary),
            None
        );
        assert!(panes.activate(&file));
        assert_eq!(panes.focused_host_target_for_input(PaneSlot::Primary), None);
        assert!(panes.focus_pane(PaneSlot::Secondary));
        assert_eq!(
            panes.focused_host_target_for_input(PaneSlot::Secondary),
            Some(host_target(&b))
        );
        panes.close_secondary();
        assert_eq!(
            panes.focused_host_target_for_input(PaneSlot::Secondary),
            None
        );
    }

    #[test]
    fn delayed_ime_commit_must_match_focused_pane_and_active_host_identity() {
        let primary = host("pty-primary");
        let secondary = host("pty-secondary");
        let editor = editor(99, 1);
        let mut panes = WorkbenchPaneLayout::new();
        panes.reconcile(&[primary.clone(), secondary.clone(), editor.clone()]);
        fn target(id: &WorkbenchTabIdentity) -> &TerminalTabContextTarget {
            match id {
                WorkbenchTabIdentity::Host(target) => target,
                _ => unreachable!(),
            }
        }
        assert!(panes.authorizes_terminal_input(PaneSlot::Primary, target(&primary)));
        assert!(panes.split_with(&secondary));
        assert!(!panes.authorizes_terminal_input(PaneSlot::Primary, target(&primary)));
        assert!(panes.authorizes_terminal_input(PaneSlot::Secondary, target(&secondary)));
        assert!(!panes.authorizes_terminal_input(PaneSlot::Secondary, target(&primary)));
        assert!(panes.activate(&editor));
        assert!(!panes.authorizes_terminal_input(PaneSlot::Primary, target(&primary)));
        assert!(panes.focus_pane(PaneSlot::Secondary));
        assert!(panes.authorizes_terminal_input(PaneSlot::Secondary, target(&secondary)));
        panes.close_secondary();
        assert!(!panes.authorizes_terminal_input(PaneSlot::Secondary, target(&secondary)));
    }

    #[test]
    fn identities_are_never_duplicated_across_panes() {
        let ids = vec![
            host("pty-1"),
            host("pty-agent"),
            editor(1, 1),
            editor(2, 1),
            editor(1, 2),
        ];
        let mut panes = WorkbenchPaneLayout::new();
        panes.reconcile(&ids);
        assert_unique(&panes);
        for id in &ids[..3] {
            assert!(panes.split_with(id));
            assert_unique(&panes);
        }
        for id in &ids {
            if panes.tabs_in(PaneSlot::Secondary).contains(id) {
                assert!(panes.move_to(id, PaneSlot::Primary));
            }
            assert!(panes.move_to(id, PaneSlot::Secondary));
            assert_unique(&panes);
            assert!(panes.move_to(id, PaneSlot::Primary));
            assert_unique(&panes);
        }
        panes.reconcile(&ids);
        panes.reconcile(&ids);
        expect(&panes, PaneSlot::Primary, &ids);
        assert!(!panes.has_secondary());
        assert_unique(&panes);
    }
}
