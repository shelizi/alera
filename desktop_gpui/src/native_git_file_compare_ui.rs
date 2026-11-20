//! Classic-equivalent, read-only file revision and branch comparison dialogs.
//! GPUI Kit owns dialog behavior; Git2 comparison runs only on a worker.
use super::*;
use alera_gpui_desktop::git_file_revision_compare::{
    compare_file_with_git_reference, list_local_git_branches,
};
use alera_gpui_desktop::git_file_split_diff::{align_git_file_patch, GitDiffCell, GitDiffCellKind};
use alera_gpui_desktop::native_i18n::tr;

fn render_git_compare_cell(cell: Option<&GitDiffCell>) -> gpui::AnyElement {
    let (line, content, background, ink) = match cell {
        Some(cell) => {
            let background = match cell.kind {
                GitDiffCellKind::Context => BG,
                GitDiffCellKind::Removed => 0x3a282d,
                GitDiffCellKind::Added => 0x183b30,
            };
            (cell.line.to_string(), cell.text.clone(), background, TEXT)
        }
        None => (String::new(), String::new(), PANEL, MUTED),
    };
    div()
        .flex_1()
        .min_w_0()
        .h(px(24.0))
        .flex()
        .items_center()
        .gap_1()
        .bg(rgb(background))
        .child(
            div()
                .w(px(48.0))
                .flex_shrink_0()
                .text_xs()
                .text_color(rgb(MUTED))
                .flex()
                .justify_end()
                .child(line),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .font_family("Consolas")
                .text_sm()
                .text_color(rgb(ink))
                .child(content),
        )
        .into_any_element()
}

impl Workbench {
    fn render_git_file_split_table(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let rows = self.git_file_compare_rows.clone();
        let count = rows.len();
        div()
            .id("git-file-compare-split-table")
            .h(px(418.0))
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .child(
                div()
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(TOOLBAR))
                    .child(
                        div()
                            .flex_1()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .px_2()
                            .child("Committed reference · old"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .px_2()
                            .border_l_1()
                            .border_color(rgb(BORDER))
                            .child("Current working file · new"),
                    ),
            )
            .child(
                gpui::uniform_list(
                    "git-file-compare-virtual-split-rows",
                    count,
                    cx.processor(move |_, range: std::ops::Range<usize>, _, _| {
                        range
                            .filter_map(|index| rows.get(index))
                            .map(|row| {
                                if let Some(label) = row.label.as_ref() {
                                    div()
                                        .h(px(24.0))
                                        .bg(rgb(0x28364a))
                                        .flex()
                                        .items_center()
                                        .px_3()
                                        .text_xs()
                                        .text_color(rgb(TEXT))
                                        .child(label.clone())
                                        .into_any_element()
                                } else {
                                    div()
                                        .h(px(24.0))
                                        .flex()
                                        .items_center()
                                        .child(render_git_compare_cell(row.old.as_ref()))
                                        .child(div().w(px(1.0)).h_full().bg(rgb(BORDER)))
                                        .child(render_git_compare_cell(row.new.as_ref()))
                                        .into_any_element()
                                }
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .flex_1()
                .min_h_0()
                .w_full(),
            )
            .into_any_element()
    }

    fn render_git_file_compare_dialog(
        &mut self,
        generation: u64,
        ticket: u64,
        requested_path: &str,
        requested_ref: &str,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        if self.workspace_generation != generation || self.git_file_compare_ticket != ticket {
            return div()
                .p_4()
                .text_color(rgb(MUTED))
                .child("Workspace or comparison changed. Close this dialog and select again.")
                .into_any_element();
        }
        let result = self
            .git_file_compare_result
            .as_ref()
            .filter(|(current, _)| *current == ticket);
        let (title, detail, patch, valid) = match result {
            Some((_, Ok(diff))) => (
                "Read-only Git comparison",
                format!(
                    "{}  ·  {} @ {}  →  worktree  ·  +{} / -{}{}",
                    diff.relative.display(),
                    diff.reference,
                    &diff.revision_oid[..diff.revision_oid.len().min(10)],
                    self.git_file_compare_additions,
                    self.git_file_compare_deletions,
                    if self.git_file_compare_clipped {
                        " · preview clipped"
                    } else {
                        ""
                    }
                ),
                diff.patch.clone(),
                true,
            ),
            Some((_, Err(error))) => (
                "Git comparison unavailable",
                format!("{requested_path} · {requested_ref}"),
                error.clone(),
                false,
            ),
            _ => (
                "Reading Git comparison",
                format!("{requested_path} · {requested_ref} → worktree · read-only"),
                "Loading committed file revision and working file…".to_owned(),
                false,
            ),
        };
        let split = self.git_file_compare_split;
        let displayed = if valid && split {
            self.render_git_file_split_table(cx)
        } else {
            div()
                .id("git-file-compare-unified-patch")
                .h(px(418.0))
                .overflow_y_scroll()
                .bg(rgb(BG))
                .p_3()
                .font_family("Consolas")
                .text_sm()
                .text_color(rgb(TEXT))
                .child(patch)
                .into_any_element()
        };
        div()
            .id("git-file-revision-compare-dialog")
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .text_color(rgb(TEXT))
                            .child(title),
                    )
                    .when(valid, |bar| {
                        bar.child(
                            div()
                                .id("git-file-compare-layout-toggle")
                                .cursor_pointer()
                                .px_2()
                                .py_1()
                                .bg(rgb(TOOLBAR))
                                .border_1()
                                .border_color(rgb(BORDER))
                                .text_xs()
                                .text_color(rgb(TEXT))
                                .child(if split {
                                    "Unified view"
                                } else {
                                    "Side-by-side view"
                                })
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    if view.workspace_generation == generation
                                        && view.git_file_compare_ticket == ticket
                                    {
                                        view.git_file_compare_split = !view.git_file_compare_split;
                                        cx.notify();
                                    }
                                })),
                        )
                    }),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(format!("{detail} · Changed sections only · no Git writes")),
            )
            .child(displayed)
            .into_any_element()
    }

    pub(super) fn open_git_file_revision_comparison(
        &mut self,
        relative: PathBuf,
        branch: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.git_file_compare_ticket = self.git_file_compare_ticket.wrapping_add(1);
        let ticket = self.git_file_compare_ticket;
        let generation = self.workspace_generation;
        let root = self.model.root().to_path_buf();
        let label = branch.clone().unwrap_or_else(|| "HEAD".to_owned());
        self.git_file_compare_result = None;
        self.git_file_compare_split = true;
        self.git_file_compare_rows = Arc::new(Vec::new());
        self.git_file_compare_additions = 0;
        self.git_file_compare_deletions = 0;
        self.git_file_compare_clipped = false;
        self.model.set_status(format!(
            "Comparing {} with {label} (read-only)…",
            relative.display()
        ));
        let owner = cx.entity().downgrade();
        let dialog_path = relative.display().to_string();
        window.open_dialog(cx, move |dialog, _, cx| {
            let content = owner
                .upgrade()
                .map(|entity| {
                    entity.update(cx, |view, cx| {
                        view.render_git_file_compare_dialog(
                            generation,
                            ticket,
                            &dialog_path,
                            &label,
                            cx,
                        )
                    })
                })
                .unwrap_or_else(|| div().p_4().child("Workspace closed").into_any_element());
            dialog
                .title("Git file comparison")
                .w(px(1050.))
                .centered()
                .on_cancel(|_, _, _| true)
                .on_ok(|_, _, _| true)
                .child(content)
        });
        cx.spawn(async move |view, cx| {
            let (result, aligned) = cx
                .background_spawn(async move {
                    let result =
                        compare_file_with_git_reference(&root, &relative, branch.as_deref());
                    let aligned = result
                        .as_ref()
                        .ok()
                        .map(|diff| align_git_file_patch(&diff.patch));
                    (result, aligned)
                })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation || view.git_file_compare_ticket != ticket
                {
                    return;
                }
                if let Some(aligned) = aligned {
                    view.git_file_compare_additions = aligned.additions;
                    view.git_file_compare_deletions = aligned.deletions;
                    view.git_file_compare_clipped = aligned.clipped;
                    view.git_file_compare_rows = Arc::new(aligned.rows);
                }
                view.model.set_status(match &result {
                    Ok(diff) => format!(
                        "Compared {} with {} without editing files",
                        diff.relative.display(),
                        diff.reference
                    ),
                    Err(error) => format!("File revision comparison refused: {error}"),
                });
                view.git_file_compare_result = Some((ticket, result));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Local branch selection is an inert, searchable list of names returned
    /// by Git2. A typed query never becomes a revspec or shell argument.
    pub(super) fn open_git_file_branch_picker(
        &mut self,
        relative: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let branches = match list_local_git_branches(self.model.root()) {
            Ok(branches) => branches,
            Err(error) => {
                self.model
                    .set_status(format!("Branch search refused: {error}"));
                cx.notify();
                return;
            }
        };
        if branches.is_empty() {
            self.model
                .set_status("No local branches available for file comparison");
            cx.notify();
            return;
        }
        let input = cx.new(|cx| QuickOpenInput::with_placeholder("Search local branches…", cx));
        cx.observe(&input, |_: &mut Workbench, _, cx| cx.notify())
            .detach();
        let owner = cx.entity().downgrade();
        let generation = self.workspace_generation;
        let first_focus = std::cell::Cell::new(true);
        let locale = self.settings_menu_locale;
        window.open_dialog(cx, move |dialog, window, cx| {
            if first_focus.replace(false) {
                window.focus(&input.read(cx).focus_handle(cx), cx);
            }
            let query = input.read(cx).query().trim().to_lowercase();
            let matching = branches
                .iter()
                .filter(|branch| branch.to_lowercase().contains(&query))
                .count();
            let rows = branches
                .iter()
                .filter(|branch| branch.to_lowercase().contains(&query))
                .take(60)
                .enumerate()
                .map(|(i, branch)| {
                    let selected_branch = branch.clone();
                    let selected_path = relative.clone();
                    let selected_owner = owner.clone();
                    div()
                        .id(("git-file-compare-branch", i))
                        .flex()
                        .items_center()
                        .px_3()
                        .h(px(30.0))
                        .cursor_pointer()
                        .hover(|style| style.bg(rgb(ACTIVE)))
                        .text_sm()
                        .child(selected_branch.clone())
                        .on_click(move |_, window, cx| {
                            window.close_dialog(cx);
                            let _ = selected_owner.update(cx, |view, cx| {
                                if view.workspace_generation == generation {
                                    view.open_git_file_revision_comparison(
                                        selected_path.clone(),
                                        Some(selected_branch.clone()),
                                        window,
                                        cx,
                                    );
                                }
                            });
                        })
                })
                .collect::<Vec<_>>();
            dialog
                .title(tr(locale, "Compare With Branch"))
                .w(px(540.))
                .centered()
                .on_cancel(|_, _, _| true)
                .on_ok(|_, _, _| false)
                .child(
                    div()
                        .p_3()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(div().child(input.clone()))
                        .child(div().text_xs().text_color(rgb(MUTED)).child(format!(
                            "{matching} matching local branch(es); showing up to 60"
                        )))
                        .child(
                            div()
                                .id("git-file-branch-results")
                                .h(px(300.0))
                                .overflow_y_scroll()
                                .children(rows),
                        ),
                )
        });
    }
}
