//! Read-only, bounded, virtualized branch-tip Diff in the mixed Git History tab.
//! It does not replace an Editor buffer or own a Terminal/Host session.

use super::*;

/// Parsed once by the comparison background reader; uniform-list renders
/// these owned rows without rescanning the Git patch on every GPUI frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CenterBranchPatchRow {
    pub(super) text: String,
    pub(super) old: Option<usize>,
    pub(super) new: Option<usize>,
    pub(super) hunk: Option<(usize, usize)>,
}

pub(super) fn parse_center_branch_patch_rows(patch: &str) -> Vec<CenterBranchPatchRow> {
    const ROW_LIMIT: usize = 8192;
    let mut rows = native_action_dialog_ui::parse_git_diff_rows(patch, ROW_LIMIT + 1)
        .into_iter()
        .map(|row| {
            let mut text = row.text.chars().take(4097).collect::<String>();
            if text.chars().count() > 4096 {
                text = text.chars().take(4096).collect();
                text.push_str(" ... row clipped");
            }
            CenterBranchPatchRow {
                text,
                old: row.old,
                new: row.new,
                hunk: row.hunk,
            }
        })
        .collect::<Vec<_>>();
    if rows.len() > ROW_LIMIT {
        rows.truncate(ROW_LIMIT);
        rows.push(CenterBranchPatchRow {
            text: "... preview row limit reached".to_owned(),
            old: None,
            new: None,
            hunk: None,
        });
    }
    rows
}

/// Return a bounded (hunk ordinal, virtual-list row) pair, not a raw scroll
/// offset. Refuse underflow and overflow instead of wrapping to old results.
pub(super) fn center_branch_hunk_target(
    rows: &[CenterBranchPatchRow],
    selected: Option<usize>,
    next: bool,
) -> Option<(usize, usize)> {
    let positions = rows
        .iter()
        .enumerate()
        .filter_map(|(row, item)| item.hunk.map(|_| row))
        .collect::<Vec<_>>();
    if positions.is_empty() {
        return None;
    }
    let target = if next {
        selected.map_or(Some(0), |index| index.checked_add(1))
    } else {
        selected.and_then(|index| index.checked_sub(1))
    }?;
    positions.get(target).map(|row| (target, *row))
}

impl Workbench {
    pub(super) fn close_center_branch_compare(&mut self, cx: &mut Context<Self>) {
        self.git_diff_request = self.git_diff_request.wrapping_add(1);
        self.git_graph_compare_center = false;
        self.git_graph_compare_selected = None;
        self.git_graph_compare_rows = Arc::new(Vec::new());
        self.git_graph_compare_hunk = None;
        self.git_branch_compare_loading = false;
        self.git_branch_comparison = None;
        self.git_diff_loading = false;
        self.git_diff = None;
        cx.notify();
    }

    fn jump_center_branch_compare_hunk(&mut self, next: bool, cx: &mut Context<Self>) {
        if !self.git_graph_compare_center
            || self.git_diff_loading
            || self.git_branch_compare_loading
        {
            return;
        }
        if let Some((hunk, row)) = center_branch_hunk_target(
            &self.git_graph_compare_rows,
            self.git_graph_compare_hunk,
            next,
        ) {
            self.git_graph_compare_hunk = Some(hunk);
            self.git_graph_compare_scroll
                .scroll_to_item(row, gpui::ScrollStrategy::Center);
            cx.notify();
        }
    }

    pub(super) fn render_center_branch_compare(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let comparison = self.git_branch_comparison.clone();
        let selected_index = self.git_graph_compare_selected;
        let loading = self.git_branch_compare_loading;
        let preview_loading = self.git_diff_loading;
        let patch_rows = self.git_graph_compare_rows.clone();
        let generation = self.workspace_generation;
        let compact = self.mixed_panes.has_secondary();
        let summary = comparison
            .as_ref()
            .map(|c| {
                format!(
                    "{} | {} ahead / {} behind | {} files | read-only",
                    c.title(),
                    c.ahead,
                    c.behind,
                    c.files.len()
                )
            })
            .unwrap_or_else(|| {
                if loading {
                    "Comparing committed branch tips...".to_owned()
                } else {
                    self.git_diff
                        .clone()
                        .unwrap_or_else(|| "No comparison selected".into())
                }
            });
        let selected_branch = comparison.as_ref().map(|c| c.selected_branch.clone());
        let reverse = comparison
            .as_ref()
            .is_some_and(|c| c.direction == CompareDirection::SelectedToCurrent);
        let has_comparison = comparison.is_some();
        let files = comparison
            .as_ref()
            .map(|c| c.files.clone())
            .unwrap_or_default();
        let file_count = files.len();
        let list = files
            .into_iter()
            .enumerate()
            .map(|(index, file)| {
                let selected = selected_index == Some(index);
                let status = file.status;
                div()
                    .id(("git-center-compare-file", index))
                    .min_w_0()
                    .flex_shrink_0()
                    .h(px(25.0))
                    .px_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .cursor_pointer()
                    .when(selected, |item| item.bg(rgb(ACTIVE)))
                    .hover(|style| style.bg(rgb(ELEVATED)))
                    .child(
                        div()
                            .w(px(18.0))
                            .text_xs()
                            .text_color(rgb(TEXT))
                            .child(status),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .text_color(rgb(if selected { TEXT } else { MUTED }))
                            .child(file.display_path()),
                    )
                    .when(status != "A", |item| {
                        item.child(
                            icon_button(
                                ("git-center-compare-original", index),
                                UiIcon::Open,
                                "Open committed source-side original as a read-only tab",
                                false,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.open_git_compared_original_tab(index, cx);
                                },
                            )),
                        )
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.preview_git_compared_file(index, false, cx);
                    }))
            })
            .collect::<Vec<_>>();
        let patch_count = patch_rows.len();
        let hunk_count = patch_rows.iter().filter(|row| row.hunk.is_some()).count();
        let active_hunk = self.git_graph_compare_hunk.map_or(0, |index| index + 1);

        div()
            .id("git-center-branch-diff")
            .flex_1()
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            .border_t_1()
            .border_color(rgb(BORDER))
            .bg(rgb(PANEL))
            .child(
                div()
                    .h(px(38.0))
                    .flex_shrink_0()
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(icon(UiIcon::Branch, 15.0, TEXT))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_xs()
                            .text_color(rgb(TEXT))
                            .child(summary),
                    )
                    .when_some(selected_branch, |row, branch| {
                        row.child(
                            icon_button(
                                "git-center-compare-reverse",
                                UiIcon::History,
                                "Reverse committed branch tips and Diff additions/deletions",
                                false,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    let next = if reverse {
                                        CompareDirection::CurrentToSelected
                                    } else {
                                        CompareDirection::SelectedToCurrent
                                    };
                                    this.compare_git_graph_center_branch(
                                        branch.clone(),
                                        next,
                                        generation,
                                        cx,
                                    );
                                },
                            )),
                        )
                    })
                    .child(
                        icon_button(
                            "git-center-compare-close",
                            UiIcon::Close,
                            "Close Git Graph comparison (does not change Git or Editor state)",
                            false,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.close_center_branch_compare(cx);
                        })),
                    ),
            )
            .when(file_count > 0, |panel| {
                panel.child(
                    div()
                        .id("git-center-compare-files")
                        .h(px(if compact { 76.0 } else { 112.0 }))
                        .flex_shrink_0()
                        .overflow_y_scroll()
                        .px_2()
                        .border_b_1()
                        .border_color(rgb(BORDER))
                        .children(list),
                )
            })
            .when(file_count == 0 && !loading && has_comparison, |panel| {
                panel.child(
                    div()
                        .px_3()
                        .py_2()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child("No changed files at the compared commit tips"),
                )
            })
            .child(
                div()
                    .id("git-center-compare-patch")
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .when(!preview_loading && hunk_count > 0, |pane| {
                        pane.child(
                            div()
                                .h(px(30.0))
                                .flex_shrink_0()
                                .px_3()
                                .flex()
                                .items_center()
                                .gap_2()
                                .border_b_1()
                                .border_color(rgb(BORDER))
                                .child(div().text_xs().text_color(rgb(MUTED))
                                    .child(format!("Hunk {active_hunk}/{hunk_count}")))
                                .child(icon_button(
                                    "git-center-compare-prev-hunk",
                                    UiIcon::ChevronUp,
                                    "Previous committed comparison hunk",
                                    false,
                                ).on_click(cx.listener(|this, _, _, cx| {
                                    this.jump_center_branch_compare_hunk(false, cx);
                                })))
                                .child(icon_button(
                                    "git-center-compare-next-hunk",
                                    UiIcon::ChevronDown,
                                    "Next committed comparison hunk",
                                    false,
                                ).on_click(cx.listener(|this, _, _, cx| {
                                    this.jump_center_branch_compare_hunk(true, cx);
                                }))),
                        )
                    })
                    .when(preview_loading && !loading, |pane| {
                        pane.child(
                            div()
                                .px_3()
                                .py_1()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child("Loading immutable file patch..."),
                        )
                    })
                    .when(!preview_loading && selected_index.is_none(), |pane| {
                        pane.child(
                            div()
                                .px_3()
                                .py_1()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child("Select a changed file to inspect its committed patch"),
                        )
                    })
                    .when(!preview_loading && patch_count > 0, |pane| {
                        pane.child(
                            gpui::uniform_list(
                                "git-center-compare-virtual-lines",
                                patch_count,
                                cx.processor(move |_, range: std::ops::Range<usize>, _, _| {
                                    range
                                        .filter_map(|index| patch_rows.get(index))
                                        .map(|line| {
                                            div()
                                                .h(px(21.0))
                                                .px_2()
                                                .flex()
                                                .items_center()
                                                .gap_1()
                                                .font_family("JetBrains Mono")
                                                .text_xs()
                                                .whitespace_nowrap()
                                                .when(line.hunk.is_some(), |row| {
                                                    row.bg(rgb(0x28364a))
                                                })
                                                .child(div().w(px(44.0)).flex_shrink_0()
                                                    .text_color(rgb(MUTED))
                                                    .child(line.old.map_or_else(String::new, |n| n.to_string())))
                                                .child(div().w(px(44.0)).flex_shrink_0()
                                                    .text_color(rgb(MUTED))
                                                    .child(line.new.map_or_else(String::new, |n| n.to_string())))
                                                .child(div().min_w_0()
                                                    .text_color(rgb(native_action_dialog_ui::git_patch_line_color(
                                                        &line.text,
                                                    )))
                                                    .child(line.text.clone()))
                                                .into_any_element()
                                        })
                                        .collect::<Vec<_>>()
                                }),
                            )
                            .flex_1()
                            .min_h_0()
                            .w_full()
                            .px_2()
                            .py_1()
                            .track_scroll(&self.git_graph_compare_scroll),
                        )
                    }),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{center_branch_hunk_target, parse_center_branch_patch_rows};

    #[test]
    fn branch_patch_gutters_track_both_directions_and_header_resets() {
        let patch = concat!(
            "main -> topic (committed tips)\n",
            "shared.txt [M]\n",
            "\n",
            "diff --git a/shared.txt b/shared.txt\n",
            "--- a/shared.txt\n",
            "+++ b/shared.txt\n",
            "@@ -3,2 +3,2 @@\n",
            "-old\n",
            "+new\n",
            " shared\n",
            "@@ -21 +21 @@\n",
            "-before\n",
            "+after\n",
        );
        let rows = parse_center_branch_patch_rows(patch);
        let delete = rows.iter().find(|row| row.text == "-old").unwrap();
        assert_eq!((delete.old, delete.new), (Some(3), None));
        let add = rows.iter().find(|row| row.text == "+new").unwrap();
        assert_eq!((add.old, add.new), (None, Some(3)));
        let context = rows.iter().find(|row| row.text == " shared").unwrap();
        assert_eq!((context.old, context.new), (Some(4), Some(4)));
        assert_eq!(
            center_branch_hunk_target(&rows, None, true).map(|t| t.0),
            Some(0)
        );
        assert_eq!(
            center_branch_hunk_target(&rows, Some(0), true).map(|t| t.0),
            Some(1)
        );
        assert_eq!(
            center_branch_hunk_target(&rows, Some(1), false).map(|t| t.0),
            Some(0)
        );
        assert_eq!(center_branch_hunk_target(&rows, Some(1), true), None);
        assert_eq!(center_branch_hunk_target(&rows, Some(0), false), None);
        assert_eq!(center_branch_hunk_target(&rows, None, false), None);
    }

    #[test]
    fn branch_patch_limits_are_bounded_and_renames_have_no_false_hunks() {
        let rename = "main -> topic\nold.txt -> renamed.txt [R]\n\n\
diff --git a/old.txt b/renamed.txt\nsimilarity index 100%\n\
rename from old.txt\nrename to renamed.txt\n";
        let rows = parse_center_branch_patch_rows(rename);
        assert!(rows.iter().all(|row| row.hunk.is_none()));
        assert!(rows
            .iter()
            .all(|row| row.old.is_none() && row.new.is_none()));
        assert_eq!(center_branch_hunk_target(&rows, None, true), None);
        let huge = format!("@@ -1 +1 @@\n+{}\n", "界".repeat(6000));
        let rows = parse_center_branch_patch_rows(&huge);
        assert!(rows[1].text.chars().count() <= 4120);
        assert!(rows[1].text.contains("row clipped"));
        let huge_rows = format!("@@ -1 +1 @@\n{}", " ctx\n".repeat(8300));
        let rows = parse_center_branch_patch_rows(&huge_rows);
        assert_eq!(rows.len(), 8193);
        assert!(rows.last().unwrap().text.contains("row limit"));
    }
}
