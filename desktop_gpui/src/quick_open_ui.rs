//! Minimal GPUI-native Unicode/IME input for read-only project file search.
//! This field never edits source files; opening a result is handled by the
//! existing canonical-path-checked WorkbenchModel.
use alera_gpui_desktop::editor::{byte_to_utf16, utf16_to_byte};
use alera_gpui_desktop::terminal::TerminalTextCommitGate;
use gpui::{
    actions, div, prelude::*, px, relative, rgb, App, Bounds, Context, Element, ElementId,
    ElementInputHandler, Entity, EntityInputHandler, FocusHandle, Focusable, GlobalElementId,
    InspectorElementId, KeyBinding, LayoutId, MouseButton, Pixels, Point, Style, UTF16Selection,
    Window,
};
use std::collections::VecDeque;
use std::ops::Range;
use std::time::Instant;
use unicode_segmentation::UnicodeSegmentation;

// A delayed Windows TextInput callback must not enqueue a Terminal commit
// after GPUI focus moved to Explorer, Settings, or the other pane.
fn accepts_native_text_event(terminal_mode: bool, has_focus: bool) -> bool {
    !terminal_mode || has_focus
}

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
    ]);
}

pub struct QuickOpenInput {
    focus: FocusHandle,
    text: String,
    placeholder: String,
    terminal_mode: bool,
    selection: Range<usize>,
    marked: Option<Range<usize>>,
    terminal_committed: VecDeque<String>,
    terminal_commit_gate: TerminalTextCommitGate,
}

impl QuickOpenInput {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self::with_placeholder("Type to filter files…", cx)
    }

    pub fn with_placeholder(placeholder: impl Into<String>, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            text: String::new(),
            placeholder: placeholder.into(),
            terminal_mode: false,
            selection: 0..0,
            marked: None,
            terminal_committed: VecDeque::new(),
            terminal_commit_gate: TerminalTextCommitGate::default(),
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

    /// Change a localized hint without changing the user's search query.
    pub fn set_placeholder(&mut self, value: impl Into<String>, cx: &mut Context<Self>) {
        self.placeholder = value.into();
        cx.notify();
    }

    pub fn composing(&self) -> bool {
        self.marked.is_some()
    }

    /// In Terminal mode the input handler is a nonvisible IME sink; only
    /// completed text leaves it. Provisional phonetic/marked text never
    /// enters the Host PTY.
    pub fn take_terminal_committed(&mut self) -> Vec<String> {
        self.terminal_committed.drain(..).collect()
    }

    /// Called only after an ordinary printable ASCII KeyDown was actually
    /// forwarded to the Host input queue. It prevents a duplicate Windows
    /// TextInput event for that one key from being echoed twice.
    pub fn note_terminal_ascii_key(&mut self, text: &str) {
        if self.terminal_mode {
            self.terminal_commit_gate
                .note_raw_ascii_key(text, Instant::now());
        }
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.text.clear();
        self.selection = 0..0;
        self.marked = None;
        self.terminal_committed.clear();
        // A new tab/workspace must never consume a stale KeyDown/TextInput
        // duplicate marker from the previous Host session.
        self.terminal_commit_gate = TerminalTextCommitGate::default();
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
        let max_length = if self.terminal_mode { 4096 } else { 512 };
        if self.text.len() - (range.end - range.start) + text.len() > max_length {
            return;
        }
        let was_marked = self.marked.is_some();
        self.text.replace_range(range.clone(), &text);
        let end = range.start + text.len();
        self.selection = end..end;
        self.marked = mark.then_some(range.start..end).filter(|r| !r.is_empty());
        if self.terminal_mode && !mark && !self.text.is_empty() {
            let committed = std::mem::take(&mut self.text);
            // KeyDown and TextInput are both valid input sources. Only drop
            // ASCII that the same focused input already sent via KeyDown;
            // pasted / Unicode-injected ASCII with no KeyDown must survive.
            if self
                .terminal_commit_gate
                .should_forward(&committed, was_marked, Instant::now())
            {
                self.terminal_committed.push_back(committed);
            }
            self.selection = 0..0;
            self.marked = None;
        }
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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focused = self.focus.is_focused(window);
        if std::env::var_os("ALERA_GPUI_TSF_TRACE").is_some() {
            // Never print user input, only focus/length metadata.
            eprintln!(
                "Alera GPUI TSF: input event=commit terminal={} focused={} chars={}",
                self.terminal_mode, focused, text.chars().count()
            );
        }
        if !accepts_native_text_event(self.terminal_mode, focused) {
            if std::env::var_os("ALERA_GPUI_TSF_TRACE").is_some() {
                eprintln!("Alera GPUI TSF: drop unfocused Terminal text commit");
            }
            return;
        }
        self.replace(range, text, false, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focused = self.focus.is_focused(window);
        if std::env::var_os("ALERA_GPUI_TSF_TRACE").is_some() {
            eprintln!(
                "Alera GPUI TSF: input event=preedit terminal={} focused={} chars={}",
                self.terminal_mode, focused, text.chars().count()
            );
        }
        if !accepts_native_text_event(self.terminal_mode, focused) {
            if std::env::var_os("ALERA_GPUI_TSF_TRACE").is_some() {
                eprintln!("Alera GPUI TSF: drop unfocused Terminal preedit");
            }
            return;
        }
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
        if self.terminal_mode {
            // Keep a full-width native TextInput capture for Windows IME
            // candidate placement. It remains only 1px tall and paints
            // no visible text or command bar.
            return div()
                .id("terminal-ime-capture")
                .key_context("AleraGpuiTerminalInput")
                .track_focus(&self.focus)
                .w_full()
                .h(px(1.0))
                .child(QuickOpenCapture { input: cx.entity() })
                .into_any_element();
        }
        div()
            .id("quick-open-query")
            .key_context("AleraGpuiQuickOpen")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::paste))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    if std::env::var_os("ALERA_GPUI_TSF_TRACE").is_some() {
                        eprintln!(
                            "Alera GPUI TSF: input click terminal={} focused_before={}",
                            this.terminal_mode, this.focus.is_focused(window)
                        );
                    }
                    window.focus(&this.focus, cx);
                }),
            )
            .flex()
            .flex_col()
            .bg(rgb(alera_gpui_desktop::visual_tokens::ACTIVE))
            .px_2()
            .py_1()
            .text_sm()
            .child(if self.text.is_empty() {
                self.placeholder.to_string()
            } else {
                format!("⌕ {}", self.text)
            })
            .child(QuickOpenCapture { input: cx.entity() })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::accepts_native_text_event;

    #[test]
    fn terminal_rejects_delayed_ime_after_blur_but_explorer_is_unchanged() {
        assert!(accepts_native_text_event(true, true));
        assert!(!accepts_native_text_event(true, false));
        assert!(accepts_native_text_event(false, true));
        assert!(accepts_native_text_event(false, false));
    }
}
