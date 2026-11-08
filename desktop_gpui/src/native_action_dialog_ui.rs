//! Explorer create/rename/delete prompts use the same reusable Native modal
//! component as Settings. Destructive delete remains a deliberately explicit
//! TWO-click confirmation, never a silent file-system operation.

use super::*;
use native_components::action_button;

impl Workbench {
    pub(super) fn render_quick_open_dialog(
        &self,
        viewport_width: f32,
        viewport_height: f32,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let input = self.quick_open.clone()?;
        if self.settings_section.is_some() || self.explorer_action.is_some() {
            return None;
        }
        let mut dims = NativeModalSize::Settings.resolve(viewport_width, viewport_height);
        dims.width = dims.width.min(760.0);
        dims.height = dims.height.min(580.0);
        let modal = AleraNativeModal::new(dims);
        let matches = self
            .file_index
            .as_ref()
            .map(|index| index.search(input.read(cx).query(), 18))
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
            format!("{total} files indexed · {count} matches")
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
        let dialog = modal
            .frame(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .child(
                        modal.header(
                            "Open file",
                            icon_button("native-quick-open-close", UiIcon::Close, "Close", false)
                                .on_click(cx.listener(|view, _, window, cx| {
                                    view.close_quick_open_dialog(window, cx);
                                })),
                        ),
                    )
                    .child(content),
            )
            .on_key_down(
                cx.listener(move |view, event: &gpui::KeyDownEvent, window, cx| {
                    let key = event.keystroke.key.as_str();
                    if key.eq_ignore_ascii_case("escape") {
                        view.close_quick_open_dialog(window, cx);
                        cx.stop_propagation();
                    } else if (key.eq_ignore_ascii_case("enter")
                        || key.eq_ignore_ascii_case("return"))
                        && first_match.is_some()
                    {
                        if let Some(path) = &first_match {
                            view.choose_quick_open_path(path, window, cx);
                        }
                        cx.stop_propagation();
                    }
                }),
            );
        Some(
            modal
                .overlay(
                    dialog,
                    cx.listener(|view, _, window, cx| {
                        view.close_quick_open_dialog(window, cx);
                        cx.stop_propagation();
                    }),
                )
                .into_any_element(),
        )
    }

    fn choose_quick_open_path(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        if self.model.open_path(path) {
            self.open_editor(cx);
            self.quick_open = None;
            self.persist_session();
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
        if let Some(editor) = self.focused_editor() {
            window.focus(&editor.read(cx).focus_handle(cx), cx);
        } else {
            window.focus(&self.explorer_keyboard_focus, cx);
        }
        cx.notify();
    }

    pub(super) fn render_explorer_action_dialog(
        &self,
        viewport_width: f32,
        viewport_height: f32,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let action = self.explorer_action?;
        if self.settings_section.is_some() {
            return None;
        }
        let mut dims = NativeModalSize::Compact.resolve(viewport_width, viewport_height);
        dims.height = dims.height.min(if action == ExplorerAction::Delete {
            265.0
        } else {
            245.0
        });
        let modal = AleraNativeModal::new(dims);
        let destructive = action == ExplorerAction::Delete;
        let title = if destructive {
            "Delete permanently"
        } else {
            action.label()
        };
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
                                window.focus(&view.explorer_keyboard_focus, cx);
                            }),
                        ),
                    )
                    .child(
                        action_button("explorer-action-apply", button_text, true, destructive)
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.apply_explorer_action(cx);
                                if view.explorer_action.is_none() {
                                    window.focus(&view.explorer_keyboard_focus, cx);
                                }
                            })),
                    ),
            );
        let dialog = modal
            .frame(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(
                        modal.header(
                            title,
                            icon_button("explorer-modal-close", UiIcon::Close, "Close", false)
                                .on_click(cx.listener(|view, _, window, cx| {
                                    view.cancel_explorer_action(cx);
                                    window.focus(&view.explorer_keyboard_focus, cx);
                                })),
                        ),
                    )
                    .child(body),
            )
            .key_context("AleraNativeActionDialog")
            .track_focus(&self.explorer_action_modal_focus)
            .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                let key = event.keystroke.key.as_str();
                if key.eq_ignore_ascii_case("escape") {
                    view.cancel_explorer_action(cx);
                    window.focus(&view.explorer_keyboard_focus, cx);
                } else if key.eq_ignore_ascii_case("enter") || key.eq_ignore_ascii_case("return") {
                    view.apply_explorer_action(cx);
                    if view.explorer_action.is_none() {
                        window.focus(&view.explorer_keyboard_focus, cx);
                    }
                }
                cx.stop_propagation();
            }));
        Some(
            modal
                .overlay(
                    dialog,
                    cx.listener(|view, _, window, cx| {
                        view.cancel_explorer_action(cx);
                        window.focus(&view.explorer_keyboard_focus, cx);
                        cx.stop_propagation();
                    }),
                )
                .into_any_element(),
        )
    }
}
