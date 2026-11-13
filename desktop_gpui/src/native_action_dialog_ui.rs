//! Explorer create/rename/delete prompts use the same reusable Native modal
//! component as Settings. Destructive delete remains a deliberately explicit
//! TWO-click confirmation, never a silent file-system operation.

use super::*;
use native_components::action_button;

/// The expanded viewport only displays an already-fetched read-only patch.
pub(super) fn can_expand_read_only_diff(patch: Option<&str>, loading: bool) -> bool {
    !loading && patch.is_some_and(|text| !text.trim().is_empty())
}

const MAX_COLORED_PATCH_LINES: usize = 400;
const MAX_HUNK_JUMP_BUTTONS: usize = 12;

/// Keep the expanded preview entirely inside the display, including the
/// dialog title and its read-only explanation. Never use a fixed desktop-only
/// width when GPUI runs at 800x600 or with high display scaling.
fn expanded_git_diff_size(viewport_width: f32, viewport_height: f32) -> (f32, f32) {
    let width = (viewport_width * 0.90)
        .clamp(280.0, 1080.0)
        .min((viewport_width - 32.0).max(240.0));
    let scroll_height = (viewport_height * 0.65)
        .clamp(180.0, 680.0)
        .min((viewport_height - 160.0).max(140.0));
    (width, scroll_height)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct GitPatchSummary {
    files: usize,
    hunks: usize,
    added: usize,
    removed: usize,
}

/// Exclude unified-diff +++/--- file markers from line counts. These totals
/// describe the captured bounded patch, not necessarily the entire file.
fn summarize_git_patch(patch: &str) -> GitPatchSummary {
    let mut summary = GitPatchSummary::default();
    for line in patch.lines() {
        if line.starts_with("diff --git ") {
            summary.files += 1;
        } else if line.starts_with("@@ ") || line.starts_with("@@@ ") {
            summary.hunks += 1;
        } else if line.starts_with('+') && !line.starts_with("+++") {
            summary.added += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            summary.removed += 1;
        }
    }
    summary
}

pub(super) fn git_patch_line_color(line: &str) -> u32 {
    if line.starts_with("@@ ") || line.starts_with("@@@ ") {
        0x83b7ed
    } else if line.starts_with('+') && !line.starts_with("+++") {
        0x84c59a
    } else if line.starts_with('-') && !line.starts_with("---") {
        0xeb9a9a
    } else if line.starts_with("diff --git ")
        || line.starts_with("+++")
        || line.starts_with("---")
        || line.starts_with("… diff preview truncated")
    {
        0xe9bd80
    } else {
        TEXT
    }
}

/// Git's old/new coordinates for a visible unified-diff record. A bounded
/// page may begin inside a hunk: until its first valid @@ header, leave both
/// coordinates blank rather than inventing the previous page's line numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GitDiffDisplayRow<'a> {
    pub(super) text: &'a str,
    pub(super) old: Option<usize>,
    pub(super) new: Option<usize>,
    pub(super) hunk: Option<(usize, usize)>,
}

fn parse_unified_hunk_header(line: &str) -> Option<(usize, usize)> {
    let rest = line.strip_prefix("@@ ")?;
    let (old, rest) = rest.split_once(' ')?;
    let (new, tail) = rest.split_once(' ')?;
    if !tail.starts_with("@@") {
        return None;
    }
    let parse_start = |token: &str, sign: char| {
        token
            .strip_prefix(sign)?
            .split(',')
            .next()?
            .parse::<usize>()
            .ok()
    };
    Some((parse_start(old, '-')?, parse_start(new, '+')?))
}

pub(super) fn parse_git_diff_rows(patch: &str, limit: usize) -> Vec<GitDiffDisplayRow<'_>> {
    let mut old = None;
    let mut new = None;
    patch
        .lines()
        .take(limit)
        .map(|line| {
            if let Some((from, to)) = parse_unified_hunk_header(line) {
                old = Some(from);
                new = Some(to);
                return GitDiffDisplayRow {
                    text: line,
                    old: None,
                    new: None,
                    hunk: Some((from, to)),
                };
            }
            let mut row = GitDiffDisplayRow {
                text: line,
                old: None,
                new: None,
                hunk: None,
            };
            if line.starts_with("diff --git ")
                || line.starts_with("@@@ ")
                || line.starts_with("index ")
            {
                // Combined diffs need their own multi-parent line model.
                old = None;
                new = None;
            } else if line.starts_with("---") || line.starts_with("+++") {
                // Git file markers are metadata, not deleted/added lines.
            } else if line.starts_with(' ') {
                row.old = old;
                row.new = new;
                old = old.and_then(|line| line.checked_add(1));
                new = new.and_then(|line| line.checked_add(1));
            } else if line.starts_with('-') {
                row.old = old;
                old = old.and_then(|line| line.checked_add(1));
            } else if line.starts_with('+') {
                row.new = new;
                new = new.and_then(|line| line.checked_add(1));
            }
            row
        })
        .collect()
}

fn visible_git_diff_rows(patch: &str) -> Vec<GitDiffDisplayRow<'_>> {
    parse_git_diff_rows(patch, MAX_COLORED_PATCH_LINES)
}

fn visible_git_hunk_targets(rows: &[GitDiffDisplayRow<'_>]) -> Vec<(usize, usize, usize)> {
    rows.iter()
        .enumerate()
        .filter_map(|(row, entry)| entry.hunk.map(|(old, new)| (row, old, new)))
        .take(MAX_HUNK_JUMP_BUTTONS)
        .collect()
}

/// A dialog can navigate one selected file only. A new file selection gets a
/// new source ticket, even when the path happens to be the same spelling.
/// Workspace switching also invalidates the dialog's file source.
#[derive(Clone)]
pub(super) struct GitDialogFileSource {
    pub(super) workspace_generation: u64,
    pub(super) source_ticket: u64,
    pub(super) relative: PathBuf,
    pub(super) area: Option<GitChangeArea>,
}

pub(super) fn git_dialog_file_source_matches(
    current_generation: u64,
    current: Option<&GitFileDiffPaging>,
    expected: &GitDialogFileSource,
) -> bool {
    current_generation == expected.workspace_generation
        && current.is_some_and(|paging| {
            paging.source_ticket == expected.source_ticket
                && paging.relative == expected.relative
                && paging.area == expected.area
        })
}

impl Workbench {
    /// Show a read-only Git patch. File patches follow the selected page with
    /// guarded in-dialog navigation; history/compare patches stay immutable.
    pub(super) fn open_kit_git_diff_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !can_expand_read_only_diff(self.git_diff.as_deref(), self.git_diff_loading) {
            self.model
                .set_status("Select a Git change and finish loading its patch first");
            cx.notify();
            return;
        }
        let Some(patch) = self.git_diff.clone() else {
            return;
        };
        let source = self
            .git_file_diff_paging
            .as_ref()
            .map(|paging| GitDialogFileSource {
                workspace_generation: self.workspace_generation,
                source_ticket: paging.source_ticket,
                relative: paging.relative.clone(),
                area: paging.area,
            });
        let owner = cx.entity().downgrade();
        let patch_scroll = ScrollHandle::new();
        window.open_dialog(cx, move |dialog, window, cx| {
            let (visible_patch, page_controls) = if let Some(source) = source.as_ref() {
                owner
                    .upgrade()
                    .and_then(|entity| {
                        entity.update(cx, |view, _| {
                            if !git_dialog_file_source_matches(
                                view.workspace_generation,
                                view.git_file_diff_paging.as_ref(),
                                source,
                            ) {
                                return None;
                            }
                            let paging = view.git_file_diff_paging.as_ref()?;
                            let page = paging.previous_starts.len() + 1;
                            let can_previous =
                                !view.git_diff_loading && !paging.previous_starts.is_empty();
                            let can_next = !view.git_diff_loading
                                && paging.next_line.is_some()
                                && paging.previous_starts.len() < 31;
                            let content = if view.git_diff_loading {
                                "Loading read-only Git patch page…".to_owned()
                            } else {
                                view.git_diff
                                    .clone()
                                    .unwrap_or_else(|| "Git patch page is unavailable".to_owned())
                            };
                            Some((content, (page, can_previous, can_next)))
                        })
                    })
                    .map(|(content, controls)| (content, Some(controls)))
                    .unwrap_or_else(|| {
                        (
                            "The selected Git file or Workspace changed. Close and reopen Diff."
                                .to_owned(),
                            None,
                        )
                    })
            } else {
                (patch.clone(), None)
            };
            let viewport = window.viewport_size();
            let (dialog_width, scroll_height) =
                expanded_git_diff_size(f32::from(viewport.width), f32::from(viewport.height));
            let summary = summarize_git_patch(&visible_patch);
            let visible_rows = visible_git_diff_rows(&visible_patch);
            let hunk_targets = visible_git_hunk_targets(&visible_rows);
            let colored_lines = visible_rows
                .iter()
                .map(|row| {
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .w(px(54.0))
                                .flex()
                                .justify_end()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child(row.old.map_or_else(String::new, |line| line.to_string())),
                        )
                        .child(
                            div()
                                .w(px(54.0))
                                .flex()
                                .justify_end()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child(row.new.map_or_else(String::new, |line| line.to_string())),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .whitespace_nowrap()
                                .text_color(rgb(git_patch_line_color(row.text)))
                                .child(row.text.to_owned()),
                        )
                })
                .collect::<Vec<_>>();
            let plain_remainder = visible_patch
                .lines()
                .skip(MAX_COLORED_PATCH_LINES)
                .collect::<Vec<_>>()
                .join("\n");
            let has_plain_remainder = !plain_remainder.is_empty();
            let preview_kind = if source.is_some() {
                "Read-only current page"
            } else {
                "Read-only snapshot"
            };
            dialog
                .title("Git patch · expanded read-only preview")
                .w(px(dialog_width))
                .centered()
                .close_button(true)
                .on_cancel(|_, _, _| true)
                .on_ok(|_, _, _| true)
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child(format!(
                                    "{preview_kind} · {} file(s) · {} hunk(s) · +{} / -{} · up to 32 KiB",
                                    summary.files, summary.hunks, summary.added, summary.removed
                                )),
                        )
                        .children(page_controls.map(|(page, can_previous, can_next)| {
                            let previous_owner = owner.clone();
                            let previous_source = source.clone().expect("file page source");
                            let next_owner = owner.clone();
                            let next_source = source.clone().expect("file page source");
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_2()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(MUTED))
                                        .child(format!("Page {page} · live Git, not a frozen cross-page snapshot")),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_3()
                                        .child(if can_previous {
                                            div()
                                                .id("dialog-git-diff-previous-page")
                                                .cursor_pointer()
                                                .text_sm()
                                                .text_color(rgb(TEXT))
                                                .child("Previous")
                                                .on_click(move |_, _, cx| {
                                                    let _ = previous_owner.update(cx, |view, cx| {
                                                        if git_dialog_file_source_matches(
                                                            view.workspace_generation,
                                                            view.git_file_diff_paging.as_ref(),
                                                            &previous_source,
                                                        ) {
                                                            view.change_git_file_diff_page(false, cx);
                                                        }
                                                    });
                                                })
                                                .into_any_element()
                                        } else {
                                            div().text_sm().text_color(rgb(MUTED))
                                                .child("Previous").into_any_element()
                                        })
                                        .child(if can_next {
                                            div()
                                                .id("dialog-git-diff-next-page")
                                                .cursor_pointer()
                                                .text_sm()
                                                .text_color(rgb(TEXT))
                                                .child("Next")
                                                .on_click(move |_, _, cx| {
                                                    let _ = next_owner.update(cx, |view, cx| {
                                                        if git_dialog_file_source_matches(
                                                            view.workspace_generation,
                                                            view.git_file_diff_paging.as_ref(),
                                                            &next_source,
                                                        ) {
                                                            view.change_git_file_diff_page(true, cx);
                                                        }
                                                    });
                                                })
                                                .into_any_element()
                                        } else {
                                            div().text_sm().text_color(rgb(MUTED))
                                                .child("Next").into_any_element()
                                        }),
                                )
                        }))
                        .children((!hunk_targets.is_empty()).then(|| {
                            div()
                                .w_full()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(MUTED))
                                        .child(format!(
                                            "Jump to hunk · {} of {} in this page (first {} lines)",
                                            hunk_targets.len(),
                                            summary.hunks,
                                            MAX_COLORED_PATCH_LINES
                                        )),
                                )
                                .child(
                                    div()
                                        .id("git-diff-hunk-toolbar")
                                        .w_full()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .max_h(px(36.0))
                                        .overflow_y_scroll()
                                        .overflow_x_scroll()
                                        .children(hunk_targets.into_iter().map(|(row, old, new)| {
                                            let jump_scroll = patch_scroll.clone();
                                            let notify_owner = owner.clone();
                                            div()
                                                .id(("git-diff-hunk-jump", row))
                                                .cursor_pointer()
                                                .text_xs()
                                                .text_color(rgb(0x83b7ed))
                                                .child(format!("-{old} / +{new}"))
                                                .on_click(move |_, _, cx| {
                                                    jump_scroll.scroll_to_item(row);
                                                    let _ = notify_owner
                                                        .update(cx, |_, cx| cx.notify());
                                                })
                                        })),
                                )
                        }))
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child("Old / New line numbers follow visible @@ headers only. A continuation page without a header does not guess line positions."),
                        )
                        .children(has_plain_remainder.then(|| {
                            div()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child("Color highlighting is limited to the first 400 lines; remaining patch text is still displayed.")
                        }))
                        .child(
                            div()
                                .id(("expanded-git-diff-scroll", page_controls.map_or(0, |value| value.0)))
                                .track_scroll(&patch_scroll)
                                .w_full()
                                .max_h(px(scroll_height))
                                .overflow_y_scroll()
                                .overflow_x_scroll()
                                .flex()
                                .flex_col()
                                .font_family("Consolas")
                                .text_sm()
                                .whitespace_nowrap()
                                .children(colored_lines)
                                .children(has_plain_remainder.then(|| {
                                    div()
                                        .mt_2()
                                        .pl(px(116.0))
                                        .whitespace_nowrap()
                                        .text_color(rgb(MUTED))
                                        .child(plain_remainder)
                                })),
                        ),
                )
        });
    }

    pub(super) fn open_kit_quick_open_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(input) = self.quick_open.clone() else {
            return;
        };
        let owner = cx.entity().downgrade();
        let first_focus = std::cell::Cell::new(true);
        window.open_dialog(cx, move |dialog, window, cx| {
            if first_focus.replace(false) {
                window.focus(&input.read(cx).focus_handle(cx), cx);
            }
            let content = owner
                .upgrade()
                .and_then(|entity| entity.update(cx, |view, cx| view.render_quick_open_dialog(cx)))
                .unwrap_or_else(|| div().into_any_element());
            let for_cancel = owner.clone();
            dialog
                .title("Open file")
                .w(px(730.))
                .centered()
                .close_button(false)
                .on_cancel(move |_, _, cx| {
                    let _ = for_cancel.update(cx, |view, cx| {
                        view.quick_open = None;
                        cx.notify();
                    });
                    true
                })
                // Enter is handled by the real QuickOpen result selection.
                // Do not silently dismiss the dialog with the default OK.
                .on_ok(|_, _, _| false)
                .child(content)
        });
    }

    pub(super) fn open_kit_explorer_action_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(action) = self.explorer_action else {
            return;
        };
        let owner = cx.entity().downgrade();
        let first_focus = std::cell::Cell::new(true);
        let input = self.explorer_action_input.clone();
        let fallback_focus = self.explorer_action_modal_focus.clone();
        window.open_dialog(cx, move |dialog, window, cx| {
            if first_focus.replace(false) {
                if let Some(input) = input.as_ref() {
                    window.focus(&input.read(cx).focus_handle(cx), cx);
                } else {
                    window.focus(&fallback_focus, cx);
                }
            }
            let content = owner
                .upgrade()
                .and_then(|entity| {
                    entity.update(cx, |view, cx| view.render_explorer_action_dialog(cx))
                })
                .unwrap_or_else(|| div().into_any_element());
            let for_cancel = owner.clone();
            let for_submit = owner.clone();
            dialog
                .title(if action == ExplorerAction::Delete {
                    "Delete permanently"
                } else {
                    action.label()
                })
                .w(px(520.))
                .centered()
                .close_button(false)
                .on_cancel(move |_, _, cx| {
                    let _ = for_cancel.update(cx, |view, cx| view.cancel_explorer_action(cx));
                    true
                })
                .on_ok(move |_, _, cx| {
                    for_submit
                        .update(cx, |view, cx| {
                            view.apply_explorer_action(cx);
                            view.explorer_action.is_none()
                        })
                        .unwrap_or(true)
                })
                .child(content)
        });
    }

    pub(super) fn render_quick_open_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let input = self.quick_open.clone()?;
        let input_for_enter = input.clone();
        if self.settings_section.is_some() || self.explorer_action.is_some() {
            return None;
        }
        let excluded = self
            .settings_extra_values
            .get("editor.quickOpenExcludedDirectories")
            .and_then(serde_json::Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_owned))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| {
                alera_core::desktop_settings_contract::QUICK_OPEN_DEFAULT_EXCLUDED_DIRECTORIES
                    .iter()
                    .map(|name| (*name).to_owned())
                    .collect()
            });
        let matches = self
            .file_index
            .as_ref()
            .map(|index| index.search_with_exclusions(input.read(cx).query(), 18, &excluded))
            .unwrap_or_default();
        let count = matches.len();
        let selected_index = self.quick_open_selected_index.min(count.saturating_sub(1));
        let selected_match = matches.get(selected_index).cloned();
        let total = self
            .file_index
            .as_ref()
            .map(|index| index.total())
            .unwrap_or_default();
        let indexed = if self.file_index_loading {
            "Indexing workspace files…".to_owned()
        } else {
            format!("{total} files scanned · {count} matches (Quick Open exclusions applied)")
        };
        let workspace = self.model.root().to_path_buf();
        let content = div()
            .id("native-quick-open-content")
            .flex_1()
            .flex()
            .flex_col()
            .min_h_0()
            .p_3()
            .gap_2()
            .child(div().child(input))
            .child(div().text_xs().text_color(rgb(MUTED)).child(indexed))
            .child(
                div()
                    .id("native-quick-open-results")
                    .track_scroll(&self.quick_open_results_scroll)
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(matches.into_iter().enumerate().map(|(index, path)| {
                        let label = path
                            .strip_prefix(&workspace)
                            .unwrap_or(&path)
                            .display()
                            .to_string();
                        native_components::menu_item(
                            ("native-quick-open-result", index),
                            label,
                            index == selected_index,
                            false,
                        )
                        .on_click(cx.listener(
                            move |view, _, window, cx| {
                                view.choose_quick_open_path(&path, window, cx);
                            },
                        ))
                    })),
            );
        let content = content.on_key_down(cx.listener(
            move |view, event: &gpui::KeyDownEvent, window, cx| {
                let key = event.keystroke.key.as_str();
                let composing = input_for_enter.read(cx).composing();
                if let Some(next) = crate::quick_open_ui::quick_open_move_selection(
                    key,
                    composing,
                    view.quick_open_selected_index,
                    count,
                ) {
                    view.quick_open_selected_index = next;
                    view.quick_open_results_scroll.scroll_to_item(next);
                    cx.notify();
                    cx.stop_propagation();
                    return;
                }
                if crate::quick_open_ui::quick_open_enter_accepts_match(
                    key,
                    composing,
                    selected_match.is_some(),
                ) {
                    if let Some(path) = &selected_match {
                        view.choose_quick_open_path(path, window, cx);
                    }
                    cx.stop_propagation();
                }
            },
        ));
        Some(content.into_any_element())
    }

    fn choose_quick_open_path(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        if self.model.open_path(path) {
            self.open_editor(cx);
            let opened_correct_file = self.focused_editor().is_some_and(|editor| {
                crate::quick_open_ui::quick_open_target_is_active(
                    path,
                    editor.read(cx).document().path(),
                )
            });
            if !opened_correct_file {
                // Keep the dialog open. A failed Native Editor open must not
                // pretend that the requested file was actually activated.
                self.model
                    .set_status("Quick Open could not activate this file; check the editor status");
                cx.notify();
                return;
            }
            self.quick_open = None;
            self.persist_session();
            window.close_dialog(cx);
            if let Some(editor) = self.focused_editor() {
                window.focus(&editor.read(cx).focus_handle(cx), cx);
            }
        } else {
            self.model
                .set_status("Cannot open this file; refresh the workspace index");
        }
        cx.notify();
    }

    pub(super) fn close_quick_open_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.quick_open = None;
        window.close_dialog(cx);
        if let Some(editor) = self.focused_editor() {
            window.focus(&editor.read(cx).focus_handle(cx), cx);
        } else {
            window.focus(&self.explorer_keyboard_focus, cx);
        }
        cx.notify();
    }

    pub(super) fn render_explorer_action_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let action = self.explorer_action?;
        if self.settings_section.is_some() {
            return None;
        }
        let destructive = action == ExplorerAction::Delete;
        let description = if destructive {
            "The selected files and folders (including contents) will be permanently removed, not moved to Recycle Bin. Review the selection before confirming."
        } else if action == ExplorerAction::Duplicate {
            "Create a copy of the selected item in its current folder."
        } else {
            "Enter the name or destination within the current workspace."
        };
        let delete_count = self
            .explorer_delete_pending
            .as_ref()
            .map(ExplorerDeleteBatch::len)
            .unwrap_or(0);
        let button_text = if self.explorer_delete_confirmed {
            format!("Confirm permanent deletion ({delete_count})")
        } else if destructive {
            "Delete permanently…".to_owned()
        } else {
            action.label().to_owned()
        };
        let input = self.explorer_action_input.clone();
        let body = div()
            .id("explorer-action-modal-body")
            .p_4()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(if destructive { 0xf4b0b0 } else { MUTED }))
                    .child(description),
            )
            .children(input.map(|input| div().child(input)))
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .justify_end()
                    .items_center()
                    .gap_2()
                    .child(
                        action_button("explorer-action-cancel", "Cancel", false, false).on_click(
                            cx.listener(|view, _, window, cx| {
                                view.cancel_explorer_action(cx);
                                window.close_dialog(cx);
                            }),
                        ),
                    )
                    .child(
                        action_button("explorer-action-apply", button_text, true, destructive)
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.apply_explorer_action(cx);
                                if view.explorer_action.is_none() {
                                    window.close_dialog(cx);
                                }
                            })),
                    ),
            );
        // GPUI Kit Dialog owns the backdrop, key focus, Escape and viewport
        // clamping. The Workbench only renders the guarded action content.
        Some(body.into_any_element())
    }
}

#[cfg(test)]
mod git_preview_tests {
    use super::{
        can_expand_read_only_diff, expanded_git_diff_size, git_dialog_file_source_matches,
        git_patch_line_color, parse_unified_hunk_header, summarize_git_patch,
        visible_git_diff_rows, visible_git_hunk_targets, GitDialogFileSource, GitFileDiffPaging,
    };
    use alera_gpui_desktop::git_inspect::GitChangeArea;

    #[test]
    fn expanded_git_preview_requires_loaded_nonempty_snapshot() {
        assert!(!can_expand_read_only_diff(None, false));
        assert!(!can_expand_read_only_diff(Some(""), false));
        assert!(!can_expand_read_only_diff(Some(" \n\t"), false));
        assert!(!can_expand_read_only_diff(Some("diff --git a/x b/x"), true));
        assert!(can_expand_read_only_diff(
            Some("diff --git a/x b/x\n+updated\n"),
            false
        ));
    }

    #[test]
    fn expanded_git_diff_respects_small_and_large_display_bounds() {
        let (small_width, small_scroll) = expanded_git_diff_size(800.0, 600.0);
        assert!((320.0..=768.0).contains(&small_width));
        assert!((180.0..=440.0).contains(&small_scroll));
        let (large_width, large_scroll) = expanded_git_diff_size(3840.0, 2160.0);
        assert_eq!(large_width, 1080.0);
        assert_eq!(large_scroll, 680.0);
    }

    #[test]
    fn diff_counts_exclude_file_markers_and_color_rows_by_hunk_and_change() {
        let patch = concat!(
            "diff --git a/one.rs b/one.rs\n",
            "--- a/one.rs\n",
            "+++ b/one.rs\n",
            "@@ -1,2 +1,3 @@\n",
            " unchanged\n",
            "-old\n",
            "+new\n",
            "+extra\n",
            "\u{2026} diff preview truncated\n"
        );
        let counts = summarize_git_patch(patch);
        assert_eq!(
            (counts.files, counts.hunks, counts.added, counts.removed),
            (1, 1, 2, 1)
        );
        assert_ne!(git_patch_line_color("+new"), git_patch_line_color("-old"));
        assert_ne!(
            git_patch_line_color("@@ -1 +1 @@"),
            git_patch_line_color(" unchanged")
        );
        assert_ne!(
            git_patch_line_color("+++ b/one.rs"),
            git_patch_line_color("+new")
        );
    }

    #[test]
    fn expanded_dialog_only_navigates_its_original_file_selection() {
        let expected = GitDialogFileSource {
            workspace_generation: 42,
            source_ticket: 7,
            relative: "src/dirty.rs".into(),
            area: Some(GitChangeArea::Changes),
        };
        let mut paging = GitFileDiffPaging {
            relative: "src/dirty.rs".into(),
            area: Some(GitChangeArea::Changes),
            source_ticket: 7,
            start_line: 0,
            next_line: Some(750),
            previous_starts: Vec::new(),
        };
        assert!(git_dialog_file_source_matches(42, Some(&paging), &expected));

        // Paging advances the request ticket but must keep the file's source
        // identity until the user selects another preview.
        paging.previous_starts.push(0);
        paging.start_line = 750;
        paging.next_line = Some(1_500);
        assert!(git_dialog_file_source_matches(42, Some(&paging), &expected));

        assert!(!git_dialog_file_source_matches(
            43,
            Some(&paging),
            &expected
        ));
        assert!(!git_dialog_file_source_matches(42, None, &expected));
        paging.relative = "src/other.rs".into();
        assert!(!git_dialog_file_source_matches(
            42,
            Some(&paging),
            &expected
        ));
        paging.relative = expected.relative.clone();
        paging.source_ticket += 1;
        // Re-selecting the same filename cannot let an old dialog advance it.
        assert!(!git_dialog_file_source_matches(
            42,
            Some(&paging),
            &expected
        ));
    }

    #[test]
    fn g02_diff_source_refuses_same_file_with_different_staged_area() {
        let expected = GitDialogFileSource {
            workspace_generation: 5,
            source_ticket: 9,
            relative: "same.txt".into(),
            area: Some(GitChangeArea::Staged),
        };
        let mut paging = GitFileDiffPaging {
            relative: "same.txt".into(),
            area: Some(GitChangeArea::Staged),
            source_ticket: 9,
            start_line: 0,
            next_line: None,
            previous_starts: Vec::new(),
        };
        assert!(git_dialog_file_source_matches(5, Some(&paging), &expected));
        paging.area = Some(GitChangeArea::Changes);
        assert!(!git_dialog_file_source_matches(5, Some(&paging), &expected));
    }
    #[test]
    fn git_diff_gutter_tracks_old_and_new_line_numbers_through_multiple_hunks() {
        let patch = concat!(
            "diff --git a/main.rs b/main.rs\n",
            "--- a/main.rs\n",
            "+++ b/main.rs\n",
            "@@ -10,3 +20,4 @@ fn run()\n",
            " context\n",
            "-removed\n",
            "+added\n",
            "+another\n",
            " context after\n",
            "\\ No newline at end of file\n",
            "@@ -100,1 +111,2 @@\n",
            "-previous\n",
            "+replaced\n",
            "+additional\n"
        );
        let rows = visible_git_diff_rows(patch);
        let coords = rows
            .iter()
            .map(|row| (row.old, row.new))
            .collect::<Vec<_>>();
        assert_eq!(coords[1], (None, None));
        assert_eq!(coords[2], (None, None));
        assert_eq!(rows[3].hunk, Some((10, 20)));
        assert_eq!(coords[4], (Some(10), Some(20)));
        assert_eq!(coords[5], (Some(11), None));
        assert_eq!(coords[6], (None, Some(21)));
        assert_eq!(coords[7], (None, Some(22)));
        assert_eq!(coords[8], (Some(12), Some(23)));
        assert_eq!(coords[9], (None, None));
        assert_eq!(rows[10].hunk, Some((100, 111)));
        assert_eq!(coords[11], (Some(100), None));
        assert_eq!(coords[12], (None, Some(111)));
        assert_eq!(coords[13], (None, Some(112)));
    }

    #[test]
    fn git_diff_gutter_does_not_invent_positions_without_hunk_or_for_combined_diff() {
        let continuation = visible_git_diff_rows(
            " context from prior page\n-removed\n+added\n@@ -0,0 +1,1 @@\n+created\n",
        );
        assert!(continuation[..3]
            .iter()
            .all(|row| row.old.is_none() && row.new.is_none()));
        assert_eq!((continuation[4].old, continuation[4].new), (None, Some(1)));
        let combined = visible_git_diff_rows("@@@ -1,3 -2,3 +1,4 @@@\n-removed\n+added\n");
        assert!(combined
            .iter()
            .all(|row| row.old.is_none() && row.new.is_none()));
        assert_eq!(
            parse_unified_hunk_header("@@ -24 +31 @@ context"),
            Some((24, 31))
        );
        assert_eq!(parse_unified_hunk_header("@@@ -1 -2 +3 @@@"), None);
        assert_eq!(parse_unified_hunk_header("@@ broken +31 @@"), None);
    }

    #[test]
    fn git_hunk_jumps_target_direct_visible_rows_and_limit_button_count() {
        let long_patch = (0..450)
            .map(|row| {
                if row % 20 == 0 {
                    format!("@@ -{} +{} @@\n", row + 1, row + 1)
                } else {
                    " context\n".to_string()
                }
            })
            .collect::<String>();
        let rows = visible_git_diff_rows(&long_patch);
        assert_eq!(rows.len(), 400);
        let targets = visible_git_hunk_targets(&rows);
        assert_eq!(targets.len(), 12);
        assert_eq!(targets.first().copied(), Some((0, 1, 1)));
        assert_eq!(targets.last().copied(), Some((220, 221, 221)));
        assert!(targets.iter().all(|(row, _, _)| *row < rows.len()));
        // The scroll list has one direct child per visible row, so the
        // browser-independent target is the row index, not a pixel estimate.
        assert_eq!(targets[5].0, 100);
        assert!(summarize_git_patch(&long_patch).hunks > targets.len());
    }

    #[test]
    fn git_line_gutter_does_not_leak_line_numbers_across_diff_files() {
        let rows = visible_git_diff_rows(concat!(
            "@@ -2 +9 @@\n",
            " before\n",
            "diff --git a/next.rs b/next.rs\n",
            "+no header yet\n",
            "@@ -10 +12 @@\n",
            "+new file line\n"
        ));
        assert_eq!((rows[1].old, rows[1].new), (Some(2), Some(9)));
        assert_eq!((rows[3].old, rows[3].new), (None, None));
        assert_eq!((rows[5].old, rows[5].new), (None, Some(12)));
    }
}
