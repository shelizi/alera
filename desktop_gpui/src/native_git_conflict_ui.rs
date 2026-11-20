//! GPUI-only, view-only three-way conflict surface. It does not expose
//! accept/stage/resolve actions, so selection and navigation never mutate Git.
use super::*;
use alera_gpui_desktop::git_conflict_compare::{
    preview_index_conflict, ConflictVersion, GitConflictComparison,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConflictSource {
    UnresolvedIndex,
    PlannedMerge,
}

#[derive(Clone)]
pub(super) struct CenterConflictPreview {
    workspace_generation: u64,
    path: PathBuf,
    source: ConflictSource,
    result: Option<Result<Arc<GitConflictComparison>, String>>,
    change_anchors: Vec<usize>,
    selected_change: Option<usize>,
    compact_version: u8,
    collapse_unchanged: bool,
    display_rows: Arc<Vec<ConflictDisplayRow>>,
    focus: Option<gpui::FocusHandle>,
}

/// The same compact row projection is used in all three virtual lists.
/// Collapsed gaps are presentation only, never fake Git stage contents.
#[derive(Clone, Debug, PartialEq, Eq)]
enum ConflictDisplayRow {
    Line(usize),
    Fold { hidden: usize },
}

const CONFLICT_CONTEXT_LINES: usize = 3;
const MIN_COLLAPSE_GAP: usize = 9;

fn project_conflict_rows(
    snapshot: &GitConflictComparison,
    collapse: bool,
) -> Arc<Vec<ConflictDisplayRow>> {
    let total = snapshot.aligned_rows.len();
    if !collapse {
        return Arc::new((0..total).map(ConflictDisplayRow::Line).collect());
    }
    let mut visible = vec![false; total];
    // Keep leading and trailing lines as recognizable file context, even
    // when no differences exist. Each diff gets three unchanged lines either
    // side; overlapping neighborhoods merge automatically.
    for index in 0..total.min(CONFLICT_CONTEXT_LINES) {
        visible[index] = true;
    }
    for index in total.saturating_sub(CONFLICT_CONTEXT_LINES)..total {
        visible[index] = true;
    }
    for (index, row) in snapshot.aligned_rows.iter().enumerate() {
        if !row.changed {
            continue;
        }
        let start = index.saturating_sub(CONFLICT_CONTEXT_LINES);
        let end = (index + CONFLICT_CONTEXT_LINES + 1).min(total);
        visible[start..end].fill(true);
    }
    let mut projection = Vec::new();
    let mut index = 0;
    while index < total {
        if visible[index] {
            projection.push(ConflictDisplayRow::Line(index));
            index += 1;
            continue;
        }
        let start = index;
        while index < total && !visible[index] {
            index += 1;
        }
        let hidden = index - start;
        if hidden < MIN_COLLAPSE_GAP {
            projection.extend((start..index).map(ConflictDisplayRow::Line));
        } else {
            projection.push(ConflictDisplayRow::Fold { hidden });
        }
    }
    Arc::new(projection)
}

fn conflict_projected_anchor(rows: &[ConflictDisplayRow], anchor: usize) -> Option<usize> {
    rows.iter()
        .position(|row| *row == ConflictDisplayRow::Line(anchor))
}

fn active_conflict_hunk(
    snapshot: &GitConflictComparison,
    anchors: &[usize],
    selected: Option<usize>,
) -> Option<std::ops::Range<usize>> {
    let first = *anchors.get(selected?)?;
    let last = (first..snapshot.aligned_rows.len())
        .take_while(|index| snapshot.aligned_rows[*index].changed)
        .last()
        .unwrap_or(first);
    Some(first..last + 1)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ConflictViewTarget {
    Transient,
    Pinned(u64),
}

const MAX_PINNED_CONFLICTS: usize = 8;

pub(super) struct PinnedConflictTab {
    pub(super) instance: u64,
    preview: CenterConflictPreview,
    scroll: [UniformListScrollHandle; 3],
}

impl PinnedConflictTab {
    pub(super) fn title(&self) -> String {
        let name = self
            .preview
            .path
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|| self.preview.path.display().to_string());
        format!("{name} · 3-way")
    }
}

#[derive(Default)]
pub(super) struct PinnedConflictTabs {
    tabs: Vec<PinnedConflictTab>,
    next_instance: u64,
}

impl PinnedConflictTabs {
    pub(super) fn entries(&self) -> &[PinnedConflictTab] {
        &self.tabs
    }
    pub(super) fn get(&self, instance: u64) -> Option<&PinnedConflictTab> {
        self.tabs.iter().find(|item| item.instance == instance)
    }
    fn get_mut(&mut self, instance: u64) -> Option<&mut PinnedConflictTab> {
        self.tabs.iter_mut().find(|item| item.instance == instance)
    }
    pub(super) fn close(&mut self, instance: u64) -> bool {
        let len = self.tabs.len();
        self.tabs.retain(|item| item.instance != instance);
        self.tabs.len() != len
    }
    pub(super) fn reset(&mut self) {
        self.tabs.clear();
        // Keep the counter: a stale id from another workspace cannot rebind.
    }
    fn open(&mut self, preview: &CenterConflictPreview) -> Result<u64, String> {
        let Some(Ok(snapshot)) = preview.result.as_ref() else {
            return Err("Wait for the complete read-only conflict snapshot before pinning".into());
        };
        if let Some(existing) = self.tabs.iter().find(|item| {
            item.preview.source == preview.source
                && item.preview.path == preview.path
                && item.preview.result.as_ref().is_some_and(|result| {
                    result
                        .as_ref()
                        .is_ok_and(|saved| saved.as_ref() == snapshot.as_ref())
                })
        }) {
            return Ok(existing.instance);
        }
        if self.tabs.len() >= MAX_PINNED_CONFLICTS {
            return Err("Close an older three-way comparison tab first (limit 8)".into());
        }
        self.next_instance = self.next_instance.wrapping_add(1);
        if self.next_instance == 0 {
            self.next_instance = 1;
        }
        let id = self.next_instance;
        self.tabs.push(PinnedConflictTab {
            instance: id,
            preview: preview.clone(),
            scroll: [
                UniformListScrollHandle::new(),
                UniformListScrollHandle::new(),
                UniformListScrollHandle::new(),
            ],
        });
        Ok(id)
    }
}

/// Consecutive changed lines make one navigation target. Use only the
/// immutable, bounded conflict snapshot; never query or mutate Git in render.
fn conflict_change_anchors(snapshot: &GitConflictComparison) -> Vec<usize> {
    snapshot
        .aligned_rows
        .iter()
        .enumerate()
        .filter_map(|(index, row)| {
            (row.changed && (index == 0 || !snapshot.aligned_rows[index - 1].changed))
                .then_some(index)
        })
        .collect()
}

fn adjacent_conflict_change(current: Option<usize>, count: usize, forward: bool) -> Option<usize> {
    if forward {
        current.map_or_else(
            || (count > 0).then_some(0),
            |index| index.checked_add(1).filter(|next| *next < count),
        )
    } else {
        current
            .filter(|index| *index < count)
            .and_then(|index| index.checked_sub(1))
    }
}

fn compact_conflict_layout(usable_width: f32) -> bool {
    usable_width < 900.0
}

impl Workbench {
    fn focused_conflict_key_target(&self) -> Option<ConflictViewTarget> {
        // A pinned comparison in the right pane can own focus even while a
        // transient three-way preview is still visible in the left pane.
        if self.mixed_panes.focused_pane() == PaneSlot::Secondary {
            if let Some(WorkbenchTabIdentity::GitConflictCompare(instance)) =
                self.mixed_panes.active_in(PaneSlot::Secondary)
            {
                if self.pinned_conflict_tabs.get(*instance).is_some() {
                    return Some(ConflictViewTarget::Pinned(*instance));
                }
            }
        }
        if self.center_git_conflict_visible() {
            return Some(ConflictViewTarget::Transient);
        }
        let active = self
            .mixed_panes
            .active_in(self.mixed_panes.focused_pane())?;
        let WorkbenchTabIdentity::GitConflictCompare(instance) = active else {
            return None;
        };
        self.pinned_conflict_tabs
            .get(*instance)
            .map(|_| ConflictViewTarget::Pinned(*instance))
    }

    pub(super) fn conflict_next_difference_action(
        &mut self,
        _: &ConflictNextDifference,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(target) = self.focused_conflict_key_target() {
            self.jump_center_conflict_change(target, true, cx);
        }
    }

    pub(super) fn conflict_previous_difference_action(
        &mut self,
        _: &ConflictPreviousDifference,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(target) = self.focused_conflict_key_target() {
            self.jump_center_conflict_change(target, false, cx);
        }
    }

    pub(super) fn conflict_toggle_context_action(
        &mut self,
        _: &ConflictToggleContext,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(target) = self.focused_conflict_key_target() {
            self.toggle_conflict_context(target, cx);
        }
    }

    fn conflict_view(
        &self,
        target: ConflictViewTarget,
    ) -> Option<(&CenterConflictPreview, &[UniformListScrollHandle; 3])> {
        match target {
            ConflictViewTarget::Transient => Some((
                self.center_git_conflict.as_ref()?,
                &self.center_git_conflict_scroll,
            )),
            ConflictViewTarget::Pinned(instance) => {
                let tab = self.pinned_conflict_tabs.get(instance)?;
                Some((&tab.preview, &tab.scroll))
            }
        }
    }

    fn conflict_view_mut(
        &mut self,
        target: ConflictViewTarget,
    ) -> Option<(&mut CenterConflictPreview, &[UniformListScrollHandle; 3])> {
        match target {
            ConflictViewTarget::Transient => Some((
                self.center_git_conflict.as_mut()?,
                &self.center_git_conflict_scroll,
            )),
            ConflictViewTarget::Pinned(instance) => {
                let tab = self.pinned_conflict_tabs.get_mut(instance)?;
                Some((&mut tab.preview, &tab.scroll))
            }
        }
    }

    fn pin_center_git_conflict(&mut self, cx: &mut Context<Self>) {
        let Some(preview) = self.center_git_conflict.as_ref() else {
            return;
        };
        if preview.workspace_generation != self.workspace_generation {
            return;
        }
        match self.pinned_conflict_tabs.open(preview) {
            Ok(instance) => {
                self.center_git_conflict_request = self.center_git_conflict_request.wrapping_add(1);
                self.center_git_conflict = None;
                self.active_pinned_conflict = Some(instance);
                self.git_original_active = false;
                self.active_git_compare_tab = None;
                self.git_graph_center_open = false;
                self.mixed_panes.focus_pane(PaneSlot::Primary);
                self.sync_mixed_tabs(cx);
                let identity = WorkbenchTabIdentity::GitConflictCompare(instance);
                self.mixed_tabs.select(&identity);
                self.mixed_tabs.set_pinned(&identity, true);
                self.mixed_panes.activate(&identity);
                self.save_mixed_pane_layout(cx);
            }
            Err(error) => self.model.set_status(error),
        }
        cx.notify();
    }

    fn close_conflict_view(&mut self, target: ConflictViewTarget, cx: &mut Context<Self>) {
        match target {
            ConflictViewTarget::Transient => self.close_center_git_conflict(cx),
            ConflictViewTarget::Pinned(instance) => {
                self.close_mixed_tab(&WorkbenchTabIdentity::GitConflictCompare(instance), cx)
            }
        }
    }

    fn close_center_git_conflict(&mut self, cx: &mut Context<Self>) {
        self.center_git_conflict_request = self.center_git_conflict_request.wrapping_add(1);
        self.center_git_conflict = None;
        cx.notify();
    }

    pub(super) fn center_git_conflict_visible(&self) -> bool {
        self.center_git_conflict
            .as_ref()
            .is_some_and(|preview| preview.workspace_generation == self.workspace_generation)
            && !self.git_original_active
    }

    /// Load unresolved conflict stages from the Git index. No version is read
    /// from the potentially marker-filled working file, and no writer runs.
    pub(super) fn open_index_git_conflict(&mut self, relative: PathBuf, cx: &mut Context<Self>) {
        let root = self.model.root().to_path_buf();
        self.open_center_conflict_common(relative.clone(), ConflictSource::UnresolvedIndex, cx);
        let generation = self.workspace_generation;
        let request = self.center_git_conflict_request;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(
                    async move { preview_index_conflict(&root, &relative).map(Arc::new) },
                )
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation
                    || view.center_git_conflict_request != request
                {
                    return;
                }
                if let Some(selection) = view.center_git_conflict.as_mut() {
                    if selection.workspace_generation == generation {
                        selection.change_anchors = result
                            .as_ref()
                            .ok()
                            .map(|snapshot| conflict_change_anchors(snapshot))
                            .unwrap_or_default();
                        selection.display_rows = result
                            .as_ref()
                            .ok()
                            .map(|snapshot| {
                                project_conflict_rows(snapshot, selection.collapse_unchanged)
                            })
                            .unwrap_or_else(|| Arc::new(Vec::new()));
                        selection.result = Some(result);
                        cx.notify();
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    /// The pre-merge planner has already built an immutable in-memory index;
    /// never read current HEAD/working files in place of that planned result.
    pub(super) fn open_planned_git_conflict(&mut self, relative: String, cx: &mut Context<Self>) {
        let file = self
            .git_conflict_preview
            .as_ref()
            .and_then(|plan| plan.conflicts.iter().find(|file| file.path == relative))
            .cloned();
        let Some(file) = file else {
            self.model
                .set_status("Refresh merge conflict preview before comparing");
            cx.notify();
            return;
        };
        let path = PathBuf::from(&file.path);
        self.open_center_conflict_common(path.clone(), ConflictSource::PlannedMerge, cx);
        let result = GitConflictComparison::from_versions(
            path,
            file.ancestor_preview.as_deref(),
            Some(&file.ours_preview),
            Some(&file.theirs_preview),
        )
        .map(Arc::new);
        if let Some(selection) = self.center_git_conflict.as_mut() {
            selection.change_anchors = result
                .as_ref()
                .ok()
                .map(|snapshot| conflict_change_anchors(snapshot))
                .unwrap_or_default();
            selection.display_rows = result
                .as_ref()
                .ok()
                .map(|snapshot| project_conflict_rows(snapshot, selection.collapse_unchanged))
                .unwrap_or_else(|| Arc::new(Vec::new()));
            selection.result = Some(result);
        }
        cx.notify();
    }

    fn open_center_conflict_common(
        &mut self,
        path: PathBuf,
        source: ConflictSource,
        cx: &mut Context<Self>,
    ) {
        self.center_git_conflict_request = self.center_git_conflict_request.wrapping_add(1);
        self.center_git_conflict = Some(CenterConflictPreview {
            workspace_generation: self.workspace_generation,
            path,
            source,
            result: None,
            change_anchors: Vec::new(),
            selected_change: None,
            compact_version: 1,
            collapse_unchanged: true,
            display_rows: Arc::new(Vec::new()),
            focus: Some(cx.focus_handle()),
        });
        self.center_git_diff_source = None;
        self.center_git_diff_hunk = None;
        self.git_original_active = false;
        self.active_git_compare_tab = None;
        self.active_pinned_conflict = None;
        self.git_graph_center_open = false;
        for scroll in &self.center_git_conflict_scroll {
            scroll.scroll_to_item(0, gpui::ScrollStrategy::Top);
        }
        cx.notify();
    }

    /// Jump all three bounded virtual lists to the same changed region.
    /// Wheel scrolling remains independent when lines have shifted.
    fn jump_center_conflict_change(
        &mut self,
        target: ConflictViewTarget,
        forward: bool,
        cx: &mut Context<Self>,
    ) {
        let generation = self.workspace_generation;
        let Some((preview, scrolls)) = self.conflict_view_mut(target) else {
            return;
        };
        if preview.workspace_generation != generation {
            return;
        }
        let Some(next) = adjacent_conflict_change(
            preview.selected_change,
            preview.change_anchors.len(),
            forward,
        ) else {
            return;
        };
        let Some(Ok(snapshot)) = &preview.result else {
            return;
        };
        preview.selected_change = Some(next);
        let anchor = preview.change_anchors[next];
        let Some(projected) = conflict_projected_anchor(&preview.display_rows, anchor) else {
            return;
        };
        for (index, scroll) in scrolls.iter().enumerate() {
            if let Some(version) = conflict_version(snapshot, index as u8) {
                if !version.lines.is_empty() && !preview.display_rows.is_empty() {
                    scroll.scroll_to_item(projected, gpui::ScrollStrategy::Center);
                }
            }
        }
        cx.notify();
    }

    fn select_center_conflict_version(
        &mut self,
        target: ConflictViewTarget,
        role: u8,
        cx: &mut Context<Self>,
    ) {
        let Some((preview, scrolls)) = self.conflict_view_mut(target) else {
            return;
        };
        if role > 2 {
            return;
        }
        preview.compact_version = role;
        if let (Some(Ok(snapshot)), Some(change)) = (&preview.result, preview.selected_change) {
            if let Some(version) = conflict_version(snapshot, role) {
                if !version.lines.is_empty() {
                    if let Some(anchor) = preview.change_anchors.get(change).and_then(|anchor| {
                        conflict_projected_anchor(&preview.display_rows, *anchor)
                    }) {
                        scrolls[usize::from(role)]
                            .scroll_to_item(anchor, gpui::ScrollStrategy::Center);
                    }
                }
            }
        }
        cx.notify();
    }

    fn toggle_conflict_context(&mut self, target: ConflictViewTarget, cx: &mut Context<Self>) {
        let Some((preview, scrolls)) = self.conflict_view_mut(target) else {
            return;
        };
        let Some(Ok(snapshot)) = &preview.result else {
            return;
        };
        preview.collapse_unchanged = !preview.collapse_unchanged;
        preview.display_rows = project_conflict_rows(snapshot, preview.collapse_unchanged);
        let selected_row = preview
            .selected_change
            .and_then(|index| preview.change_anchors.get(index))
            .and_then(|anchor| conflict_projected_anchor(&preview.display_rows, *anchor));
        for scroll in scrolls {
            if let Some(index) = selected_row {
                scroll.scroll_to_item(index, gpui::ScrollStrategy::Center);
            } else {
                scroll.scroll_to_item(0, gpui::ScrollStrategy::Top);
            }
        }
        cx.notify();
    }

    fn conflict_version_pane(
        snapshot: Arc<GitConflictComparison>,
        visible_rows: Arc<Vec<ConflictDisplayRow>>,
        selected_hunk: Option<std::ops::Range<usize>>,
        title: String,
        role: u8,
        accent: u32,
        list_name: &'static str,
        scroll: &UniformListScrollHandle,
        locale: UiLocale,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let version = conflict_version(&snapshot, role);
        let count = visible_rows.len();
        let changed = version.map_or(0, |version| version.changed_lines);
        let present = version.is_some();
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .py_2()
                    .px_3()
                    .bg(rgb(PANEL))
                    .text_sm()
                    .text_color(rgb(TEXT))
                    .child(format!(
                        "{title} · {changed} {}",
                        tr(locale, "changed line(s)")
                    )),
            )
            .child(if present && count > 0 {
                gpui::uniform_list(
                    list_name,
                    count,
                    cx.processor(move |_, range: std::ops::Range<usize>, _, _| {
                        let Some(version) = conflict_version(&snapshot, role) else {
                            return Vec::new();
                        };
                        range
                            .filter_map(|index| visible_rows.get(index))
                            .map(|display| {
                                let ConflictDisplayRow::Line(index) = display else {
                                    let ConflictDisplayRow::Fold { hidden } = display else {
                                        unreachable!();
                                    };
                                    return div()
                                        .h(px(22.0))
                                        .w_full()
                                        .px_2()
                                        .flex()
                                        .items_center()
                                        .bg(rgb(0x252c36))
                                        .text_xs()
                                        .text_color(rgb(MUTED))
                                        .child(format!(
                                            "··· {hidden} {} ···",
                                            tr(locale, "unchanged lines hidden")
                                        ))
                                        .into_any_element();
                                };
                                let Some(aligned) = snapshot.aligned_rows.get(*index) else {
                                    return div().h(px(22.0)).into_any_element();
                                };
                                let in_selected_hunk = selected_hunk
                                    .as_ref()
                                    .is_some_and(|range| range.contains(index));
                                let marker = if in_selected_hunk {
                                    0xffcf7a
                                } else if aligned.changed {
                                    accent
                                } else {
                                    BG
                                };
                                let Some(line) = aligned.indices[usize::from(role)]
                                    .and_then(|index| version.lines.get(index))
                                else {
                                    return div()
                                        .h(px(22.0))
                                        .w_full()
                                        .flex()
                                        .bg(rgb(if aligned.changed { 0x20262e } else { BG }))
                                        .child(div().h_full().w(px(3.0)).bg(rgb(marker)))
                                        .into_any_element();
                                };
                                let spans = line.highlights.iter().cloned().map(|span| {
                                    (
                                        span,
                                        gpui::HighlightStyle {
                                            background_color: Some(rgb(accent).into()),
                                            font_weight: Some(gpui::FontWeight::BOLD),
                                            ..Default::default()
                                        },
                                    )
                                });
                                div()
                                    .h(px(22.0))
                                    .w_full()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .whitespace_nowrap()
                                    .bg(rgb(if line.changed { 0x272e38 } else { BG }))
                                    .child(div().h_full().w(px(3.0)).bg(rgb(marker)))
                                    .child(
                                        div()
                                            .w(px(42.0))
                                            .flex()
                                            .justify_end()
                                            .text_xs()
                                            .text_color(rgb(if line.changed {
                                                0xffcf7a
                                            } else {
                                                MUTED
                                            }))
                                            .child(line.number.to_string()),
                                    )
                                    .child(
                                        div()
                                            .min_w_0()
                                            .font_family("Consolas")
                                            .text_sm()
                                            .text_color(rgb(TEXT))
                                            .child(
                                                gpui::StyledText::new(line.text.clone())
                                                    .with_highlights(spans),
                                            ),
                                    )
                                    .into_any_element()
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .flex_1()
                .min_h_0()
                .w_full()
                .px_1()
                .track_scroll(scroll)
                .into_any_element()
            } else {
                div()
                    .flex_1()
                    .p_3()
                    .text_sm()
                    .text_color(rgb(MUTED))
                    .child(tr(
                        locale,
                        if present {
                            "(empty file)"
                        } else {
                            "(no stage: file absent/deleted)"
                        },
                    ))
                    .into_any_element()
            })
            .into_any_element()
    }

    pub(super) fn render_center_git_conflict(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        self.render_git_conflict_view(ConflictViewTarget::Transient, window, cx)
    }

    pub(super) fn render_pinned_git_conflict(
        &mut self,
        instance: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        self.render_git_conflict_view(ConflictViewTarget::Pinned(instance), window, cx)
    }

    fn render_git_conflict_view(
        &mut self,
        target: ConflictViewTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let Some((preview, scrolls)) = self.conflict_view(target) else {
            return div().into_any_element();
        };
        let locale = self.settings_menu_locale;
        let heading = format!(
            "{} · {}",
            tr(locale, "Merge conflict"),
            preview.path.display()
        );
        let origin = tr(
            locale,
            match preview.source {
                ConflictSource::UnresolvedIndex => {
                    "Git index stages 1/2/3 · read-only snapshot · no resolving or staging"
                }
                ConflictSource::PlannedMerge => {
                    "Simulated merge index · read-only snapshot · no file or index changes"
                }
            },
        );
        let result = preview.result.clone();
        let change_count = preview.change_anchors.len();
        let selected_change = preview.selected_change;
        let visible_rows = preview.display_rows.clone();
        let collapse_unchanged = preview.collapse_unchanged;
        let has_folded_rows = visible_rows
            .iter()
            .any(|row| matches!(row, ConflictDisplayRow::Fold { .. }));
        let selected_hunk = result
            .as_ref()
            .and_then(|result| result.as_ref().ok())
            .and_then(|snapshot| {
                active_conflict_hunk(snapshot, &preview.change_anchors, selected_change)
            });
        let focus = preview.focus.clone();
        let previous_available =
            adjacent_conflict_change(selected_change, change_count, false).is_some();
        let next_available =
            adjacent_conflict_change(selected_change, change_count, true).is_some();
        let selected_role = preview.compact_version;
        let width = f32::from(window.viewport_size().width);
        let context_initial = if self.right_sidebar_visible {
            self.right_sidebar_width
        } else {
            PROJECTS_COLLAPSED_WIDTH
        };
        let projects = if self.projects_sidebar_collapsed {
            PROJECTS_COLLAPSED_WIDTH
        } else {
            clamp_projects_sidebar_width(self.projects_sidebar_width, width, context_initial)
        };
        let context = if self.right_sidebar_visible {
            clamp_context_sidebar_width(self.right_sidebar_width, width, projects)
        } else {
            PROJECTS_COLLAPSED_WIDTH
        };
        let center_width = (width - projects - context - RESIZE_CHROME).max(0.0);
        let effective_width = if self.mixed_panes.has_secondary()
            && self.mixed_panes.axis() == PaneAxis::Horizontal
        {
            center_width / 2.0
        } else {
            center_width
        };
        let compact = compact_conflict_layout(effective_width);
        div()
            .id("center-three-way-git-conflict")
            .key_context("AleraGpuiConflict")
            .when_some(focus, |panel, focus| panel.track_focus(&focus))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |view, _, window, cx| {
                    let current_focus = view
                        .conflict_view(target)
                        .and_then(|(preview, _)| preview.focus.clone());
                    if let Some(current_focus) = current_focus {
                        window.focus(&current_focus, cx);
                        match target {
                            ConflictViewTarget::Transient => {
                                view.mixed_panes.focus_pane(PaneSlot::Primary);
                            }
                            ConflictViewTarget::Pinned(instance) => {
                                let identity = WorkbenchTabIdentity::GitConflictCompare(instance);
                                view.mixed_panes.activate(&identity);
                                view.mixed_tabs.select(&identity);
                            }
                        }
                    }
                }),
            )
            .flex_1()
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .px_3()
                    .py_2()
                    .bg(rgb(TOOLBAR))
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .text_color(rgb(TEXT))
                            .child(heading),
                    )
                    .child(
                        div()
                            .id("close-three-way-git-conflict")
                            .cursor_pointer()
                            .text_sm()
                            .text_color(rgb(TEXT))
                            .child(tr(
                                locale,
                                if target == ConflictViewTarget::Transient {
                                    "Close Preview"
                                } else {
                                    "Close tab"
                                },
                            ))
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.close_conflict_view(target, cx);
                            })),
                    ),
            )
            .when(
                target == ConflictViewTarget::Transient && matches!(result.as_ref(), Some(Ok(_))),
                |outer| {
                    outer.child(
                        div()
                            .id("pin-three-way-conflict-to-tab")
                            .cursor_pointer()
                            .px_3()
                            .py_2()
                            .text_sm()
                            .text_color(rgb(TEXT))
                            .child(tr(locale, "Pin conflict comparison to workspace tab"))
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.pin_center_git_conflict(cx);
                            })),
                    )
                },
            )
            .child(
                div()
                    .px_3()
                    .py_2()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(origin),
            )
            .child(
                div()
                    .id("three-way-conflict-navigation")
                    .px_3()
                    .py_1()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_3()
                    .child(div().text_xs().text_color(rgb(MUTED)).child(format!(
                        "{} {}/{}",
                        tr(locale, "Conflict differences"),
                        selected_change.map_or(0, |index| index + 1),
                        change_count,
                    )))
                    .child(if previous_available {
                        div()
                            .id("three-way-conflict-previous-change")
                            .cursor_pointer()
                            .text_sm()
                            .text_color(rgb(TEXT))
                            .child(tr(locale, "Previous difference"))
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.jump_center_conflict_change(target, false, cx);
                            }))
                            .into_any_element()
                    } else {
                        div()
                            .text_sm()
                            .text_color(rgb(MUTED))
                            .child(tr(locale, "Previous difference"))
                            .into_any_element()
                    })
                    .child(if next_available {
                        div()
                            .id("three-way-conflict-next-change")
                            .cursor_pointer()
                            .text_sm()
                            .text_color(rgb(TEXT))
                            .child(tr(locale, "Next difference"))
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.jump_center_conflict_change(target, true, cx);
                            }))
                            .into_any_element()
                    } else {
                        div()
                            .text_sm()
                            .text_color(rgb(MUTED))
                            .child(tr(locale, "Next difference"))
                            .into_any_element()
                    })
                    .child(if result.as_ref().is_some_and(|result| result.is_ok()) {
                        div()
                            .id("three-way-conflict-toggle-context")
                            .cursor_pointer()
                            .text_sm()
                            .text_color(rgb(TEXT))
                            .child(tr(
                                locale,
                                if collapse_unchanged {
                                    "Show all lines"
                                } else {
                                    "Collapse unchanged lines"
                                },
                            ))
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.toggle_conflict_context(target, cx);
                            }))
                            .into_any_element()
                    } else {
                        div().into_any_element()
                    })
                    .when(has_folded_rows, |nav| {
                        nav.child(
                            div()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child(tr(locale, "Long unchanged sections are folded")),
                        )
                    })
                    .child(div().text_xs().text_color(rgb(MUTED)).child(tr(
                        locale,
                        "F7 next · Shift+F7 previous · Alt+U expand/collapse",
                    ))),
            )
            .child(match result {
                None => div()
                    .flex_1()
                    .p_4()
                    .text_sm()
                    .text_color(rgb(MUTED))
                    .child(tr(locale, "Reading Git conflict stages (read-only)…"))
                    .into_any_element(),
                Some(Err(error)) => div()
                    .flex_1()
                    .p_4()
                    .text_sm()
                    .text_color(rgb(0xffa59a))
                    .child(format!(
                        "{}: {error}",
                        tr(locale, "Cannot preview conflict")
                    ))
                    .into_any_element(),
                Some(Ok(snapshot)) if compact => div()
                    .id("three-way-conflict-compact")
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .id("three-way-conflict-version-picker")
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .py_2()
                            .children((0u8..3).map(|role| {
                                let selected = role == selected_role;
                                let label = tr(
                                    locale,
                                    match role {
                                        0 => "Base version",
                                        1 => "Ours version",
                                        _ => "Theirs version",
                                    },
                                );
                                div()
                                    .id(("three-way-conflict-version", usize::from(role)))
                                    .cursor_pointer()
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(rgb(if selected {
                                        BORDER_ACTIVE
                                    } else {
                                        BORDER
                                    }))
                                    .bg(rgb(if selected { ELEVATED } else { TOOLBAR }))
                                    .text_sm()
                                    .text_color(rgb(if selected { TEXT } else { MUTED }))
                                    .child(label)
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        view.select_center_conflict_version(target, role, cx);
                                    }))
                            })),
                    )
                    .child(Self::conflict_version_pane(
                        snapshot,
                        visible_rows.clone(),
                        selected_hunk.clone(),
                        tr(
                            locale,
                            match selected_role {
                                0 => "Base (stage 1)",
                                1 => "Current · Ours (stage 2)",
                                _ => "Incoming · Theirs (stage 3)",
                            },
                        ),
                        selected_role,
                        match selected_role {
                            0 => 0x425b88,
                            1 => 0x765b34,
                            _ => 0x28684f,
                        },
                        match selected_role {
                            0 => "conflict-base-lines",
                            1 => "conflict-ours-lines",
                            _ => "conflict-theirs-lines",
                        },
                        &scrolls[usize::from(selected_role)],
                        locale,
                        cx,
                    ))
                    .into_any_element(),
                Some(Ok(snapshot)) => div()
                    .id("three-way-conflict-columns")
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .flex()
                    .flex_row()
                    .child(Self::conflict_version_pane(
                        snapshot.clone(),
                        visible_rows.clone(),
                        selected_hunk.clone(),
                        tr(locale, "Base (stage 1)"),
                        0,
                        0x425b88,
                        "conflict-base-lines",
                        &scrolls[0],
                        locale,
                        cx,
                    ))
                    .child(Self::conflict_version_pane(
                        snapshot.clone(),
                        visible_rows.clone(),
                        selected_hunk.clone(),
                        tr(locale, "Current · Ours (stage 2)"),
                        1,
                        0x765b34,
                        "conflict-ours-lines",
                        &scrolls[1],
                        locale,
                        cx,
                    ))
                    .child(Self::conflict_version_pane(
                        snapshot,
                        visible_rows,
                        selected_hunk,
                        tr(locale, "Incoming · Theirs (stage 3)"),
                        2,
                        0x28684f,
                        "conflict-theirs-lines",
                        &scrolls[2],
                        locale,
                        cx,
                    ))
                    .into_any_element(),
            })
            .into_any_element()
    }
}

fn conflict_version(snapshot: &GitConflictComparison, role: u8) -> Option<&ConflictVersion> {
    match role {
        0 => snapshot.base.as_ref(),
        1 => snapshot.ours.as_ref(),
        2 => snapshot.theirs.as_ref(),
        _ => None,
    }
}

#[cfg(test)]
mod conflict_navigation_tests {
    use super::*;

    #[test]
    fn long_conflict_fold_keeps_shared_hunk_rows_and_correct_logical_line_numbers() {
        let mut base = (0..250)
            .map(|index| format!("common {index:03}"))
            .collect::<Vec<_>>();
        let mut ours = base.clone();
        let mut theirs = base.clone();
        base[179] = "base 中文".into();
        ours[179] = "ours 中文".into();
        theirs[179] = "theirs 中文".into();
        ours.insert(174, "ours inserted".into());
        theirs.insert(185, "theirs inserted".into());
        let text = |lines: Vec<String>| format!("{}\n", lines.join("\n"));
        let snapshot = GitConflictComparison::from_versions(
            "sample.txt".into(),
            Some(&text(base)),
            Some(&text(ours)),
            Some(&text(theirs)),
        )
        .unwrap();
        let expanded = project_conflict_rows(&snapshot, false);
        let folded = project_conflict_rows(&snapshot, true);
        assert_eq!(expanded.len(), snapshot.aligned_rows.len());
        assert!(
            folded.len() < expanded.len() / 2,
            "long context should fold heavily"
        );
        assert!(folded.iter().any(
            |row| matches!(row, ConflictDisplayRow::Fold { hidden } if *hidden >= MIN_COLLAPSE_GAP)
        ));
        let anchors = conflict_change_anchors(&snapshot);
        assert!(anchors.len() >= 2);
        for anchor in anchors {
            let visible = conflict_projected_anchor(&folded, anchor)
                .expect("every changed hunk remains visible");
            assert_eq!(folded[visible], ConflictDisplayRow::Line(anchor));
            assert!(snapshot.aligned_rows[anchor].changed);
        }
        let mut actual = folded.iter().filter_map(|row| match row {
            ConflictDisplayRow::Line(index) => Some(*index),
            _ => None,
        });
        let first = actual.next().unwrap();
        assert_eq!(first, 0);
        assert!(actual.all(|next| next < snapshot.aligned_rows.len()));
    }

    #[test]
    fn short_and_add_add_files_remain_readable_and_projected_anchors_do_not_change() {
        for snapshot in [
            GitConflictComparison::from_versions(
                "short.txt".into(),
                Some("common\nold\ncommon\n"),
                Some("common\nours\ncommon\n"),
                Some("common\ntheirs\ncommon\n"),
            )
            .unwrap(),
            GitConflictComparison::from_versions(
                "add.txt".into(),
                None,
                Some("ours 中文\n"),
                Some("theirs 中文\n"),
            )
            .unwrap(),
        ] {
            let full = project_conflict_rows(&snapshot, false);
            let folded = project_conflict_rows(&snapshot, true);
            assert_eq!(full, folded);
            assert!(folded
                .iter()
                .all(|row| matches!(row, ConflictDisplayRow::Line(_))));
            for anchor in conflict_change_anchors(&snapshot) {
                assert_eq!(conflict_projected_anchor(&folded, anchor), Some(anchor));
            }
        }
    }

    #[test]
    fn collapsed_projection_keeps_file_edges_when_no_lines_change() {
        let text = (0..100)
            .map(|index| format!("stable {index}"))
            .collect::<Vec<_>>()
            .join("\n");
        let snapshot = GitConflictComparison::from_versions(
            "stable.txt".into(),
            Some(&text),
            Some(&text),
            Some(&text),
        )
        .unwrap();
        let folded = project_conflict_rows(&snapshot, true);
        assert_eq!(folded.first(), Some(&ConflictDisplayRow::Line(0)));
        assert_eq!(folded.last(), Some(&ConflictDisplayRow::Line(99)));
        assert_eq!(
            folded.len(),
            7,
            "first and last 3 lines plus one fold marker"
        );
        assert_eq!(folded[3], ConflictDisplayRow::Fold { hidden: 94 });
        assert!(conflict_change_anchors(&snapshot).is_empty());
        assert!(active_conflict_hunk(&snapshot, &[], None).is_none());
    }

    #[test]
    fn navigation_groups_contiguous_changes_and_never_wraps() {
        let comparison = GitConflictComparison::from_versions(
            "conflict.txt".into(),
            Some("same\nold A\nold B\nsame\nold C\n"),
            Some("same\nours A\nours B\nsame\nours C\n"),
            Some("same\ntheirs A\ntheirs B\nsame\ntheirs C\n"),
        )
        .unwrap();
        let changes = conflict_change_anchors(&comparison);
        assert_eq!(changes, vec![1, 4]);
        assert_eq!(adjacent_conflict_change(None, changes.len(), true), Some(0));
        assert_eq!(
            adjacent_conflict_change(Some(0), changes.len(), true),
            Some(1)
        );
        assert_eq!(adjacent_conflict_change(Some(1), changes.len(), true), None);
        assert_eq!(
            adjacent_conflict_change(Some(1), changes.len(), false),
            Some(0)
        );
        assert_eq!(
            adjacent_conflict_change(Some(0), changes.len(), false),
            None
        );
        assert_eq!(adjacent_conflict_change(None, 0, true), None);
        assert_eq!(adjacent_conflict_change(Some(usize::MAX), 1, true), None);
    }

    #[test]
    fn change_locator_handles_missing_stage_and_far_down_conflicts() {
        let mut base = (0..260).map(|_| "same").collect::<Vec<_>>();
        let mut ours = base.clone();
        let mut theirs = base.clone();
        base[179] = "common ancestor";
        ours[179] = "current";
        theirs[179] = "incoming";
        let text = |items: Vec<&str>| format!("{}\n", items.join("\n"));
        let comparison = GitConflictComparison::from_versions(
            "long.txt".into(),
            Some(&text(base)),
            Some(&text(ours)),
            Some(&text(theirs)),
        )
        .unwrap();
        assert_eq!(conflict_change_anchors(&comparison), vec![179]);

        let add_add = GitConflictComparison::from_versions(
            "new.txt".into(),
            None,
            Some("ours\n"),
            Some("theirs\n"),
        )
        .unwrap();
        assert_eq!(conflict_change_anchors(&add_add), vec![0]);
        assert!(!compact_conflict_layout(900.0));
        assert!(compact_conflict_layout(899.0));
        assert!(compact_conflict_layout(340.0));
    }

    fn preview(path: &str, value: &str) -> CenterConflictPreview {
        let snapshot = Arc::new(
            GitConflictComparison::from_versions(
                path.into(),
                Some("base\n"),
                Some(value),
                Some("theirs\n"),
            )
            .unwrap(),
        );
        CenterConflictPreview {
            workspace_generation: 7,
            path: path.into(),
            source: ConflictSource::UnresolvedIndex,
            change_anchors: conflict_change_anchors(&snapshot),
            selected_change: None,
            compact_version: 1,
            collapse_unchanged: true,
            display_rows: project_conflict_rows(&snapshot, true),
            focus: None,
            result: Some(Ok(snapshot)),
        }
    }

    #[test]
    fn pinned_three_way_snapshot_deduplicates_only_identical_versions_and_stays_immutable() {
        let mut tabs = PinnedConflictTabs::default();
        let a = preview("src/sample.rs", "ours\n");
        let first = tabs.open(&a).unwrap();
        assert_eq!(
            tabs.open(&preview("src/sample.rs", "ours\n")).unwrap(),
            first
        );
        let different = tabs.open(&preview("src/sample.rs", "modified\n")).unwrap();
        assert_ne!(first, different);
        assert_eq!(tabs.entries().len(), 2);
        assert_eq!(tabs.get(first).unwrap().title(), "sample.rs · 3-way");
        assert_eq!(
            tabs.get(first)
                .unwrap()
                .preview
                .result
                .as_ref()
                .unwrap()
                .as_ref()
                .unwrap()
                .ours
                .as_ref()
                .unwrap()
                .lines[0]
                .text,
            "ours"
        );
        assert!(tabs.close(first));
        assert!(tabs.get(first).is_none());
        tabs.reset();
        let next = tabs.open(&a).unwrap();
        assert_ne!(
            next, first,
            "old mixed-tab identity cannot rebind after workspace switch"
        );
    }

    #[test]
    fn pinned_conflicts_have_a_bound_and_never_accept_loading_or_failed_previews() {
        let mut tabs = PinnedConflictTabs::default();
        let mut not_loaded = preview("wait.txt", "ours\n");
        not_loaded.result = None;
        assert!(tabs.open(&not_loaded).is_err());
        not_loaded.result = Some(Err("unknown".into()));
        assert!(tabs.open(&not_loaded).is_err());
        for index in 0..MAX_PINNED_CONFLICTS {
            tabs.open(&preview(&format!("{index}.txt"), "ours\n"))
                .unwrap();
        }
        assert!(tabs.open(&preview("overflow.txt", "ours\n")).is_err());
        assert_eq!(tabs.entries().len(), MAX_PINNED_CONFLICTS);
    }
}
