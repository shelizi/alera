//! Explorer create/rename/delete prompts use the same reusable Native modal
//! component as Settings. Destructive delete remains a deliberately explicit
//! TWO-click confirmation, never a silent file-system operation.

use super::*;
use native_components::action_button;

/// The expanded viewport only displays an already-fetched read-only patch.
pub(super) fn can_expand_read_only_diff(patch: Option<&str>, loading: bool) -> bool {
    !loading && patch.is_some_and(|text| !text.trim().is_empty())
}

impl Workbench {
    /// Present a captured bounded Git patch in a larger read-only dialog.
    /// A later Workspace switch cannot replace this snapshot or mutate Git.
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
        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .title("Git patch · expanded read-only preview")
                .w(px(760.0))
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
                                .child("Snapshot only · up to 32 KiB · no staging or Git writes"),
                        )
                        .child(
                            div()
                                .id("expanded-git-diff-scroll")
                                .w_full()
                                .max_h(px(430.0))
                                .overflow_y_scroll()
                                .overflow_x_scroll()
                                .font_family("Consolas")
                                .text_sm()
                                .text_color(rgb(TEXT))
                                .child(patch.clone()),
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
        let first_match = matches.first().cloned();
        let count = matches.len();
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
                            index == 0,
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
                if (key.eq_ignore_ascii_case("enter") || key.eq_ignore_ascii_case("return"))
                    && first_match.is_some()
                {
                    if let Some(path) = &first_match {
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
    use super::can_expand_read_only_diff;

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
}
