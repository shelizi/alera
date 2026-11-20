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
