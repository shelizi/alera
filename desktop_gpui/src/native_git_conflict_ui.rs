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

pub(super) struct CenterConflictPreview {
    workspace_generation: u64,
    path: PathBuf,
    source: ConflictSource,
    result: Option<Result<Arc<GitConflictComparison>, String>>,
    change_anchors: Vec<usize>,
    selected_change: Option<usize>,
    compact_version: u8,
}

/// Consecutive changed lines make one navigation target. Use only the
/// immutable, bounded conflict snapshot; never query or mutate Git in render.
fn conflict_change_anchors(snapshot: &GitConflictComparison) -> Vec<usize> {
    let versions = [&snapshot.base, &snapshot.ours, &snapshot.theirs];
    let longest = versions
        .iter()
        .filter_map(|item| item.as_ref().map(|version| version.lines.len()))
        .max()
        .unwrap_or(0);
    let mut changed = vec![false; longest];
    for version in versions.into_iter().flatten() {
        for (index, line) in version.lines.iter().enumerate() {
            changed[index] |= line.changed;
        }
    }
    changed
        .iter()
        .enumerate()
        .filter_map(|(index, value)| {
            (*value && (index == 0 || !changed[index - 1])).then_some(index)
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
        });
        self.center_git_diff_source = None;
        self.center_git_diff_hunk = None;
        self.git_original_active = false;
        self.active_git_compare_tab = None;
        self.git_graph_center_open = false;
        for scroll in &self.center_git_conflict_scroll {
            scroll.scroll_to_item(0, gpui::ScrollStrategy::Top);
        }
        cx.notify();
    }

    /// Jump all three bounded virtual lists to the same changed region.
    /// Wheel scrolling remains independent when lines have shifted.
    fn jump_center_conflict_change(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(preview) = self.center_git_conflict.as_mut() else {
            return;
        };
        if preview.workspace_generation != self.workspace_generation {
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
        for (index, scroll) in self.center_git_conflict_scroll.iter().enumerate() {
            if let Some(version) = conflict_version(snapshot, index as u8) {
                if !version.lines.is_empty() {
                    scroll.scroll_to_item(
                        anchor.min(version.lines.len() - 1),
                        gpui::ScrollStrategy::Center,
                    );
                }
            }
        }
        cx.notify();
    }

    fn select_center_conflict_version(&mut self, role: u8, cx: &mut Context<Self>) {
        let Some(preview) = self.center_git_conflict.as_mut() else {
            return;
        };
        if role > 2 {
            return;
        }
        preview.compact_version = role;
        if let (Some(Ok(snapshot)), Some(change)) = (&preview.result, preview.selected_change) {
            if let Some(version) = conflict_version(snapshot, role) {
                if !version.lines.is_empty() {
                    let anchor = preview.change_anchors[change].min(version.lines.len() - 1);
                    self.center_git_conflict_scroll[usize::from(role)]
                        .scroll_to_item(anchor, gpui::ScrollStrategy::Center);
                }
            }
        }
        cx.notify();
    }

    fn conflict_version_pane(
        snapshot: Arc<GitConflictComparison>,
        title: String,
        role: u8,
        accent: u32,
        list_name: &'static str,
        scroll: &UniformListScrollHandle,
        locale: UiLocale,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let version = conflict_version(&snapshot, role);
        let count = version.map_or(0, |version| version.lines.len());
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
                            .filter_map(|index| version.lines.get(index))
                            .map(|line| {
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
        let Some(preview) = self.center_git_conflict.as_ref() else {
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
                            .child(tr(locale, "Close Preview"))
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.close_center_git_conflict(cx);
                            })),
                    ),
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
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.jump_center_conflict_change(false, cx);
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
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.jump_center_conflict_change(true, cx);
                            }))
                            .into_any_element()
                    } else {
                        div()
                            .text_sm()
                            .text_color(rgb(MUTED))
                            .child(tr(locale, "Next difference"))
                            .into_any_element()
                    }),
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
                                        view.select_center_conflict_version(role, cx);
                                    }))
                            })),
                    )
                    .child(Self::conflict_version_pane(
                        snapshot,
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
                        &self.center_git_conflict_scroll[usize::from(selected_role)],
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
                        tr(locale, "Base (stage 1)"),
                        0,
                        0x425b88,
                        "conflict-base-lines",
                        &self.center_git_conflict_scroll[0],
                        locale,
                        cx,
                    ))
                    .child(Self::conflict_version_pane(
                        snapshot.clone(),
                        tr(locale, "Current · Ours (stage 2)"),
                        1,
                        0x765b34,
                        "conflict-ours-lines",
                        &self.center_git_conflict_scroll[1],
                        locale,
                        cx,
                    ))
                    .child(Self::conflict_version_pane(
                        snapshot,
                        tr(locale, "Incoming · Theirs (stage 3)"),
                        2,
                        0x28684f,
                        "conflict-theirs-lines",
                        &self.center_git_conflict_scroll[2],
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
}
