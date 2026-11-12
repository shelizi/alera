//! Bounded, read-only Git file-diff viewport in the primary center pane.
//! It never owns Editor documents, Terminal sessions, or Git write actions.
//! The central viewer is a temporary projection of one selected Git file page.

use super::*;
use native_action_dialog_ui::{
    git_dialog_file_source_matches, git_patch_line_color, parse_git_diff_rows, GitDialogFileSource,
};

const MAX_CENTER_DIFF_SOURCE_BYTES: usize = 32 * 1024 + 128;
const MAX_CENTER_DISPLAY_LINE_CHARS: usize = 4096;

#[derive(Clone)]
pub(super) struct CenterGitDiffRow {
    text: String,
    old: Option<usize>,
    new: Option<usize>,
    hunk: Option<(usize, usize)>,
}

pub(super) fn center_git_diff_rows(patch: &str) -> Vec<CenterGitDiffRow> {
    if patch.len() > MAX_CENTER_DIFF_SOURCE_BYTES {
        return Vec::new();
    }
    // A bounded page can have up to 32k one-byte lines. UniformList paints
    // only the visible range, rather than creating 32k GPUI Div elements.
    parse_git_diff_rows(patch, MAX_CENTER_DIFF_SOURCE_BYTES)
        .into_iter()
        .map(|row| {
            let mut text = row
                .text
                .chars()
                .take(MAX_CENTER_DISPLAY_LINE_CHARS)
                .collect::<String>();
            if row.text.chars().count() > MAX_CENTER_DISPLAY_LINE_CHARS {
                text.push_str(" … row display clipped; expand Diff for original");
            }
            CenterGitDiffRow {
                text,
                old: row.old,
                new: row.new,
                hunk: row.hunk,
            }
        })
        .collect()
}

fn hunk_positions(rows: &[CenterGitDiffRow]) -> Vec<usize> {
    rows.iter()
        .enumerate()
        .filter_map(|(index, row)| row.hunk.map(|_| index))
        .collect()
}

/// Source freshness and selected Git-original tabs take precedence over the
/// temporary center diff surface. Opening Center may switch the active tab,
/// but must not mutate any original snapshot or editor document.
/// The paged central preview is authoritative for working/untracked changes.
/// A staged or conflicted row must keep its exact Git area instead of
/// silently showing a different (e.g. working-tree) patch.
pub(super) fn uses_paged_git_preview(area: Option<GitChangeArea>, original: bool) -> bool {
    !original
        && matches!(
            area,
            None | Some(GitChangeArea::Changes) | Some(GitChangeArea::Untracked)
        )
}

pub(super) fn should_show_center_git_diff_view(
    git_original_active: bool,
    source_current: bool,
) -> bool {
    source_current && !git_original_active
}

/// Keep both Terminal branches (live PTY and empty/offline fallback) out of
/// the center pane while an Editor or temporary read-only Git Diff owns it.
pub(super) fn should_hide_underlying_terminal(
    center_is_editor: bool,
    center_git_diff_visible: bool,
) -> bool {
    center_is_editor || center_git_diff_visible
}

/// A fresh page starts at its first hunk; a previous/next click never wraps,
/// overflows or jumps to a hunk from an obsolete page.
fn adjacent_hunk_index(selected: Option<usize>, count: usize, next: bool) -> Option<usize> {
    if next {
        match selected {
            None if count > 0 => Some(0),
            Some(index) => index.checked_add(1).filter(|next| *next < count),
            _ => None,
        }
    } else {
        selected
            .filter(|index| *index < count)
            .and_then(|index| index.checked_sub(1))
    }
}

fn center_hunk_controls(selected: Option<usize>, count: usize) -> (usize, bool, bool) {
    if selected.is_some_and(|index| index >= count) {
        return (0, false, false);
    }
    (
        selected.map_or(0, |index| index.saturating_add(1)),
        adjacent_hunk_index(selected, count, false).is_some(),
        adjacent_hunk_index(selected, count, true).is_some(),
    )
}

fn center_hunk_target(
    rows: &[CenterGitDiffRow],
    selected: Option<usize>,
    next: bool,
) -> Option<(usize, usize)> {
    let positions = hunk_positions(rows);
    let target = adjacent_hunk_index(selected, positions.len(), next)?;
    positions.get(target).copied().map(|row| (target, row))
}

impl Workbench {
    pub(super) fn center_git_diff_is_current(&self) -> bool {
        self.center_git_diff_source.as_ref().is_some_and(|source| {
            git_dialog_file_source_matches(
                self.workspace_generation,
                self.git_file_diff_paging.as_ref(),
                source,
            )
        })
    }

    pub(super) fn open_center_git_diff(&mut self, cx: &mut Context<Self>) {
        if self.git_diff_loading
            || !native_action_dialog_ui::can_expand_read_only_diff(self.git_diff.as_deref(), false)
        {
            self.model
                .set_status("Select a changed Git file and finish loading its patch first");
            cx.notify();
            return;
        }
        let Some(paging) = self.git_file_diff_paging.as_ref() else {
            self.model
                .set_status("Center Diff is for a selected Git file, not History/Compare");
            cx.notify();
            return;
        };
        self.center_git_diff_source = Some(GitDialogFileSource {
            workspace_generation: self.workspace_generation,
            source_ticket: paging.source_ticket,
            relative: paging.relative.clone(),
        });
        // Switch the active surface, not the snapshot ownership: the original
        // remains in GitOriginalTabs and can be selected again without losing data.
        self.git_original_active = false;
        self.center_git_diff_hunk = None;
        self.center_git_diff_scroll
            .scroll_to_item(0, gpui::ScrollStrategy::Top);
        cx.notify();
    }

    pub(super) fn close_center_git_diff(&mut self, cx: &mut Context<Self>) {
        self.center_git_diff_source = None;
        self.center_git_diff_hunk = None;
        cx.notify();
    }

    fn go_to_center_git_hunk(&mut self, next: bool, cx: &mut Context<Self>) {
        if !self.center_git_diff_is_current() || self.git_diff_loading {
            return;
        }
        if self.git_diff.is_none() {
            return;
        }
        // The page loader already parsed the bounded patch once. Reuse its
        // cached virtual rows rather than re-parsing on every Hunk click.
        if let Some((selected, row_index)) = center_hunk_target(
            self.center_git_diff_rows.as_ref(),
            self.center_git_diff_hunk,
            next,
        ) {
            self.center_git_diff_hunk = Some(selected);
            self.center_git_diff_scroll
                .scroll_to_item(row_index, gpui::ScrollStrategy::Center);
            cx.notify();
        }
    }

    fn change_center_git_diff_page(&mut self, next: bool, cx: &mut Context<Self>) {
        if !self.center_git_diff_is_current() || self.git_diff_loading {
            return;
        }
        self.center_git_diff_hunk = None;
        self.center_git_diff_scroll
            .scroll_to_item(0, gpui::ScrollStrategy::Top);
        self.change_git_file_diff_page(next, cx);
    }

    pub(super) fn render_center_git_diff(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let valid = self.center_git_diff_is_current();
        let paging = self.git_file_diff_paging.as_ref();
        let page_index = paging.map_or(0, |page| page.previous_starts.len() + 1);
        let can_previous_page = valid
            && !self.git_diff_loading
            && paging.is_some_and(|page| !page.previous_starts.is_empty());
        let can_next_page = valid
            && !self.git_diff_loading
            && paging
                .is_some_and(|page| page.next_line.is_some() && page.previous_starts.len() < 31);
        let status = if !valid {
            "Git file or Workspace changed; close this preview and select a file again".to_owned()
        } else if self.git_diff_loading {
            "Reading next read-only Git page…".to_owned()
        } else {
            format!("Page {page_index} · live Git · 32 KiB/page · no Stage/Commit writes")
        };
        let rows = if valid && !self.git_diff_loading {
            self.center_git_diff_rows.clone()
        } else {
            Arc::new(Vec::new())
        };
        let hunk_count = rows.iter().filter(|row| row.hunk.is_some()).count();
        let (selected_hunk, previous_available, next_available) =
            center_hunk_controls(self.center_git_diff_hunk, hunk_count);
        let can_previous_hunk = valid && !self.git_diff_loading && previous_available;
        let can_next_hunk = valid && !self.git_diff_loading && next_available;
        let count = rows.len();
        div()
            .id("central-git-diff-view")
            .flex_1()
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .border_t_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .px_3()
                    .py_2()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .bg(rgb(TOOLBAR))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .text_color(rgb(TEXT))
                            .truncate()
                            .child(format!(
                                "Git Diff · {}",
                                self.center_git_diff_source
                                    .as_ref()
                                    .map(|source| source.relative.display().to_string())
                                    .unwrap_or_default(),
                            )),
                    )
                    .child(
                        div()
                            .id("central-git-diff-close")
                            .cursor_pointer()
                            .text_sm()
                            .text_color(rgb(TEXT))
                            .child("Close Diff")
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.close_center_git_diff(cx);
                            })),
                    ),
            )
            .child(
                div()
                    .px_3()
                    .py_1()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(status),
            )
            .child(
                div()
                    .px_3()
                    .py_1()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(if can_previous_page {
                        div()
                            .id("central-git-previous-page")
                            .cursor_pointer()
                            .text_sm()
                            .text_color(rgb(TEXT))
                            .child("Previous page")
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.change_center_git_diff_page(false, cx);
                            }))
                            .into_any_element()
                    } else {
                        div()
                            .text_sm()
                            .text_color(rgb(MUTED))
                            .child("Previous page")
                            .into_any_element()
                    })
                    .child(if can_next_page {
                        div()
                            .id("central-git-next-page")
                            .cursor_pointer()
                            .text_sm()
                            .text_color(rgb(TEXT))
                            .child("Next page")
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.change_center_git_diff_page(true, cx);
                            }))
                            .into_any_element()
                    } else {
                        div()
                            .text_sm()
                            .text_color(rgb(MUTED))
                            .child("Next page")
                            .into_any_element()
                    })
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child(format!("Hunk {selected_hunk}/{hunk_count}")),
                    )
                    .child(if can_previous_hunk {
                        div()
                            .id("central-git-previous-hunk")
                            .cursor_pointer()
                            .text_sm()
                            .text_color(rgb(TEXT))
                            .child("Previous hunk")
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.go_to_center_git_hunk(false, cx);
                            }))
                            .into_any_element()
                    } else {
                        div()
                            .text_sm()
                            .text_color(rgb(MUTED))
                            .child("Previous hunk")
                            .into_any_element()
                    })
                    .child(if can_next_hunk {
                        div()
                            .id("central-git-next-hunk")
                            .cursor_pointer()
                            .text_sm()
                            .text_color(rgb(TEXT))
                            .child("Next hunk")
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.go_to_center_git_hunk(true, cx);
                            }))
                            .into_any_element()
                    } else {
                        div()
                            .text_sm()
                            .text_color(rgb(MUTED))
                            .child("Next hunk")
                            .into_any_element()
                    }),
            )
            .child(
                div()
                    .px_3()
                    .min_w_0()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .truncate()
                    .child("Old | New | Patch · read-only · long rows clipped"),
            )
            .child(
                gpui::uniform_list(
                    "central-git-diff-virtual-rows",
                    count,
                    cx.processor(move |_, range: std::ops::Range<usize>, _, _| {
                        range
                            .filter_map(|index| rows.get(index))
                            .map(|row| {
                                div()
                                    .h(px(22.0))
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .whitespace_nowrap()
                                    .font_family("Consolas")
                                    .text_sm()
                                    .when(row.hunk.is_some(), |line| line.bg(rgb(0x28364a)))
                                    .child(
                                        div()
                                            .w(px(55.0))
                                            .flex()
                                            .justify_end()
                                            .text_xs()
                                            .text_color(rgb(MUTED))
                                            .child(
                                                row.old.map_or_else(String::new, |n| n.to_string()),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .w(px(55.0))
                                            .flex()
                                            .justify_end()
                                            .text_xs()
                                            .text_color(rgb(MUTED))
                                            .child(
                                                row.new.map_or_else(String::new, |n| n.to_string()),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .min_w_0()
                                            .whitespace_nowrap()
                                            .text_color(rgb(git_patch_line_color(&row.text)))
                                            .child(row.text.clone()),
                                    )
                                    .into_any_element()
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .flex_1()
                .min_h_0()
                .w_full()
                .px_3()
                .py_2()
                .track_scroll(&self.center_git_diff_scroll),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        adjacent_hunk_index, center_git_diff_rows, center_hunk_controls, center_hunk_target,
        hunk_positions, should_hide_underlying_terminal, should_show_center_git_diff_view,
        uses_paged_git_preview, GitChangeArea, MAX_CENTER_DIFF_SOURCE_BYTES,
    };

    #[test]
    fn center_diff_virtual_rows_include_hunks_beyond_modal_first_400_lines() {
        let mut patch = String::from("diff --git a/x b/x\n");
        patch.push_str("@@ -1,1 +1,1 @@\n");
        for _ in 0..510 {
            patch.push_str(" unchanged\n");
        }
        patch.push_str("@@ -512,1 +512,1 @@\n+new\n");
        let rows = center_git_diff_rows(&patch);
        assert!(rows.len() > 400);
        let targets = hunk_positions(&rows);
        assert_eq!(targets, vec![1, 512]);
        assert_eq!(rows[513].new, Some(512));
        assert_eq!(rows[513].old, None);
        assert!(rows.len() <= patch.len() + 1);
    }

    #[test]
    fn center_diff_limits_long_single_row_and_total_bytes() {
        let patch = format!("@@ -1 +1 @@\n+{}\n", "🙂".repeat(6000));
        let rows = center_git_diff_rows(&patch);
        assert_eq!(rows.len(), 2);
        assert!(rows[1].text.contains("row display clipped"));
        assert!(rows[1].text.chars().count() <= 4200);
        let oversized = "x".repeat(MAX_CENTER_DIFF_SOURCE_BYTES + 1);
        assert!(center_git_diff_rows(&oversized).is_empty());
    }
    #[test]
    fn center_diff_never_overrides_active_git_original_and_only_displays_current_source() {
        assert!(!should_show_center_git_diff_view(true, true));
        assert!(should_show_center_git_diff_view(false, true));
        assert!(!should_show_center_git_diff_view(false, false));
        assert!(!should_show_center_git_diff_view(true, false));
    }

    #[test]
    fn center_diff_hides_underlying_terminal_even_when_no_host_tab_exists() {
        assert!(!should_hide_underlying_terminal(false, false));
        assert!(should_hide_underlying_terminal(true, false));
        assert!(should_hide_underlying_terminal(false, true));
        assert!(should_hide_underlying_terminal(true, true));
    }

    #[test]
    fn center_diff_hunk_navigation_never_wraps_overflows_or_targets_stale_page() {
        assert_eq!(adjacent_hunk_index(None, 0, true), None);
        assert_eq!(adjacent_hunk_index(None, 3, true), Some(0));
        assert_eq!(adjacent_hunk_index(Some(0), 3, true), Some(1));
        assert_eq!(adjacent_hunk_index(Some(2), 3, true), None);
        assert_eq!(adjacent_hunk_index(Some(0), 3, false), None);
        assert_eq!(adjacent_hunk_index(Some(2), 3, false), Some(1));
        assert_eq!(adjacent_hunk_index(Some(9), 3, false), None);
        assert_eq!(adjacent_hunk_index(Some(usize::MAX), 3, true), None);
        assert_eq!(adjacent_hunk_index(Some(usize::MAX), 3, false), None);
    }
    #[test]
    fn center_diff_hunk_buttons_fail_closed_on_invalid_or_overflowed_selection() {
        assert_eq!(center_hunk_controls(None, 0), (0, false, false));
        assert_eq!(center_hunk_controls(None, 3), (0, false, true));
        assert_eq!(center_hunk_controls(Some(0), 3), (1, false, true));
        assert_eq!(center_hunk_controls(Some(1), 3), (2, true, true));
        assert_eq!(center_hunk_controls(Some(2), 3), (3, true, false));
        assert_eq!(center_hunk_controls(Some(3), 3), (0, false, false));
        assert_eq!(center_hunk_controls(Some(usize::MAX), 3), (0, false, false));
    }
    #[test]
    fn center_hunk_jump_targets_cached_virtual_rows_and_refuses_outdated_positions() {
        let mut patch = String::from("diff --git a/x b/x\n@@ -1 +1 @@\n");
        for _ in 0..530 {
            patch.push_str(" context\n");
        }
        patch.push_str("@@ -532 +532 @@\n+new\n");
        let rows = center_git_diff_rows(&patch);
        assert_eq!(center_hunk_target(&rows, None, true), Some((0, 1)));
        assert_eq!(center_hunk_target(&rows, Some(0), true), Some((1, 532)));
        assert_eq!(center_hunk_target(&rows, Some(1), true), None);
        assert_eq!(center_hunk_target(&rows, Some(1), false), Some((0, 1)));
        assert_eq!(center_hunk_target(&rows, Some(999), true), None);
        assert_eq!(center_hunk_target(&[], Some(0), false), None);
    }
    #[test]
    fn git_change_rows_expose_bounded_center_diff_without_mixing_staged_previews() {
        assert!(uses_paged_git_preview(None, false));
        assert!(uses_paged_git_preview(Some(GitChangeArea::Changes), false));
        assert!(uses_paged_git_preview(
            Some(GitChangeArea::Untracked),
            false
        ));
        assert!(!uses_paged_git_preview(Some(GitChangeArea::Staged), false));
        assert!(!uses_paged_git_preview(
            Some(GitChangeArea::Conflicts),
            false
        ));
        assert!(!uses_paged_git_preview(Some(GitChangeArea::Changes), true));
    }
}
