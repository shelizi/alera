//! Minimal GPUI-native Unicode/IME input for read-only project file search.
//! This field never edits source files; opening a result is handled by the
//! existing canonical-path-checked WorkbenchModel.
use alera_gpui_desktop::editor::{byte_to_utf16, utf16_to_byte};
use gpui::{
    actions, div, prelude::*, px, relative, rgb, App, Bounds, Context, Element, ElementId,
    ElementInputHandler, Entity, EntityInputHandler, FocusHandle, Focusable, GlobalElementId,
    InspectorElementId, KeyBinding, LayoutId, MouseButton, Pixels, Point, Style, UTF16Selection,
    Window,
};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

actions!(
    alera_gpui_quick_open,
    [QueryBackspace, QuerySelectAll, QueryPaste]
);

pub fn bind_quick_open_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", QueryBackspace, Some("AleraGpuiQuickOpen")),
        KeyBinding::new("ctrl-a", QuerySelectAll, Some("AleraGpuiQuickOpen")),
        KeyBinding::new("cmd-a", QuerySelectAll, Some("AleraGpuiQuickOpen")),
        KeyBinding::new("ctrl-v", QueryPaste, Some("AleraGpuiQuickOpen")),
        KeyBinding::new("cmd-v", QueryPaste, Some("AleraGpuiQuickOpen")),
        KeyBinding::new("backspace", QueryBackspace, Some("AleraGpuiTerminalInput")),
        KeyBinding::new("ctrl-a", QuerySelectAll, Some("AleraGpuiTerminalInput")),
        KeyBinding::new("cmd-a", QuerySelectAll, Some("AleraGpuiTerminalInput")),
        KeyBinding::new("ctrl-v", QueryPaste, Some("AleraGpuiTerminalInput")),
        KeyBinding::new("cmd-v", QueryPaste, Some("AleraGpuiTerminalInput")),
    ]);
}

pub struct QuickOpenInput {
    focus: FocusHandle,
    text: String,
    placeholder: &'static str,
    terminal_mode: bool,
    selection: Range<usize>,
    marked: Option<Range<usize>>,
}

impl QuickOpenInput {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self::with_placeholder("Type to filter files…", cx)
    }

    pub fn with_placeholder(placeholder: &'static str, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            text: String::new(),
            placeholder,
            terminal_mode: false,
            selection: 0..0,
            marked: None,
        }
    }

    pub fn terminal(cx: &mut Context<Self>) -> Self {
        let mut input = Self::with_placeholder("Command for selected Host session…", cx);
        input.terminal_mode = true;
        input
    }

    pub fn query(&self) -> &str {
        &self.text
    }

    pub fn composing(&self) -> bool {
        self.marked.is_some()
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.text.clear();
        self.selection = 0..0;
        self.marked = None;
        cx.notify();
    }

    /// Select a known local branch from the Git Graph without synthesizing
    /// keyboard/IME events. A user edit still follows the ordinary handler.
    pub fn set_query(&mut self, value: &str, cx: &mut Context<Self>) -> bool {
        if value.len() > 512 || value.contains(['\r', '\n', '\0']) {
            return false;
        }
        self.text = value.to_owned();
        self.selection = self.text.len()..self.text.len();
        self.marked = None;
        cx.notify();
        true
    }

    fn replace(
        &mut self,
        range: Option<Range<usize>>,
        raw: &str,
        mark: bool,
        cx: &mut Context<Self>,
    ) {
        let range = range
            .map(|range| {
                utf16_to_byte(&self.text, range.start)..utf16_to_byte(&self.text, range.end)
            })
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection.clone());
        if range.start > range.end
            || range.end > self.text.len()
            || !self.text.is_char_boundary(range.start)
            || !self.text.is_char_boundary(range.end)
        {
            return;
        }
        let text = raw.replace(['\r', '\n', '\0'], " ");
        if self.text.len() - (range.end - range.start) + text.len() > 512 {
            return;
        }
        self.text.replace_range(range.clone(), &text);
        let end = range.start + text.len();
        self.selection = end..end;
        self.marked = mark.then_some(range.start..end).filter(|r| !r.is_empty());
        cx.notify();
    }

    fn backspace(&mut self, _: &QueryBackspace, _: &mut Window, cx: &mut Context<Self>) {
        let range = if self.selection.is_empty() {
            let left = self.text[..self.selection.start]
                .grapheme_indices(true)
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            left..self.selection.start
        } else {
            self.selection.clone()
        };
        self.selection = range;
        self.replace(None, "", false, cx);
    }

    fn select_all(&mut self, _: &QuerySelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.selection = 0..self.text.len();
        cx.notify();
    }

    fn paste(&mut self, _: &QueryPaste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace(None, &text, false, cx);
        }
    }
}

impl Focusable for QuickOpenInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl EntityInputHandler for QuickOpenInput {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let start = utf16_to_byte(&self.text, range.start);
        let end = utf16_to_byte(&self.text, range.end).max(start);
        *actual = Some(byte_to_utf16(&self.text, start)..byte_to_utf16(&self.text, end));
        Some(self.text[start..end].to_owned())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: byte_to_utf16(&self.text, self.selection.start)
                ..byte_to_utf16(&self.text, self.selection.end),
            reversed: false,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked
            .as_ref()
            .map(|r| byte_to_utf16(&self.text, r.start)..byte_to_utf16(&self.text, r.end))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace(range, text, false, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace(range, text, true, cx);
    }

    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        Some(bounds)
    }

    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(byte_to_utf16(&self.text, self.text.len()))
    }
}

struct QuickOpenCapture {
    input: Entity<QuickOpenInput>,
}

impl Element for QuickOpenCapture {
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
        let focus = self.input.read(cx).focus.clone();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
    }
}

impl IntoElement for QuickOpenCapture {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

impl Render for QuickOpenInput {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("quick-open-query")
            .key_context(if self.terminal_mode {
                "AleraGpuiTerminalInput"
            } else {
                "AleraGpuiQuickOpen"
            })
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::paste))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    window.focus(&this.focus, cx);
                }),
            )
            .flex()
            .flex_col()
            .bg(rgb(0x303741))
            .px_2()
            .py_1()
            .text_sm()
            .child(if self.text.is_empty() {
                self.placeholder.to_string()
            } else {
                format!("⌕ {}", self.text)
            })
            .child(QuickOpenCapture { input: cx.entity() })
    }
}
