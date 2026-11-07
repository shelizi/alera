//! First experimental native GPUI file editor surface.
//!
//! The OS IME enters through EntityInputHandler; the pure EditorDocument
//! owns Unicode indexing and conflict-safe writes. This deliberately is NOT
//! a virtualized production code editor: only the first 600 lines are drawn,
//! click places the caret at line end, and IME candidate rectangle/selection
//! geometry need independent visual and real Chinese IME tests.
use alera_gpui_desktop::editor::{EditorDocument, EditorDraftStore};
use gpui::{
    actions, div, prelude::*, px, relative, rgb, App, Bounds, ClipboardItem, Context, CursorStyle,
    Element, ElementId, ElementInputHandler, Entity, EntityInputHandler, FocusHandle, Focusable,
    GlobalElementId, InspectorElementId, KeyBinding, LayoutId, MouseButton, Pixels, Point, Style,
    UTF16Selection, Window,
};
use std::ops::Range;

const MAX_RENDER_LINES: usize = 600;
const PANEL_BG: u32 = 0x15171b;
const EDITOR_TEXT: u32 = 0xe1e4e8;
const MUTED_TEXT: u32 = 0x9fa7b3;

actions!(
    alera_gpui_native_editor,
    [MoveLeft, MoveRight, Backspace, Delete, Newline, SelectAll, SaveFile, Paste, Copy, Cut]
);

pub fn bind_editor_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("left", MoveLeft, Some("AleraGpuiEditor")),
        KeyBinding::new("right", MoveRight, Some("AleraGpuiEditor")),
        KeyBinding::new("backspace", Backspace, Some("AleraGpuiEditor")),
        KeyBinding::new("delete", Delete, Some("AleraGpuiEditor")),
        KeyBinding::new("enter", Newline, Some("AleraGpuiEditor")),
        KeyBinding::new("ctrl-a", SelectAll, Some("AleraGpuiEditor")),
        KeyBinding::new("cmd-a", SelectAll, Some("AleraGpuiEditor")),
        KeyBinding::new("ctrl-s", SaveFile, Some("AleraGpuiEditor")),
        KeyBinding::new("cmd-s", SaveFile, Some("AleraGpuiEditor")),
        KeyBinding::new("ctrl-v", Paste, Some("AleraGpuiEditor")),
        KeyBinding::new("cmd-v", Paste, Some("AleraGpuiEditor")),
        KeyBinding::new("ctrl-c", Copy, Some("AleraGpuiEditor")),
        KeyBinding::new("cmd-c", Copy, Some("AleraGpuiEditor")),
        KeyBinding::new("ctrl-x", Cut, Some("AleraGpuiEditor")),
        KeyBinding::new("cmd-x", Cut, Some("AleraGpuiEditor")),
    ]);
}

pub struct NativeEditor {
    pub document: EditorDocument,
    draft_store: Option<EditorDraftStore>,
    focus: FocusHandle,
    status: String,
}

impl NativeEditor {
    pub fn new(
        document: EditorDocument,
        draft_store: Option<EditorDraftStore>,
        cx: &mut Context<Self>,
    ) -> Self {
        let recovered = document.is_dirty();
        Self {
            document,
            draft_store,
            focus: cx.focus_handle(),
            status: if recovered {
                "Recovered local unsaved edit; review before saving".into()
            } else {
                "Native UTF-8 text editor (experimental)".into()
            },
        }
    }

    pub fn has_unsaved_changes(&self) -> bool {
        self.document.is_dirty()
    }

    pub fn save(&mut self, cx: &mut Context<Self>) -> bool {
        match self.document.save() {
            Ok(()) => {
                self.status = "File saved; reopen preview to see latest bytes".into();
                if let Some(store) = &self.draft_store {
                    if let Err(error) = store.remove(&self.document) {
                        self.status = format!("File saved; draft cleanup failed: {error}");
                    }
                }
                cx.notify();
                true
            }
            Err(error) => {
                self.status = format!("Save blocked: {error}");
                cx.notify();
                false
            }
        }
    }

    pub fn discard_recovery(&self) -> std::io::Result<()> {
        match &self.draft_store {
            Some(store) => store.remove(&self.document),
            None => Ok(()),
        }
    }

    fn move_left(&mut self, _: &MoveLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.document.move_left();
        cx.notify();
    }
    fn move_right(&mut self, _: &MoveRight, _: &mut Window, cx: &mut Context<Self>) {
        self.document.move_right();
        cx.notify();
    }
    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        let result = self.document.backspace();
        self.edit_result(result, cx);
    }
    fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        let result = self.document.delete_forward();
        self.edit_result(result, cx);
    }
    fn newline(&mut self, _: &Newline, _: &mut Window, cx: &mut Context<Self>) {
        let result = self.document.replace(None, "\n");
        self.edit_result(result, cx);
    }
    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        let size = self.document.content().len();
        let result = self.document.set_selection(0..size);
        self.edit_result(result, cx);
    }
    fn save_file(&mut self, _: &SaveFile, _: &mut Window, cx: &mut Context<Self>) {
        self.save(cx);
    }
    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(value) = cx.read_from_clipboard().and_then(|item| item.text()) {
            let result = self.document.replace(None, &value);
            self.edit_result(result, cx);
        }
    }
    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        let range = self.document.selection();
        if !range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.document.content()[range].to_string(),
            ));
        }
    }
    fn cut(&mut self, _: &Cut, _: &mut Window, cx: &mut Context<Self>) {
        let range = self.document.selection();
        if !range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.document.content()[range].to_string(),
            ));
            let result = self.document.replace(None, "");
            self.edit_result(result, cx);
        }
    }
    fn edit_result(&mut self, result: std::io::Result<()>, cx: &mut Context<Self>) {
        match result {
            Err(error) => self.status = format!("Edit rejected: {error}"),
            Ok(()) => {
                if let Some(store) = &self.draft_store {
                    if let Err(error) = store.persist(&self.document) {
                        self.status = format!("Local recovery save failed: {error}");
                    }
                }
            }
        }
        cx.notify();
    }
}

impl Focusable for NativeEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl EntityInputHandler for NativeEditor {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let (content, actual) = self.document.text_for_utf16_range(range_utf16);
        *actual_range = Some(actual);
        Some(content)
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.document.utf16_selection(),
            reversed: false,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.document.utf16_marked()
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.document.unmark();
        cx.notify();
    }
    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result = self.document.replace(range_utf16, text);
        self.edit_result(result, cx);
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        selected_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result = self
            .document
            .replace_and_mark(range_utf16, text, selected_utf16);
        self.edit_result(result, cx);
    }
    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        // A real code editor needs scroll-aware text shaping and cursor
        // rectangles for Windows/macOS candidate windows. This conservative
        // rectangle keeps the platform IME pipeline intact without claiming
        // full candidate placement support.
        Some(bounds)
    }
    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.document.utf16_selection().end)
    }
}

/// GPUI registers input on paint. A small nonvisible element is enough
/// for keyboard and native IME dispatch; multiline text is rendered below.
struct NativeInputCapture {
    editor: Entity<NativeEditor>,
}

impl Element for NativeInputCapture {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = px(1.0).into();
        (window.request_layout(style, [], cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus = self.editor.read(cx).focus_handle(cx);
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );
    }
}

impl IntoElement for NativeInputCapture {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

impl Render for NativeEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let text = self.document.content();
        let caret = self.document.cursor();
        let mut offset = 0;
        let lines: Vec<_> = text
            .split('\n')
            .take(MAX_RENDER_LINES)
            .enumerate()
            .map(|(i, line)| {
                let start = offset;
                let end = start + line.len();
                let mut display = line.to_owned();
                if caret >= start && caret <= end && display.is_char_boundary(caret - start) {
                    display.insert(caret - start, '│');
                }
                offset = end + 1;
                (i, end, display)
            })
            .collect();
        let more = text.split('\n').count() > MAX_RENDER_LINES;
        let status = format!(
            "{} · {}{}",
            self.status,
            if self.document.is_dirty() {
                "Modified"
            } else {
                "Saved"
            },
            if self.document.marked().is_some() {
                " · composing"
            } else {
                ""
            }
        );
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(PANEL_BG))
            .text_color(rgb(EDITOR_TEXT))
            .key_context("AleraGpuiEditor")
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::move_left))
            .on_action(cx.listener(Self::move_right))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::newline))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::save_file))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, _cx| window.focus(&this.focus)),
            )
            .child(
                div()
                    .px_3()
                    .py_1()
                    .text_sm()
                    .text_color(rgb(MUTED_TEXT))
                    .child(status),
            )
            .child(
                div()
                    .id("gpui-editor-text-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_3()
                    .py_2()
                    .font_family("JetBrains Mono")
                    .text_sm()
                    .child(NativeInputCapture {
                        editor: cx.entity(),
                    })
                    .children(lines.into_iter().map(|(index, end, line)| {
                        div()
                            .id(("editable-line", index))
                            .min_h(px(19.0))
                            .line_height(px(19.0))
                            .whitespace_nowrap()
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, window, cx| {
                                    window.focus(&this.focus);
                                    let _ = this.document.set_selection(end..end);
                                    cx.notify();
                                }),
                            )
                            .child(if line.is_empty() { " ".into() } else { line })
                    }))
                    .when(more, |surface| {
                        surface.child("… Rendering limited to first 600 lines in this prototype")
                    }),
            )
    }
}
