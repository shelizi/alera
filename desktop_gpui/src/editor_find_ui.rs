//! In-document Find/Replace, using the shared native IME input and search model.
use super::*;
use crate::quick_open_ui::QuickOpenInput;
use alera_gpui_desktop::editor_search::EditorSearch;

actions!(
    alera_gpui_editor_find,
    [ShowFind, FindNext, FindPrevious, HideFind, FocusFindInput]
);

pub(super) fn bind_find_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-f", ShowFind, Some("AleraGpuiEditorSurface")),
        KeyBinding::new("cmd-f", ShowFind, Some("AleraGpuiEditorSurface")),
        KeyBinding::new("ctrl-h", ShowFind, Some("AleraGpuiEditorSurface")),
        KeyBinding::new("cmd-alt-f", ShowFind, Some("AleraGpuiEditorSurface")),
        KeyBinding::new("enter", FindNext, Some("AleraGpuiEditorFind")),
        KeyBinding::new("shift-enter", FindPrevious, Some("AleraGpuiEditorFind")),
        KeyBinding::new("escape", HideFind, Some("AleraGpuiEditorFind")),
        KeyBinding::new("tab", FocusFindInput, Some("AleraGpuiEditorFind")),
        KeyBinding::new("shift-tab", FocusFindInput, Some("AleraGpuiEditorFind")),
    ]);
}

pub(super) struct EditorFindUi {
    query: Entity<QuickOpenInput>,
    replacement: Entity<QuickOpenInput>,
    visible: bool,
    case_sensitive: bool,
}

impl EditorFindUi {
    pub(super) fn new(cx: &mut Context<NativeEditor>) -> Self {
        let query = cx.new(QuickOpenInput::new);
        let replacement = cx.new(QuickOpenInput::new);
        for input in [&query, &replacement] {
            cx.observe(input, |_, _, cx| cx.notify()).detach();
        }
        Self {
            query,
            replacement,
            visible: false,
            case_sensitive: false,
        }
    }

    fn search(&self, cx: &App) -> EditorSearch {
        EditorSearch::new(self.query.read(cx).query(), self.case_sensitive)
    }

    fn composing(&self, window: &mut Window, cx: &mut App) -> bool {
        [&self.query, &self.replacement].into_iter().any(|input| {
            input.update(cx, |input, cx| {
                input.marked_text_range(window, cx).is_some()
            })
        })
    }
}

impl NativeEditor {
    fn find_ready(&self, window: &mut Window, cx: &mut App) -> bool {
        self.tabs
            .document()
            .is_some_and(|doc| doc.marked().is_none())
            && !self.find.composing(window, cx)
    }

    pub(super) fn show_find(&mut self, _: &ShowFind, window: &mut Window, cx: &mut Context<Self>) {
        if !self.find_ready(window, cx) {
            return;
        }
        self.stop_drag();
        self.find.visible = true;
        window.focus(&self.find.query.focus_handle(cx), cx);
        cx.notify();
    }

    pub(super) fn hide_find(&mut self, _: &HideFind, window: &mut Window, cx: &mut Context<Self>) {
        if !self.find_ready(window, cx) {
            return;
        }
        self.find.visible = false;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(super) fn focus_find_input(
        &mut self,
        _: &FocusFindInput,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.find_ready(window, cx) {
            return;
        }
        let query = self.find.query.focus_handle(cx);
        let target = if query.is_focused(window) {
            self.find.replacement.focus_handle(cx)
        } else {
            query
        };
        window.focus(&target, cx);
    }

    pub(super) fn find_next(&mut self, _: &FindNext, window: &mut Window, cx: &mut Context<Self>) {
        self.navigate_match(false, window, cx);
    }

    pub(super) fn find_previous(
        &mut self,
        _: &FindPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigate_match(true, window, cx);
    }

    fn navigate_match(&mut self, previous: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.find_ready(window, cx) {
            return;
        }
        let search = self.find.search(cx);
        let result = if previous {
            search.find_previous(self.doc_mut())
        } else {
            search.find_next(self.doc_mut())
        };
        self.status = match result {
            Ok(Some(_)) => "Selected search match".into(),
            Ok(None) => "No matching text".into(),
            Err(error) => format!("Find rejected: {error}"),
        };
        self.navigated(cx);
    }

    fn replace_match(&mut self, all: bool, window: &mut Window, cx: &mut Context<Self>) {
        // Recheck at dispatch time: a queued click may outlive the last tab
        // or arrive after native IME composition has started in any input.
        let composing = self.find.composing(window, cx);
        let search = self.find.search(cx);
        let replacement = self.find.replacement.read(cx).query().to_owned();
        let result = replace_document(
            self.tabs.document_mut(),
            &search,
            &replacement,
            all,
            composing,
        );
        match result {
            Ok(0) => {
                self.status = "No selected matching text to replace".into();
                cx.notify();
            }
            Ok(count) => {
                self.status = format!("Replaced {count} match(es)");
                self.line_geometry.clear();
                self.edit_result(Ok(()), cx);
            }
            Err(error) => self.edit_result(Err(error), cx),
        }
    }

    pub(super) fn render_find_toolbar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        if !self.find.visible {
            return div()
                .id("editor-show-find")
                .px_3()
                .py_1()
                .text_sm()
                .cursor_pointer()
                .child("Find / Replace")
                .on_click(cx.listener(|this, _, window, cx| {
                    this.show_find(&ShowFind, window, cx);
                }))
                .into_any_element();
        }
        let search = self.find.search(cx);
        let matches = search.matches(self.doc());
        let current = matches
            .iter()
            .position(|range| *range == self.doc().selection());
        let ready = self.find_ready(window, cx);
        let navigable = ready && !matches.is_empty();
        let replaceable = ready && current.is_some();
        div()
            .key_context("AleraGpuiEditorFind")
            .flex()
            .flex_col()
            .gap_1()
            .px_3()
            .py_1()
            .text_sm()
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child("Find")
                    .child(
                        div()
                            .min_w(px(160.0))
                            .flex_1()
                            .child(self.find.query.clone()),
                    )
                    .child(
                        find_button("find-previous", "Previous", navigable).on_click(
                            cx.listener(|this, _, window, cx| {
                                this.navigate_match(true, window, cx)
                            }),
                        ),
                    )
                    .child(find_button("find-next", "Next", navigable).on_click(
                        cx.listener(|this, _, window, cx| this.navigate_match(false, window, cx)),
                    ))
                    .child(
                        find_button(
                            "find-case",
                            if self.find.case_sensitive {
                                "Match Case: On"
                            } else {
                                "Match Case: Off"
                            },
                            ready,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            if this.find_ready(window, cx) {
                                this.find.case_sensitive = !this.find.case_sensitive;
                                cx.notify();
                            }
                        })),
                    )
                    .child(format!(
                        "{} / {}",
                        current.map_or(0, |index| index + 1),
                        matches.len()
                    ))
                    .child(find_button("find-close", "Close", ready).on_click(
                        cx.listener(|this, _, window, cx| this.hide_find(&HideFind, window, cx)),
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child("Replace")
                    .child(
                        div()
                            .min_w(px(160.0))
                            .flex_1()
                            .child(self.find.replacement.clone()),
                    )
                    .child(
                        find_button("find-replace", "Replace Current", replaceable).on_click(
                            cx.listener(|this, _, window, cx| {
                                this.replace_match(false, window, cx)
                            }),
                        ),
                    )
                    .child(
                        find_button("find-replace-all", "Replace All", navigable).on_click(
                            cx.listener(|this, _, window, cx| this.replace_match(true, window, cx)),
                        ),
                    ),
            )
            .into_any_element()
    }
}

fn replace_document(
    document: Option<&mut EditorDocument>,
    search: &EditorSearch,
    replacement: &str,
    all: bool,
    input_composing: bool,
) -> io::Result<usize> {
    let Some(document) = document else {
        return Ok(0);
    };
    if input_composing {
        return Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "Finish IME composition before replacing text",
        ));
    }
    if all {
        search.replace_all(document, replacement)
    } else {
        search
            .replace_current(document, replacement)
            .map(usize::from)
    }
}

fn find_button(id: &'static str, label: &'static str, enabled: bool) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .px_2()
        .py_1()
        .rounded_sm()
        .bg(rgb(PANEL_BG))
        .text_color(rgb(if enabled { EDITOR_TEXT } else { MUTED_TEXT }))
        .when(enabled, |button| button.cursor_pointer())
        .child(label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_rechecks_selection_and_active_document() {
        let root = tempfile::tempdir().unwrap();
        let mut tabs = EditorTabs::new(None);
        tabs.open_scratch(root.path()).unwrap();
        let doc = tabs.document_mut().unwrap();
        doc.replace(None, "猫 cat cat").unwrap();
        doc.set_selection(0..0).unwrap();
        let search = EditorSearch::new("cat", true);
        assert_eq!(
            replace_document(Some(doc), &search, "犬", false, false).unwrap(),
            0
        );
        search.find_next(doc).unwrap();
        assert_eq!(
            replace_document(Some(doc), &search, "犬", false, false).unwrap(),
            1
        );
        assert_eq!(doc.content(), "猫 犬 cat");
        assert!(doc.undo());
        assert_eq!(doc.content(), "猫 cat cat");
        assert_eq!(
            replace_document(Some(doc), &search, "犬", true, false).unwrap(),
            2
        );
        assert!(doc.undo());
        assert_eq!(doc.content(), "猫 cat cat");
        tabs.close_active(true);
        tabs.close_active(true);
        assert!(tabs.is_empty());
        for all in [false, true] {
            assert_eq!(
                replace_document(tabs.document_mut(), &search, "x", all, false).unwrap(),
                0
            );
        }
    }

    #[test]
    fn replacement_preserves_document_during_input_or_document_composition() {
        let root = tempfile::tempdir().unwrap();
        let mut doc = EditorDocument::scratch(root.path()).unwrap();
        doc.replace(None, "cat cat").unwrap();
        doc.set_selection(0..3).unwrap();
        let search = EditorSearch::new("cat", true);
        for document_composing in [false, true] {
            if document_composing {
                doc.replace_and_mark(None, "cat", Some(3..3)).unwrap();
            }
            let content = doc.content().to_owned();
            let selection = doc.selection();
            let marked = doc.marked();
            let history = doc.history_depth();
            for all in [false, true] {
                let error =
                    replace_document(Some(&mut doc), &search, "犬", all, !document_composing)
                        .unwrap_err();
                assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
                assert_eq!(doc.content(), content);
                assert_eq!(doc.selection(), selection);
                assert_eq!(doc.marked(), marked);
                assert_eq!(doc.history_depth(), history);
            }
        }
    }
}
