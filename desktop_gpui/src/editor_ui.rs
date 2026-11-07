//! First experimental native GPUI file editor surface.
//!
//! The OS IME enters through EntityInputHandler; the pure EditorDocument
//! owns Unicode indexing and conflict-safe writes. This deliberately is NOT
//! a virtualized production code editor: only the first 600 lines are drawn,
//! click places the caret at line end, and IME candidate rectangle/selection
//! geometry need independent visual and real Chinese IME tests.
use alera_gpui_desktop::editor::{
    byte_to_utf16, nearest_grapheme_edge, utf16_to_byte, EditorDocument, EditorDraftStore,
};
use gpui::{
    actions, div, fill, point, prelude::*, px, relative, rgb, size, App, Bounds, ClipboardItem,
    Context, CursorStyle, Element, ElementId, ElementInputHandler, Entity, EntityInputHandler,
    FocusHandle, Focusable, GlobalElementId, InspectorElementId, KeyBinding, LayoutId, MouseButton,
    Pixels, Point, ShapedLine, Style, TextRun, UTF16Selection, Window,
};
use std::collections::BTreeMap;
use std::ops::Range;

const MAX_RENDER_LINES: usize = 600;
const PANEL_BG: u32 = 0x15171b;
const EDITOR_TEXT: u32 = 0xe1e4e8;
const MUTED_TEXT: u32 = 0x9fa7b3;
const SELECTED_BG: u32 = 0x345477;
const COMPOSITION_COLOR: u32 = 0x74d9d2;
const CURSOR_COLOR: u32 = 0x8bc5ff;

actions!(
    alera_gpui_native_editor,
    [
        MoveLeft,
        MoveRight,
        MoveUp,
        MoveDown,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        MoveHome,
        MoveEnd,
        SelectHome,
        SelectEnd,
        UndoEdit,
        RedoEdit,
        Backspace,
        Delete,
        Newline,
        SelectAll,
        SaveFile,
        Paste,
        Copy,
        Cut
    ]
);

pub fn bind_editor_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("left", MoveLeft, Some("AleraGpuiEditor")),
        KeyBinding::new("right", MoveRight, Some("AleraGpuiEditor")),
        KeyBinding::new("up", MoveUp, Some("AleraGpuiEditor")),
        KeyBinding::new("down", MoveDown, Some("AleraGpuiEditor")),
        KeyBinding::new("shift-left", SelectLeft, Some("AleraGpuiEditor")),
        KeyBinding::new("shift-right", SelectRight, Some("AleraGpuiEditor")),
        KeyBinding::new("shift-up", SelectUp, Some("AleraGpuiEditor")),
        KeyBinding::new("shift-down", SelectDown, Some("AleraGpuiEditor")),
        KeyBinding::new("home", MoveHome, Some("AleraGpuiEditor")),
        KeyBinding::new("end", MoveEnd, Some("AleraGpuiEditor")),
        KeyBinding::new("shift-home", SelectHome, Some("AleraGpuiEditor")),
        KeyBinding::new("shift-end", SelectEnd, Some("AleraGpuiEditor")),
        KeyBinding::new("ctrl-z", UndoEdit, Some("AleraGpuiEditor")),
        KeyBinding::new("cmd-z", UndoEdit, Some("AleraGpuiEditor")),
        KeyBinding::new("ctrl-y", RedoEdit, Some("AleraGpuiEditor")),
        KeyBinding::new("ctrl-shift-z", RedoEdit, Some("AleraGpuiEditor")),
        KeyBinding::new("cmd-shift-z", RedoEdit, Some("AleraGpuiEditor")),
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
    line_geometry: BTreeMap<usize, EditorLineGeometry>,
    mouse_selecting: bool,
}

/// Geometry is produced by GPUI's actual native font shaper during paint.
/// It is not inferred from byte offsets, fixed-width guesses, or the scroll
/// position. ContentMask is the visible part of the scroll container.
struct EditorLineGeometry {
    start: usize,
    end: usize,
    bounds: Bounds<Pixels>,
    clip: Bounds<Pixels>,
    shaped: ShapedLine,
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
            line_geometry: BTreeMap::new(),
            mouse_selecting: false,
        }
    }

    pub fn has_unsaved_changes(&self) -> bool {
        self.document.is_dirty()
    }

    fn pointer_index_at(&self, position: Point<Pixels>) -> Option<usize> {
        let y = f32::from(position.y);
        let geometry = self
            .line_geometry
            .values()
            // Skip off-screen rows. They may still have layout geometry
            // because the experimental renderer isn't virtualized yet.
            .filter(|row| {
                row.bounds.bottom() > row.clip.top()
                    && row.bounds.top() < row.clip.bottom()
                    && row.start <= row.end
            })
            .min_by(|a, b| {
                let distance = |row: &EditorLineGeometry| {
                    let top = f32::from(row.bounds.top().max(row.clip.top()));
                    let bottom = f32::from(row.bounds.bottom().min(row.clip.bottom()));
                    if y < top {
                        top - y
                    } else if y > bottom {
                        y - bottom
                    } else {
                        0.0
                    }
                };
                distance(a).total_cmp(&distance(b))
            })?;
        let text = self.document.content().get(geometry.start..geometry.end)?;
        let x = position.x - geometry.bounds.left();
        let guessed = geometry.shaped.closest_index_for_x(x).min(text.len());
        let grapheme = nearest_grapheme_edge(text, guessed, f32::from(x), |index| {
            f32::from(geometry.shaped.x_for_index(index))
        });
        Some(geometry.start + grapheme)
    }

    fn pointer_down(&mut self, event: &gpui::MouseDownEvent, cx: &mut Context<Self>) {
        let Some(offset) = self.pointer_index_at(event.position) else {
            return;
        };
        let result = if event.modifiers.shift {
            self.document.extend_pointer_selection(offset)
        } else {
            self.document.set_pointer_caret(offset)
        };
        if result.is_ok() {
            self.mouse_selecting = true;
        } else if let Err(error) = result {
            self.status = format!("Pointer input blocked: {error}");
        }
        cx.notify();
    }

    fn pointer_move(&mut self, event: &gpui::MouseMoveEvent, cx: &mut Context<Self>) {
        if !self.mouse_selecting {
            return;
        }
        if let Some(offset) = self.pointer_index_at(event.position) {
            if self.document.extend_pointer_selection(offset).is_ok() {
                cx.notify();
            }
        }
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
    fn move_up(&mut self, _: &MoveUp, _: &mut Window, cx: &mut Context<Self>) {
        self.document.move_up(false);
        cx.notify();
    }
    fn move_down(&mut self, _: &MoveDown, _: &mut Window, cx: &mut Context<Self>) {
        self.document.move_down(false);
        cx.notify();
    }
    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.document.select_left();
        cx.notify();
    }
    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.document.select_right();
        cx.notify();
    }
    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.document.move_up(true);
        cx.notify();
    }
    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.document.move_down(true);
        cx.notify();
    }
    fn move_home(&mut self, _: &MoveHome, _: &mut Window, cx: &mut Context<Self>) {
        self.document.move_home(false);
        cx.notify();
    }
    fn move_end(&mut self, _: &MoveEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.document.move_end(false);
        cx.notify();
    }
    fn select_home(&mut self, _: &SelectHome, _: &mut Window, cx: &mut Context<Self>) {
        self.document.move_home(true);
        cx.notify();
    }
    fn select_end(&mut self, _: &SelectEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.document.move_end(true);
        cx.notify();
    }
    fn undo_edit(&mut self, _: &UndoEdit, _: &mut Window, cx: &mut Context<Self>) {
        if self.document.undo() {
            self.edit_result(Ok(()), cx);
        }
    }
    fn redo_edit(&mut self, _: &RedoEdit, _: &mut Window, cx: &mut Context<Self>) {
        if self.document.redo() {
            self.edit_result(Ok(()), cx);
        }
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
        if self.document.marked().is_some() {
            return;
        }
        let result = self.document.replace(None, "\n");
        self.edit_result(result, cx);
    }
    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        if self.document.marked().is_some() {
            return;
        }
        let size = self.document.content().len();
        let result = self.document.set_selection(0..size);
        if let Err(error) = result {
            self.status = format!("Selection rejected: {error}");
        }
        cx.notify();
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
            reversed: self.document.selection_reversed(),
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.document.utf16_marked()
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.document.unmark();
        self.edit_result(Ok(()), cx);
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
        range_utf16: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let byte = utf16_to_byte(self.document.content(), range_utf16.start);
        let geometry = self.line_geometry.values().find(|row| {
            row.start <= byte
                && byte <= row.end
                && row.bounds.bottom() > row.clip.top()
                && row.bounds.top() < row.clip.bottom()
        })?;
        let x = geometry.bounds.left() + geometry.shaped.x_for_index(byte - geometry.start);
        if x < geometry.clip.left() || x > geometry.clip.right() {
            // Do not place the OS IME candidate window at an offscreen caret.
            return None;
        }
        let top = geometry.bounds.top().max(geometry.clip.top());
        let bottom = geometry.bounds.bottom().min(geometry.clip.bottom());
        Some(Bounds::new(point(x, top), size(px(2.0), bottom - top)))
    }
    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        self.pointer_index_at(point)
            .map(|byte| byte_to_utf16(self.document.content(), byte))
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

/// Transparent overlay occupying exactly the painted row. Unlike heuristic
/// character-width hit testing, its native ShapedLine uses the inherited
/// font and font size from the same GPUI text context as the visible text.
struct EditorLineMetrics {
    editor: Entity<NativeEditor>,
    index: usize,
    start: usize,
    end: usize,
    text: String,
    caret: usize,
}

impl IntoElement for EditorLineMetrics {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for EditorLineMetrics {
    type RequestLayoutState = ();
    type PrepaintState = Option<ShapedLine>;

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
        style.size.height = relative(1.0).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        _: &mut App,
    ) -> Option<ShapedLine> {
        let style = window.text_style();
        let run = TextRun {
            len: self.text.len(),
            font: style.font(),
            color: style.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let font_size = style.font_size.to_pixels(window.rem_size());
        Some(
            window
                .text_system()
                .shape_line(self.text.clone().into(), font_size, &[run], None),
        )
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        shaped: &mut Option<ShapedLine>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(line) = shaped.take() else { return };
        let clip = window.content_mask().bounds;
        // Paint a 1px native caret without inserting a fake glyph into
        // the visible text, which previously shifted all later glyphs.
        if self.caret >= self.start
            && self.caret <= self.end
            && self.editor.read(cx).document.selection().is_empty()
            && self.editor.read(cx).focus_handle(cx).is_focused(window)
        {
            let local = self.caret - self.start;
            let cursor_x = bounds.left() + line.x_for_index(local);
            if bounds.bottom() > clip.top() && bounds.top() < clip.bottom() {
                window.paint_quad(fill(
                    Bounds::new(
                        point(cursor_x, bounds.top()),
                        size(px(1.5), bounds.size.height),
                    ),
                    rgb(CURSOR_COLOR),
                ));
            }
        }
        self.editor.update(cx, |editor, _| {
            editor.line_geometry.insert(
                self.index,
                EditorLineGeometry {
                    start: self.start,
                    end: self.end,
                    bounds,
                    clip,
                    shaped: line,
                },
            );
        });
    }
}

impl Render for NativeEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.line_geometry.clear();
        let text = self.document.content();
        let caret = self.document.cursor();
        let selection = self.document.selection();
        let marked = self.document.marked();
        let (line_index, column_index) = self.document.line_column();
        let (undo_count, redo_count) = self.document.history_depth();
        let mut offset = 0;
        let lines: Vec<_> = text
            .split('\n')
            .take(MAX_RENDER_LINES)
            .enumerate()
            .map(|(i, line)| {
                let start = offset;
                let end = start + line.len();
                let mut display = line.to_owned();
                let mut highlights = Vec::new();
                if let Some(composition) = &marked {
                    let from = composition.start.max(start);
                    let to = composition.end.min(end);
                    if from < to {
                        highlights.push((
                            from - start..to - start,
                            gpui::HighlightStyle {
                                underline: Some(gpui::UnderlineStyle {
                                    thickness: px(1.0),
                                    color: Some(rgb(COMPOSITION_COLOR).into()),
                                    wavy: false,
                                }),
                                ..Default::default()
                            },
                        ));
                    }
                } else if !selection.is_empty() {
                    let from = selection.start.max(start);
                    let to = selection.end.min(end);
                    if from < to {
                        highlights.push((
                            from - start..to - start,
                            gpui::HighlightStyle {
                                background_color: Some(rgb(SELECTED_BG).into()),
                                ..Default::default()
                            },
                        ));
                    }
                    // Make selected newline characters visible, including
                    // empty lines and multi-line selection boundaries.
                    if end < text.len() && selection.start <= end && selection.end > end {
                        let from = display.len();
                        display.push('↵');
                        highlights.push((
                            from..display.len(),
                            gpui::HighlightStyle {
                                background_color: Some(rgb(SELECTED_BG).into()),
                                ..Default::default()
                            },
                        ));
                    }
                }
                offset = end + 1;
                (
                    i,
                    start,
                    end,
                    line.to_owned(),
                    gpui::StyledText::new(display).with_highlights(highlights),
                )
            })
            .collect();
        let more = text.split('\n').count() > MAX_RENDER_LINES;
        let status = format!(
            "{} · Ln {} Col {} · Undo {} / Redo {} · {}{}",
            self.status,
            line_index + 1,
            column_index + 1,
            undo_count,
            redo_count,
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
            .on_action(cx.listener(Self::move_up))
            .on_action(cx.listener(Self::move_down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::move_home))
            .on_action(cx.listener(Self::move_end))
            .on_action(cx.listener(Self::select_home))
            .on_action(cx.listener(Self::select_end))
            .on_action(cx.listener(Self::undo_edit))
            .on_action(cx.listener(Self::redo_edit))
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
            .on_mouse_move(cx.listener(|this, event, _, cx| {
                this.pointer_move(event, cx);
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.mouse_selecting = false),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.mouse_selecting = false),
            )
            .child(
                div()
                    .px_3()
                    .py_1()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .text_sm()
                    .text_color(rgb(MUTED_TEXT))
                    .child(div().flex_1().min_w_0().overflow_hidden().child(status))
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .child(
                                div()
                                    .id("editor-undo-click")
                                    .cursor_pointer()
                                    .text_color(rgb(if undo_count > 0 {
                                        EDITOR_TEXT
                                    } else {
                                        MUTED_TEXT
                                    }))
                                    .child("↶ Undo")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if this.document.undo() {
                                            this.edit_result(Ok(()), cx);
                                        }
                                    })),
                            )
                            .child(
                                div()
                                    .id("editor-redo-click")
                                    .cursor_pointer()
                                    .text_color(rgb(if redo_count > 0 {
                                        EDITOR_TEXT
                                    } else {
                                        MUTED_TEXT
                                    }))
                                    .child("↷ Redo")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if this.document.redo() {
                                            this.edit_result(Ok(()), cx);
                                        }
                                    })),
                            ),
                    ),
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
                    .children(lines.into_iter().map(|(index, start, end, raw, line)| {
                        div()
                            .id(("editable-line", index))
                            .relative()
                            .min_h(px(19.0))
                            .line_height(px(19.0))
                            .whitespace_nowrap()
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                                    window.focus(&this.focus);
                                    this.pointer_down(event, cx);
                                }),
                            )
                            .child(line)
                            .child(div().absolute().inset_0().child(EditorLineMetrics {
                                editor: cx.entity(),
                                index,
                                start,
                                end,
                                text: raw,
                                caret,
                            }))
                    }))
                    .when(more, |surface| {
                        surface.child("… Rendering limited to first 600 lines in this prototype")
                    }),
            )
    }
}
