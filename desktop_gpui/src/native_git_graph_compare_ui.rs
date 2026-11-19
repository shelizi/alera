//! Read-only, bounded, virtualized branch-tip Diff in the mixed Git History tab.
//! It does not replace an Editor buffer or own a Terminal/Host session.

use super::*;

impl Workbench {
    pub(super) fn close_center_branch_compare(&mut self, cx: &mut Context<Self>) {
        self.git_diff_request = self.git_diff_request.wrapping_add(1);
        self.git_graph_compare_center = false;
        self.git_graph_compare_selected = None;
        self.git_graph_compare_rows = Arc::new(Vec::new());
        self.git_branch_compare_loading = false;
        self.git_branch_comparison = None;
        self.git_diff_loading = false;
        self.git_diff = None;
        cx.notify();
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
                                                .font_family("JetBrains Mono")
                                                .text_xs()
                                                .whitespace_nowrap()
                                                .text_color(rgb(
                                                    native_action_dialog_ui::git_patch_line_color(
                                                        line,
                                                    ),
                                                ))
                                                .child(line.clone())
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
