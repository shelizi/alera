//! Classic-equivalent, read-only file revision and branch comparison dialogs.
//! GPUI Kit owns dialog behavior; Git2 comparison runs only on a worker.
use super::*;
use alera_gpui_desktop::git_file_revision_compare::{
    compare_file_with_git_reference, list_local_git_branches,
};
use alera_gpui_desktop::git_file_split_diff::{
    align_git_file_patch, flatten_full_file_rows, GitDiffCell, GitDiffCellKind, GitSplitDiffRow,
};
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
    fn render_git_file_split_table(
        &mut self,
        full: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let rows = if full {
            self.git_file_compare_full_rows.clone()
        } else {
            self.git_file_compare_rows.clone()
        };
        self.render_git_file_split_rows(rows, full, false, "dialog".into(), cx)
    }

    fn render_git_file_split_rows(
        &mut self,
        rows: Arc<Vec<GitSplitDiffRow>>,
        full: bool,
        embedded: bool,
        scope: String,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let count = rows.len();
        div()
            .id(format!("git-file-compare-split-table-{scope}"))
            .when(embedded, |node| node.flex_1().min_h_0())
            .when(!embedded, |node| node.h(px(418.0)))
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
                    format!(
                        "git-compare-{scope}-{}-split",
                        if full { "full" } else { "diff" }
                    ),
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

    fn render_git_file_full_unified(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        self.render_git_file_full_unified_rows(
            self.git_file_compare_full_unified_rows.clone(),
            false,
            "dialog".into(),
            cx,
        )
    }

    fn render_git_file_full_unified_rows(
        &mut self,
        rows: Arc<Vec<GitDiffCell>>,
        embedded: bool,
        scope: String,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        if rows.is_empty() {
            return div()
                .id(format!("git-file-full-unified-empty-{scope}"))
                .when(embedded, |node| node.flex_1().min_h_0())
                .when(!embedded, |node| node.h(px(418.0)))
                .p_3()
                .text_color(rgb(MUTED))
                .child("Both files are empty")
                .into_any_element();
        }
        let count = rows.len();
        div()
            .id(format!("git-file-full-unified-table-{scope}"))
            .when(embedded, |node| node.flex_1().min_h_0())
            .when(!embedded, |node| node.h(px(418.0)))
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .child(
                div()
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .px_3()
                    .bg(rgb(TOOLBAR))
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child("Complete file · old deletions and new additions · read-only"),
            )
            .child(
                gpui::uniform_list(
                    format!("git-compare-{scope}-full-unified"),
                    count,
                    cx.processor(move |_, range: std::ops::Range<usize>, _, _| {
                        range
                            .filter_map(|index| rows.get(index))
                            .map(|cell| {
                                let (marker, background) = match cell.kind {
                                    GitDiffCellKind::Context => (" ", BG),
                                    GitDiffCellKind::Removed => ("-", 0x3a282d),
                                    GitDiffCellKind::Added => ("+", 0x183b30),
                                };
                                div()
                                    .h(px(24.0))
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .px_2()
                                    .bg(rgb(background))
                                    .font_family("Consolas")
                                    .text_sm()
                                    .child(
                                        div()
                                            .w(px(48.0))
                                            .flex_shrink_0()
                                            .flex()
                                            .justify_end()
                                            .text_xs()
                                            .text_color(rgb(MUTED))
                                            .child(cell.line.to_string()),
                                    )
                                    .child(div().w(px(10.0)).text_color(rgb(MUTED)).child(marker))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .truncate()
                                            .text_color(rgb(TEXT))
                                            .child(cell.text.clone()),
                                    )
                                    .into_any_element()
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

    /// A pinned comparison is a snapshot, not the live transient dialog.
    /// Switching another tab cannot make this read a different Git revision
    /// or silently change the file contents behind the user's inspection.
    pub(super) fn render_git_file_compare_center_tab(
        &mut self,
        instance: u64,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let Some(tab) = self.git_compare_tabs.get(instance).cloned() else {
            return div()
                .flex_1()
                .min_h_0()
                .p_4()
                .text_color(rgb(MUTED))
                .child("Git comparison was closed")
                .into_any_element();
        };
        let generation = self.workspace_generation;
        let full = tab.displayed_full();
        let split = tab.split;
        let scope = format!("pinned-{}", instance);
        let content = if split {
            self.render_git_file_split_rows(
                if full {
                    tab.full_rows.clone()
                } else {
                    tab.diff_rows.clone()
                },
                full,
                true,
                scope.clone(),
                cx,
            )
        } else if full {
            self.render_git_file_full_unified_rows(tab.full_unified_rows.clone(), true, scope, cx)
        } else {
            div()
                .id("pinned-git-compare-unified-patch")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .bg(rgb(BG))
                .p_3()
                .font_family("Consolas")
                .text_sm()
                .text_color(rgb(TEXT))
                .child(tab.comparison.patch.clone())
                .into_any_element()
        };
        let locale = self.settings_menu_locale;
        let ref_name = &tab.comparison.reference;
        let reference = &tab.comparison.revision_oid;
        let details = format!(
            "{} · {} @ {} → worktree · +{} / -{} · {} · read-only{}",
            tab.comparison.relative.display(),
            ref_name,
            &reference[..reference.len().min(10)],
            tab.additions,
            tab.deletions,
            if full {
                "Full file"
            } else {
                "Differences only"
            },
            if tab.diff_clipped { " · clipped" } else { "" },
        );
        div()
            .id(format!("pinned-git-file-comparison-{instance}"))
            .flex_1()
            .min_h_0()
            .min_w_0()
            .w_full()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .child(
                div()
                    .h(px(42.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
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
                            .child(format!(
                                "{} · {}",
                                tr(locale, "Git file comparison"),
                                tab.title()
                            )),
                    )
                    .child(
                        div()
                            .id(format!("pinned-compare-content-toggle-{instance}"))
                            .px_2()
                            .py_1()
                            .bg(rgb(PANEL))
                            .border_1()
                            .border_color(rgb(BORDER))
                            .text_xs()
                            .text_color(rgb(if tab.full_available() { TEXT } else { MUTED }))
                            .child(tr(
                                locale,
                                if full {
                                    "Differences only"
                                } else {
                                    "Full file"
                                },
                            ))
                            .when(tab.full_available(), |button| {
                                button.cursor_pointer().on_click(cx.listener(
                                    move |view, _, _, cx| {
                                        if view.workspace_generation == generation {
                                            if let Some(tab) =
                                                view.git_compare_tabs.get_mut(instance)
                                            {
                                                tab.set_full(!full);
                                                cx.notify();
                                            }
                                        }
                                    },
                                ))
                            }),
                    )
                    .child(
                        div()
                            .id(format!("pinned-compare-layout-toggle-{instance}"))
                            .cursor_pointer()
                            .px_2()
                            .py_1()
                            .bg(rgb(PANEL))
                            .border_1()
                            .border_color(rgb(BORDER))
                            .text_xs()
                            .text_color(rgb(TEXT))
                            .child(tr(
                                locale,
                                if split {
                                    "Unified view"
                                } else {
                                    "Side-by-side view"
                                },
                            ))
                            .on_click(cx.listener(move |view, _, _, cx| {
                                if view.workspace_generation == generation {
                                    if let Some(tab) = view.git_compare_tabs.get_mut(instance) {
                                        tab.split = !tab.split;
                                        cx.notify();
                                    }
                                }
                            })),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .px_3()
                    .py_2()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(details),
            )
            .when(tab.full_error.is_some(), |view| {
                view.child(
                    div()
                        .flex_shrink_0()
                        .px_3()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(format!(
                            "Full file unavailable: {} · showing bounded diff only",
                            tab.full_error.as_deref().unwrap_or("unknown")
                        )),
                )
            })
            .child(content)
            .into_any_element()
    }

    /// Opening a pinned tab uses the exact successful dialog snapshot. It
    /// never re-runs Git, stage, checkout, save or edits a NativeEditor buffer.
    fn pin_git_file_comparison_snapshot(
        &mut self,
        generation: u64,
        ticket: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_generation != generation || self.git_file_compare_ticket != ticket {
            return;
        }
        let Some((_, Ok(diff))) = self
            .git_file_compare_result
            .as_ref()
            .filter(|(current, _)| *current == ticket)
        else {
            return;
        };
        let comparison = diff.clone();
        let id = match self.git_compare_tabs.open(comparison) {
            Ok(id) => id,
            Err(error) => {
                self.model.set_status(error);
                cx.notify();
                return;
            }
        };
        if let Some(tab) = self.git_compare_tabs.get_mut(id) {
            tab.split = self.git_file_compare_split;
            tab.set_full(self.git_file_compare_full);
        }
        let identity = WorkbenchTabIdentity::GitFileCompare(id);
        self.active_git_compare_tab = Some(id);
        self.git_graph_center_open = false;
        self.git_original_active = false;
        self.explorer_preview_active = false;
        self.center_git_diff_source = None;
        self.center_git_diff_hunk = None;
        self.sync_mixed_tabs(cx);
        self.mixed_tabs.select(&identity);
        self.mixed_tabs.set_pinned(&identity, true);
        self.mixed_panes.activate(&identity);
        self.model
            .set_status("Pinned an immutable Git comparison in the workbench; no files changed");
        self.save_mixed_pane_layout(cx);
        window.close_all_dialogs(cx);
        cx.notify();
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
        let full_available = valid && self.git_file_compare_full_error.is_none();
        let full = self.git_file_compare_full && full_available;
        let full_error = self.git_file_compare_full_error.clone();
        let locale = self.settings_menu_locale;
        let displayed = if valid && split {
            self.render_git_file_split_table(full, cx)
        } else if valid && full {
            self.render_git_file_full_unified(cx)
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
                                .id("git-file-compare-pin-to-workbench")
                                .cursor_pointer()
                                .px_2()
                                .py_1()
                                .bg(rgb(TOOLBAR))
                                .border_1()
                                .border_color(rgb(BORDER))
                                .text_xs()
                                .text_color(rgb(TEXT))
                                .child(tr(locale, "Pin comparison in workbench"))
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    view.pin_git_file_comparison_snapshot(
                                        generation, ticket, window, cx,
                                    );
                                })),
                        )
                    })
                    .when(valid, |bar| {
                        bar.child(
                            div()
                                .id("git-file-compare-content-toggle")
                                .px_2()
                                .py_1()
                                .bg(rgb(TOOLBAR))
                                .border_1()
                                .border_color(rgb(BORDER))
                                .text_xs()
                                .text_color(rgb(if full_available { TEXT } else { MUTED }))
                                .child(tr(
                                    locale,
                                    if full {
                                        "Differences only"
                                    } else {
                                        "Full file"
                                    },
                                ))
                                .when(full_available, |toggle| {
                                    toggle.cursor_pointer().on_click(cx.listener(
                                        move |view, _, _, cx| {
                                            if view.workspace_generation == generation
                                                && view.git_file_compare_ticket == ticket
                                            {
                                                view.git_file_compare_full =
                                                    !view.git_file_compare_full;
                                                cx.notify();
                                            }
                                        },
                                    ))
                                }),
                        )
                    })
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
                                .child(tr(
                                    locale,
                                    if split {
                                        "Unified view"
                                    } else {
                                        "Side-by-side view"
                                    },
                                ))
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
            .child(div().text_xs().text_color(rgb(MUTED)).child(format!(
                "{detail} · {} · no Git writes",
                if full {
                    "Complete file"
                } else {
                    "Changed sections only"
                }
            )))
            .when(valid && !full_available, |panel| {
                panel.child(div().text_xs().text_color(rgb(MUTED)).child(format!(
                    "Full file unavailable: {}. Differences preview remains available.",
                    full_error.unwrap_or_else(|| "Source unavailable".to_owned())
                )))
            })
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
        self.git_file_compare_full = true;
        self.git_file_compare_rows = Arc::new(Vec::new());
        self.git_file_compare_full_rows = Arc::new(Vec::new());
        self.git_file_compare_full_unified_rows = Arc::new(Vec::new());
        self.git_file_compare_full_error = None;
        self.git_file_compare_additions = 0;
        self.git_file_compare_deletions = 0;
        self.git_file_compare_clipped = false;
        self.model.set_status(format!(
            "Comparing {} with {label} (read-only)…",
            relative.display()
        ));
        let owner = cx.entity().downgrade();
        let dialog_path = relative.display().to_string();
        let locale = self.settings_menu_locale;
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
                .title(tr(locale, "Git file comparison"))
                .w(px(1050.))
                .centered()
                .on_cancel(|_, _, _| true)
                .on_ok(|_, _, _| true)
                .child(content)
        });
        cx.spawn(async move |view, cx| {
            let (result, aligned, full_flat) = cx
                .background_spawn(async move {
                    let result =
                        compare_file_with_git_reference(&root, &relative, branch.as_deref());
                    let aligned = result
                        .as_ref()
                        .ok()
                        .map(|diff| align_git_file_patch(&diff.patch));
                    let full_flat = result
                        .as_ref()
                        .ok()
                        .and_then(|diff| diff.full_file.as_ref().ok())
                        .map(flatten_full_file_rows);
                    (result, aligned, full_flat)
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
                if let Ok(diff) = &result {
                    match &diff.full_file {
                        Ok(full) => {
                            view.git_file_compare_full_rows = Arc::new(full.rows.clone());
                            view.git_file_compare_full_unified_rows =
                                Arc::new(full_flat.unwrap_or_default());
                        }
                        Err(error) => {
                            view.git_file_compare_full_error = Some(error.clone());
                            view.git_file_compare_full = false;
                        }
                    }
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
