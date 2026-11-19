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

use crate::terminal::{TerminalSession, TerminalTabContextTarget};
use crate::workbench_layout_store::SavedWorkbenchHostTab;
use crate::workbench_tabs::{WorkbenchTab, WorkbenchTabIdentity};

/// In a split, both tab headers reserve the same pinned row so that the
/// actual tab-strip baseline remains aligned even if just one pane has pins.
/// For a single pane, only the primary pane can add this extra row.
pub fn shared_pinned_tab_row(
    primary_two_rows: bool,
    secondary_two_rows: bool,
    has_secondary: bool,
) -> bool {
    primary_two_rows || (has_secondary && secondary_two_rows)
}

/// Only side-by-side panes share a pinned Tab baseline. Stacked panes have
/// independent headers: a pin in the bottom pane must not introduce an empty
/// row above the top Terminal.
pub fn primary_pinned_row_visible(
    primary_two_rows: bool,
    secondary_two_rows: bool,
    has_secondary: bool,
    axis: PaneAxis,
) -> bool {
    shared_pinned_tab_row(
        primary_two_rows,
        secondary_two_rows,
        has_secondary && axis == PaneAxis::Horizontal,
    )
}

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

/// Capture the destination pane for a menu or pending Host RPC. A secondary
/// guard is anchored to an existing tab so a dismissed/recreated split
/// cannot accidentally inherit an old asynchronous attachment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneAttachGuard {
    generation: u64,
    pane: PaneSlot,
    anchor: Option<WorkbenchTabIdentity>,
}

impl PaneAttachGuard {
    pub fn capture(generation: u64, pane: PaneSlot, layout: &WorkbenchPaneLayout) -> Option<Self> {
        if pane == PaneSlot::Secondary && !layout.has_secondary() {
            return None;
        }
        Some(Self {
            generation,
            pane,
            anchor: layout.active_in(pane).cloned(),
        })
    }

    pub fn pane(&self) -> PaneSlot {
        self.pane
    }

    pub fn still_valid(&self, generation: u64, layout: &WorkbenchPaneLayout) -> bool {
        if generation != self.generation {
            return false;
        }
        if self.pane == PaneSlot::Primary {
            return true;
        }
        self.anchor.as_ref().is_some_and(|anchor| {
            layout.has_secondary() && layout.tabs_in(PaneSlot::Secondary).contains(anchor)
        })
    }
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

    /// Reposition *only* already-open Host previews whose complete opaque
    /// triple also matches an authenticated, still-running Host session.
    /// This never opens/attaches/creates a Host PTY. Returned count is moves.
    pub fn restore_live_host_positions(
        &mut self,
        primary: &[SavedWorkbenchHostTab],
        secondary: &[SavedWorkbenchHostTab],
        live: &[TerminalSession],
    ) -> usize {
        let mut moved = 0;
        for (saved, destination) in [
            (primary, PaneSlot::Primary),
            (secondary, PaneSlot::Secondary),
        ] {
            for hint in saved {
                let verified = live.iter().any(|session| {
                    session.running
                        && session.handle == hint.handle
                        && session.workspace_id == hint.workspace_id
                        && session.tab_id == hint.tab_id
                });
                if !verified {
                    continue;
                }
                let identity = self
                    .primary
                    .iter()
                    .chain(self.secondary.iter())
                    .find(|id| {
                        matches!(id, WorkbenchTabIdentity::Host(target)
                        if target.handle == hint.handle
                        && target.workspace_id() == hint.workspace_id
                        && target.tab_id() == hint.tab_id)
                    })
                    .cloned();
                if let Some(identity) = identity {
                    if self.move_to(&identity, destination) {
                        moved += 1;
                    }
                }
            }
        }
        moved
    }

    /// A cross-pane drop should activate only the identity that actually
    /// became active in the destination. Same-pane sorting must preserve
    /// the user's current selection; stale or closed sources get no focus.
    pub fn post_cross_pane_drop_focus(
        &self,
        moved: &WorkbenchTabIdentity,
        crossed_panes: bool,
    ) -> Option<WorkbenchTabIdentity> {
        crossed_panes
            .then(|| self.active_in(self.focused))
            .flatten()
            .filter(|active| *active == moved)
            .cloned()
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

    /// Append an existing tab to the end of its own pane when dropped onto
    /// the blank tail of the Tab strip. Never transfers an Editor or Host owner.
    pub fn move_to_end(&mut self, id: &WorkbenchTabIdentity, slot: PaneSlot) -> bool {
        let tabs = match slot {
            PaneSlot::Primary => &mut self.primary,
            PaneSlot::Secondary => &mut self.secondary,
        };
        let Some(index) = tabs.iter().position(|tab| tab == id) else {
            return false;
        };
        if index + 1 == tabs.len() {
            return false;
        }
        let moved = tabs.remove(index);
        tabs.push(moved);
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

    /// Visual tab metadata follows this pane's independent order; no
    /// identity is displayed twice and stale entries are never invented.
    pub fn visible_tabs_in(&self, slot: PaneSlot, entries: &[WorkbenchTab]) -> Vec<WorkbenchTab> {
        self.tabs_in(slot)
            .iter()
            .filter_map(|id| entries.iter().find(|tab| &tab.identity == id).cloned())
            .collect()
    }

    /// Rehydrate the saved Editor subsequence without moving Host previews
    /// or changing focus. Scratch recovery precedes file tab restoration,
    /// so its discovery order is not necessarily the user's strip order.
    pub fn restore_editor_order(
        &mut self,
        slot: PaneSlot,
        desired: &[WorkbenchTabIdentity],
    ) -> bool {
        let tabs = match slot {
            PaneSlot::Primary => &mut self.primary,
            PaneSlot::Secondary => &mut self.secondary,
        };
        let before = tabs.clone();
        let available = tabs
            .iter()
            .filter(|id| matches!(id, WorkbenchTabIdentity::Editor { .. }))
            .cloned()
            .collect::<Vec<_>>();
        let mut ordered = Vec::with_capacity(available.len());
        for id in desired.iter().chain(available.iter()) {
            if matches!(id, WorkbenchTabIdentity::Editor { .. })
                && available.contains(id)
                && !ordered.contains(id)
            {
                ordered.push(id.clone());
            }
        }
        let mut next = ordered.into_iter();
        for tab in tabs.iter_mut() {
            if matches!(tab, WorkbenchTabIdentity::Editor { .. }) {
                *tab = next.next().expect("editor count unchanged");
            }
        }
        *tabs != before
    }
    /// A same-pane drag reorders labels without transferring a Host PTY or
    /// Editor buffer. Cross-pane moves must use the guarded owner transfer.
    pub fn reorder_within(
        &mut self,
        source: &WorkbenchTabIdentity,
        target: &WorkbenchTabIdentity,
    ) -> bool {
        let tabs = if self.primary.contains(source) && self.primary.contains(target) {
            &mut self.primary
        } else if self.secondary.contains(source) && self.secondary.contains(target) {
            &mut self.secondary
        } else {
            return false;
        };
        let from = tabs.iter().position(|id| id == source).unwrap();
        let to = tabs.iter().position(|id| id == target).unwrap();
        if from == to {
            return false;
        }
        let id = tabs.remove(from);
        tabs.insert(to, id);
        true
    }

    /// Classic's tab context commands are local to the clicked pane.
    /// Choosing Close Others or Close to Right must not close any tab in
    /// the adjacent pane, including a dirty Editor or a running Host preview.
    pub fn close_candidates(
        &self,
        clicked: &WorkbenchTabIdentity,
        to_right: bool,
    ) -> Vec<WorkbenchTabIdentity> {
        let entries = if self.primary.contains(clicked) {
            &self.primary
        } else if self.secondary.contains(clicked) {
            &self.secondary
        } else {
            return Vec::new();
        };
        let index = entries.iter().position(|id| id == clicked).unwrap();
        entries
            .iter()
            .enumerate()
            .filter_map(|(position, id)| {
                (position != index && (!to_right || position > index)).then_some(id.clone())
            })
            .collect()
    }

    /// Classic closes only unpinned tabs in the clicked pane for batch-close.
    pub fn close_unpinned_candidates(
        &self,
        clicked: &WorkbenchTabIdentity,
        to_right: bool,
        pinned: &[WorkbenchTabIdentity],
    ) -> Vec<WorkbenchTabIdentity> {
        self.close_candidates(clicked, to_right)
            .into_iter()
            .filter(|id| !pinned.contains(id))
            .collect()
    }

    pub fn adjacent_tab(
        &self,
        clicked: &WorkbenchTabIdentity,
        move_right: bool,
    ) -> Option<WorkbenchTabIdentity> {
        let entries = if self.primary.contains(clicked) {
            &self.primary
        } else if self.secondary.contains(clicked) {
            &self.secondary
        } else {
            return None;
        };
        let index = entries.iter().position(|id| id == clicked)?;
        let neighbor = if move_right {
            (index + 1 < entries.len()).then_some(index + 1)
        } else {
            index.checked_sub(1)
        }?;
        entries.get(neighbor).cloned()
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

    /// Native Windows TextInput can deliver a delayed IME completion after
    /// focus moved to Explorer, Settings or a different Terminal. A selected
    /// Host tab alone is not permission to forward that completion to a PTY.
    pub fn authorizes_native_terminal_commit(
        &self,
        slot: PaneSlot,
        active_target: &TerminalTabContextTarget,
        input_has_os_focus: bool,
    ) -> bool {
        input_has_os_focus && self.authorizes_terminal_input(slot, active_target)
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
            agent_state: None,
            interrupted: false,
            agent_state_started_at: None,
            agent_updated_at: None,
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
    fn blank_tail_drop_appends_graph_without_cloning_host_or_editor() {
        let terminal = host("tail-host");
        let file = editor(9, 2);
        let graph = WorkbenchTabIdentity::GitHistory;
        let mut panes = WorkbenchPaneLayout::new();
        panes.reconcile(&[graph.clone(), terminal.clone(), file.clone()]);
        assert!(panes.move_to_end(&graph, PaneSlot::Primary));
        expect(
            &panes,
            PaneSlot::Primary,
            &[terminal.clone(), file.clone(), graph.clone()],
        );
        assert!(!panes.move_to_end(&graph, PaneSlot::Primary));
        assert!(!panes.move_to_end(&editor(9, 999), PaneSlot::Primary));
        assert!(panes.move_to(&graph, PaneSlot::Secondary));
        assert!(panes.move_to(&file, PaneSlot::Secondary));
        assert!(panes.move_to_end(&graph, PaneSlot::Secondary));
        expect(&panes, PaneSlot::Primary, &[terminal.clone()]);
        expect(&panes, PaneSlot::Secondary, &[file.clone(), graph.clone()]);
        assert!(!panes.move_to_end(&graph, PaneSlot::Primary));
        assert_unique(&panes);
        assert_eq!(panes.active_in(PaneSlot::Secondary), Some(&file));
    }

    #[test]
    fn git_history_moves_between_mixed_panes_without_cloning_editor_or_pty() {
        let terminal = host("terminal-1");
        let file = editor(7, 2);
        let graph = WorkbenchTabIdentity::GitHistory;
        let mut panes = WorkbenchPaneLayout::new();
        panes.reconcile(&[terminal.clone(), file.clone(), graph.clone()]);
        assert!(panes.split_with(&graph));
        assert_unique(&panes);
        expect(&panes, PaneSlot::Primary, &[terminal.clone(), file.clone()]);
        expect(&panes, PaneSlot::Secondary, &[graph.clone()]);
        assert_eq!(panes.active_in(PaneSlot::Secondary), Some(&graph));
        assert!(panes.move_to(&graph, PaneSlot::Primary));
        assert_unique(&panes);
        assert!(!panes.has_secondary());
        expect(&panes, PaneSlot::Primary, &[terminal, file, graph]);
    }

    #[test]
    fn restored_host_pane_positions_require_running_exact_three_part_identity() {
        let left = host("left");
        let right = host("right");
        let mut panes = WorkbenchPaneLayout::new();
        panes.reconcile(&[left.clone(), right.clone()]);
        let saved_right = SavedWorkbenchHostTab::new("right", "ws", "tab-right");
        let saved_left = SavedWorkbenchHostTab::new("left", "ws", "tab-left");
        let live = vec![
            TerminalSession {
                handle: "left".into(),
                workspace_id: "ws".into(),
                tab_id: "tab-left".into(),
                running: true,
                agent_type: None,
                agent_state: None,
                interrupted: false,
                agent_state_started_at: None,
                agent_updated_at: None,
            },
            TerminalSession {
                handle: "right".into(),
                workspace_id: "ws".into(),
                tab_id: "tab-right".into(),
                running: true,
                agent_type: None,
                agent_state: None,
                interrupted: false,
                agent_state_started_at: None,
                agent_updated_at: None,
            },
        ];
        assert_eq!(
            panes.restore_live_host_positions(&[saved_left], &[saved_right.clone()], &live),
            1
        );
        assert_eq!(panes.tabs_in(PaneSlot::Primary), &[left.clone()]);
        assert_eq!(panes.tabs_in(PaneSlot::Secondary), &[right.clone()]);
        assert_unique(&panes);

        let mut recycled = WorkbenchPaneLayout::new();
        recycled.reconcile(&[left, right.clone()]);
        let mut replaced = live.clone();
        replaced[1].tab_id = "recycled-tab".into();
        assert_eq!(
            recycled.restore_live_host_positions(&[], &[saved_right.clone()], &replaced),
            0
        );
        assert!(!recycled.has_secondary());
        replaced[1].tab_id = "tab-right".into();
        replaced[1].running = false;
        assert_eq!(
            recycled.restore_live_host_positions(&[], &[saved_right], &replaced),
            0
        );
        assert!(!recycled.has_secondary());
    }

    #[test]
    fn split_tab_headers_share_one_row_even_when_only_one_side_is_pinned() {
        assert!(!shared_pinned_tab_row(false, false, true));
        assert!(shared_pinned_tab_row(true, false, true));
        assert!(shared_pinned_tab_row(false, true, true));
        assert!(shared_pinned_tab_row(true, true, true));
        assert!(!shared_pinned_tab_row(false, true, false));
    }

    #[test]
    fn only_side_by_side_panes_share_pinned_tab_header_height() {
        assert!(primary_pinned_row_visible(
            false,
            true,
            true,
            PaneAxis::Horizontal
        ));
        assert!(primary_pinned_row_visible(
            true,
            false,
            true,
            PaneAxis::Horizontal
        ));
        assert!(!primary_pinned_row_visible(
            false,
            true,
            true,
            PaneAxis::Vertical
        ));
        assert!(primary_pinned_row_visible(
            true,
            false,
            true,
            PaneAxis::Vertical
        ));
        assert!(!primary_pinned_row_visible(
            false,
            true,
            false,
            PaneAxis::Horizontal
        ));
    }

    #[test]
    fn cross_pane_host_drop_requests_focus_only_for_moved_live_identity() {
        let agent = host("claude-session");
        let terminal = host("cmd-session");
        let mut panes = WorkbenchPaneLayout::new();
        panes.reconcile(&[agent.clone(), terminal.clone()]);
        assert_eq!(panes.post_cross_pane_drop_focus(&agent, false), None);
        assert!(panes.move_to(&terminal, PaneSlot::Secondary));
        assert_eq!(
            panes.post_cross_pane_drop_focus(&terminal, true),
            Some(terminal.clone())
        );
        assert_eq!(
            panes.post_cross_pane_drop_focus(&agent, true),
            None,
            "the old primary Host must not steal the moved tab focus"
        );
        assert!(panes.move_to(&agent, PaneSlot::Secondary));
        assert_eq!(
            panes.post_cross_pane_drop_focus(&agent, true),
            Some(agent.clone())
        );
        panes.reconcile(&[terminal.clone()]);
        assert_eq!(
            panes.post_cross_pane_drop_focus(&agent, true),
            None,
            "a closed Host preview cannot receive focus"
        );
        assert_eq!(panes.post_cross_pane_drop_focus(&terminal, false), None);
    }

    #[test]
    fn per_pane_projection_and_reorder_keep_active_editor_and_unique_tabs() {
        use crate::workbench_tabs::{WorkbenchTab, WorkbenchTabKind};
        let a = editor(5, 1);
        let b = editor(5, 2);
        let c = editor(5, 3);
        let d = editor(5, 4);
        let mut panes = WorkbenchPaneLayout::new();
        panes.reconcile(&[a.clone(), b.clone(), c.clone(), d.clone()]);
        assert!(panes.move_to(&b, PaneSlot::Secondary));
        assert!(panes.move_to(&d, PaneSlot::Secondary));
        // Moving into a pane already activates the tab; selecting it again
        // correctly reports no change.
        assert_eq!(panes.active_in(PaneSlot::Secondary), Some(&d));
        assert!(!panes.activate(&d));
        let entries = [c.clone(), d.clone(), a.clone(), b.clone()]
            .into_iter()
            .map(|identity| WorkbenchTab {
                title: format!("{identity:?}"),
                identity,
                kind: WorkbenchTabKind::Editor,
                dirty: true,
                running: true,
                agent_state: None,
                interrupted: false,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            panes
                .visible_tabs_in(PaneSlot::Primary, &entries)
                .iter()
                .map(|tab| &tab.identity)
                .collect::<Vec<_>>(),
            vec![&a, &c]
        );
        assert_eq!(
            panes
                .visible_tabs_in(PaneSlot::Secondary, &entries)
                .iter()
                .map(|tab| &tab.identity)
                .collect::<Vec<_>>(),
            vec![&b, &d]
        );
        assert!(panes.reorder_within(&d, &b));
        expect(&panes, PaneSlot::Secondary, &[d.clone(), b.clone()]);
        assert_eq!(panes.active_in(PaneSlot::Secondary), Some(&d));
        assert!(!panes.reorder_within(&a, &d));
        assert_unique(&panes);
        let remaining = vec![entries[0].clone(), entries[2].clone()];
        assert!(panes
            .visible_tabs_in(PaneSlot::Secondary, &remaining)
            .is_empty());
        assert_eq!(
            panes.visible_tabs_in(PaneSlot::Primary, &remaining).len(),
            2
        );
    }

    #[test]
    fn restored_editor_order_handles_scratch_files_and_preserves_host_slots() {
        let scratch_b = editor(7, 11);
        let scratch_a = editor(7, 12);
        let file_b = editor(7, 13);
        let file_a = editor(7, 14);
        let new_file = editor(7, 15);
        let terminal = host("retained-terminal");
        let mut panes = WorkbenchPaneLayout::new();
        // Startup recovery observes scratch drafts before file-backed tabs.
        panes.reconcile(&[
            scratch_b.clone(),
            terminal.clone(),
            scratch_a.clone(),
            file_b.clone(),
            file_a.clone(),
            new_file.clone(),
        ]);
        panes.move_to(&file_b, PaneSlot::Secondary);
        panes.move_to(&file_a, PaneSlot::Secondary);
        panes.activate(&scratch_b);
        let old_active = panes.active_in(PaneSlot::Primary).cloned();
        let stale = editor(3, 999);
        assert!(panes.restore_editor_order(
            PaneSlot::Primary,
            &[
                scratch_a.clone(),
                stale,
                scratch_b.clone(),
                scratch_a.clone()
            ],
        ));
        assert!(panes.restore_editor_order(
            PaneSlot::Secondary,
            &[file_a.clone(), file_b.clone(), scratch_a.clone()],
        ));
        expect(
            &panes,
            PaneSlot::Primary,
            &[scratch_a.clone(), terminal, scratch_b.clone(), new_file],
        );
        expect(&panes, PaneSlot::Secondary, &[file_a, file_b]);
        assert_eq!(panes.active_in(PaneSlot::Primary), old_active.as_ref());
        assert!(!panes.restore_editor_order(PaneSlot::Primary, &[scratch_a, scratch_b]));
        assert_unique(&panes);
    }
    #[test]
    fn close_others_and_right_skip_pins_and_other_pane() {
        let first = editor(6, 1);
        let pinned = editor(6, 2);
        let third = editor(6, 3);
        let secondary = editor(6, 4);
        let mut layout = WorkbenchPaneLayout::new();
        layout.reconcile(&[
            first.clone(),
            pinned.clone(),
            third.clone(),
            secondary.clone(),
        ]);
        assert!(layout.move_to(&secondary, PaneSlot::Secondary));
        assert_eq!(
            layout.close_unpinned_candidates(&first, true, std::slice::from_ref(&pinned)),
            vec![third.clone()]
        );
        assert_eq!(
            layout.close_unpinned_candidates(&third, false, std::slice::from_ref(&pinned)),
            vec![first.clone()]
        );
        assert!(layout
            .close_unpinned_candidates(&secondary, false, &[])
            .is_empty());
    }

    #[test]
    fn per_pane_pinned_close_targets_survive_reorder_and_duplicate_instance_numbers() {
        // Two editor mounts can use the same instance number in different
        // epochs. Pin and batch-close decisions must compare full identities
        // and must never touch the opposite pane.
        let primary_active = editor(7, 1);
        let primary_pinned = editor(7, 2);
        let secondary_pinned = editor(8, 1);
        let secondary_unpinned = editor(8, 3);
        let secondary_active = editor(8, 4);
        let mut panes = WorkbenchPaneLayout::new();
        panes.reconcile(&[
            primary_active.clone(),
            primary_pinned.clone(),
            secondary_pinned.clone(),
            secondary_unpinned.clone(),
            secondary_active.clone(),
        ]);
        for tab in [&secondary_pinned, &secondary_unpinned, &secondary_active] {
            assert!(panes.move_to(tab, PaneSlot::Secondary));
        }
        let pinned = vec![primary_pinned.clone(), secondary_pinned.clone()];
        assert!(panes
            .close_unpinned_candidates(&primary_active, false, &pinned)
            .is_empty());
        assert_eq!(
            panes.close_unpinned_candidates(&secondary_unpinned, false, &pinned),
            vec![secondary_active.clone()]
        );
        assert!(panes.reorder_within(&secondary_active, &secondary_pinned));
        assert_eq!(
            panes.close_unpinned_candidates(&secondary_active, true, &pinned),
            vec![secondary_unpinned.clone()]
        );
        expect(
            &panes,
            PaneSlot::Primary,
            &[primary_active.clone(), primary_pinned.clone()],
        );
        expect(
            &panes,
            PaneSlot::Secondary,
            &[
                secondary_active.clone(),
                secondary_pinned.clone(),
                secondary_unpinned.clone(),
            ],
        );
        assert_unique(&panes);
    }

    #[test]
    fn context_actions_only_target_neighbors_in_the_clicked_pane() {
        let a = editor(3, 1);
        let b = editor(3, 2);
        let c = editor(3, 3);
        let d = host("pty-live");
        let mut panes = WorkbenchPaneLayout::new();
        panes.reconcile(&[a.clone(), b.clone(), c.clone(), d.clone()]);
        assert!(panes.move_to(&b, PaneSlot::Secondary));
        assert!(panes.move_to(&c, PaneSlot::Secondary));
        assert_eq!(panes.close_candidates(&b, true), vec![c.clone()]);
        assert_eq!(panes.close_candidates(&c, true), Vec::new());
        assert_eq!(panes.close_candidates(&d, false), vec![a.clone()]);
        assert_eq!(panes.adjacent_tab(&b, true), Some(c.clone()));
        assert_eq!(panes.adjacent_tab(&b, false), None);
        assert_eq!(panes.adjacent_tab(&a, true), Some(d.clone()));
        assert_eq!(panes.adjacent_tab(&c, true), None);
        assert!(panes.reorder_within(&c, &b));
        assert_eq!(panes.close_candidates(&c, true), vec![b.clone()]);
        assert_eq!(panes.close_candidates(&a, false), vec![d.clone()]);
        assert!(panes.close_candidates(&editor(999, 1), false).is_empty());
        assert_unique(&panes);
    }

    #[test]
    fn pane_attach_guard_rejects_workspace_switch_and_recreated_secondary() {
        let first = editor(4, 1);
        let second = editor(4, 2);
        let mut layout = WorkbenchPaneLayout::new();
        layout.reconcile(&[first.clone(), second.clone()]);
        assert!(PaneAttachGuard::capture(7, PaneSlot::Secondary, &layout).is_none());
        let primary = PaneAttachGuard::capture(7, PaneSlot::Primary, &layout).unwrap();
        assert!(layout.move_to(&second, PaneSlot::Secondary));
        let secondary = PaneAttachGuard::capture(7, PaneSlot::Secondary, &layout).unwrap();
        assert!(secondary.still_valid(7, &layout));
        assert!(!secondary.still_valid(8, &layout));
        assert!(primary.still_valid(7, &layout));
        assert!(layout.move_to(&second, PaneSlot::Primary));
        assert!(!secondary.still_valid(7, &layout));
        assert!(layout.move_to(&first, PaneSlot::Secondary));
        assert!(
            !secondary.still_valid(7, &layout),
            "stale menu cannot target recreated split"
        );
        assert!(primary.still_valid(7, &layout));
        assert!(!primary.still_valid(8, &layout));
    }

    #[test]
    fn pane_attach_guard_survives_other_selection_in_original_pane() {
        let first = editor(1, 1);
        let second = editor(1, 2);
        let mut layout = WorkbenchPaneLayout::new();
        layout.reconcile(&[first.clone(), second.clone()]);
        assert!(layout.move_to(&first, PaneSlot::Secondary));
        let target = PaneAttachGuard::capture(5, PaneSlot::Secondary, &layout).unwrap();
        assert!(layout.move_to(&second, PaneSlot::Secondary));
        assert!(layout.activate(&first));
        assert!(target.still_valid(5, &layout));
        assert_eq!(target.pane(), PaneSlot::Secondary);
        assert!(layout.activate(&second));
        assert!(target.still_valid(5, &layout));
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
    fn delayed_native_ime_commit_requires_os_focus_not_just_selected_host_tab() {
        let active = host("pty-zhu-yin");
        let other = host("pty-second-pane");
        let mut panes = WorkbenchPaneLayout::new();
        panes.reconcile(&[active.clone(), other.clone()]);
        let WorkbenchTabIdentity::Host(target) = &active else {
            unreachable!();
        };
        // Clicking Explorer leaves the last selected Host in TerminalDock,
        // but a delayed IME callback from that Host must not type into it.
        assert!(!panes.authorizes_native_terminal_commit(PaneSlot::Primary, target, false));
        assert!(panes.authorizes_native_terminal_commit(PaneSlot::Primary, target, true));
        assert!(panes.split_with(&other));
        assert!(!panes.authorizes_native_terminal_commit(PaneSlot::Primary, target, true));
        assert!(!panes.authorizes_native_terminal_commit(PaneSlot::Secondary, target, true));
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
