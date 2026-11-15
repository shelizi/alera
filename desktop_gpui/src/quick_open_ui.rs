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

// A late Windows IME event must never enter *any* previously focused input.
// Terminal and Explorer share a native HWND; OS focus alone cannot distinguish them.
fn accepts_native_text_event(has_focus: bool) -> bool {
    has_focus
}

/// Enter accepts an actual Quick Open result only after IME candidate
/// composition has finished. During zhuyin/other IME preedit, Enter belongs
/// to the native candidate picker and must not navigate away from the query.
pub(super) fn quick_open_enter_accepts_match(key: &str, composing: bool, has_match: bool) -> bool {
    has_match
        && !composing
        && (key.eq_ignore_ascii_case("enter") || key.eq_ignore_ascii_case("return"))
}

/// Quick Open keyboard navigation targets the *current* result set.
/// IME candidate selection owns Up/Down while a composition is active, and
/// there is no synthetic selection when no files match the query.
pub(super) fn quick_open_move_selection(
    key: &str,
    composing: bool,
    selected: usize,
    count: usize,
) -> Option<usize> {
    if composing || count == 0 {
        return None;
    }
    match key.to_ascii_lowercase().as_str() {
        "up" => Some(selected.min(count - 1).saturating_sub(1)),
        "down" => Some(selected.saturating_add(1).min(count - 1)),
        _ => None,
    }
}

/// Windows IME owns Backspace while a candidate is being composed.
/// Otherwise remove one Unicode grapheme or the selected range. Never slice
/// through a stale/invalid UTF-8 selection from a previous focus owner.
fn query_backspace_range(
    text: &str,
    selection: Range<usize>,
    composing: bool,
) -> Option<Range<usize>> {
    if composing
        || selection.start > selection.end
        || selection.end > text.len()
        || !text.is_char_boundary(selection.start)
        || !text.is_char_boundary(selection.end)
    {
        return None;
    }
    if !selection.is_empty() {
        return Some(selection);
    }
    let left = text[..selection.start]
        .grapheme_indices(true)
        .last()
        .map(|(i, _)| i)
        .unwrap_or(0);
    Some(left..selection.start)
}

/// Only dismiss Quick Open after the active Native Editor owns the exact
/// requested disk file. A failed open must not silently close the dialog
/// while leaving some unrelated dirty buffer focused.
pub(super) fn quick_open_target_is_active(
    requested: &std::path::Path,
    active: Option<&std::path::Path>,
) -> bool {
    let Ok(target) = dunce::canonicalize(requested) else {
        return false;
    };
    active == Some(target.as_path())
}

/// Drop only the provisional marked text when an Explorer input loses focus.
/// Previously committed query content must survive a Terminal focus switch.
fn remove_marked_preedit(text: &mut String, marked: Option<Range<usize>>) -> Option<usize> {
    let range = marked?;
    if range.start > range.end
        || range.end > text.len()
        || !text.is_char_boundary(range.start)
        || !text.is_char_boundary(range.end)
    {
        return None;
    }
    text.replace_range(range.clone(), "");
    Some(range.start)
}

/// The Profile Prompt is the only QuickOpenInput mode that preserves lines.
fn normalize_input_text(raw: &str, multiline: bool) -> String {
    if multiline {
        raw.replace("\r\n", "\n")
            .replace('\r', "\n")
            .replace('\0', "")
    } else {
        raw.replace(['\r', '\n', '\0'], " ")
    }
}

actions!(
    alera_gpui_quick_open,
    [QueryBackspace, QuerySelectAll, QueryPaste, QueryNewline]
);

pub fn bind_quick_open_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", QueryBackspace, Some("AleraGpuiQuickOpen")),
        KeyBinding::new("ctrl-a", QuerySelectAll, Some("AleraGpuiQuickOpen")),
        KeyBinding::new("cmd-a", QuerySelectAll, Some("AleraGpuiQuickOpen")),
        KeyBinding::new("ctrl-v", QueryPaste, Some("AleraGpuiQuickOpen")),
        KeyBinding::new("cmd-v", QueryPaste, Some("AleraGpuiQuickOpen")),
        KeyBinding::new(
            "backspace",
            QueryBackspace,
            Some("AleraGpuiProfileMultiline"),
        ),
        KeyBinding::new("ctrl-a", QuerySelectAll, Some("AleraGpuiProfileMultiline")),
        KeyBinding::new("cmd-a", QuerySelectAll, Some("AleraGpuiProfileMultiline")),
        KeyBinding::new("ctrl-v", QueryPaste, Some("AleraGpuiProfileMultiline")),
        KeyBinding::new("cmd-v", QueryPaste, Some("AleraGpuiProfileMultiline")),
        KeyBinding::new("enter", QueryNewline, Some("AleraGpuiProfileMultiline")),
    ]);
}

pub struct QuickOpenInput {
    focus: FocusHandle,
    text: String,
    placeholder: String,
    terminal_mode: bool,
    multiline_mode: bool,
    max_input_bytes: usize,
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
            multiline_mode: false,
            max_input_bytes: 512,
            selection: 0..0,
            marked: None,
            terminal_committed: VecDeque::new(),
            terminal_commit_gate: TerminalTextCommitGate::default(),
        }
    }

    pub fn terminal(cx: &mut Context<Self>) -> Self {
        let mut input = Self::with_placeholder("Command for selected Host session…", cx);
        input.terminal_mode = true;
        input.max_input_bytes = 4096;
        input
    }

    /// Editable single-line Host Profile command; unlike file search it
    /// supports the validated 4 KiB Host command limit.
    pub fn profile_command(cx: &mut Context<Self>) -> Self {
        let mut input = Self::with_placeholder("Host Command (4 KiB max)…", cx);
        input.max_input_bytes = 4096;
        input
    }

    /// Host-owned Custom Prompt requires real newlines, not Quick Open query
    /// filtering. This mode is opt-in and does not alter search or Terminal.
    pub fn profile_multiline(cx: &mut Context<Self>) -> Self {
        let mut input =
            Self::with_placeholder("Custom Prompt (up to 8 KiB, Enter for newline)…", cx);
        input.multiline_mode = true;
        input.max_input_bytes = 8192;
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

    /// Cancel an uncommitted Windows IME preedit without erasing the user's
    /// previously committed Explorer search query.
    pub fn discard_marked_preedit(&mut self, cx: &mut Context<Self>) {
        let marked = self.marked.take();
        if let Some(cursor) = remove_marked_preedit(&mut self.text, marked) {
            self.selection = cursor..cursor;
            cx.notify();
        }
    }

    /// Select a known local branch from the Git Graph without synthesizing
    /// keyboard/IME events. A user edit still follows the ordinary handler.
    pub fn set_query(&mut self, value: &str, cx: &mut Context<Self>) -> bool {
        if value.len() > self.max_input_bytes
            || value.contains('\0')
            || (!self.multiline_mode && value.contains(['\r', '\n']))
        {
            return false;
        }
        self.text = if self.multiline_mode {
            value.replace("\r\n", "\n").replace('\r', "\n")
        } else {
            value.to_owned()
        };
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
        let text = normalize_input_text(raw, self.multiline_mode);
        let max_length = self.max_input_bytes;
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
        let Some(range) =
            query_backspace_range(&self.text, self.selection.clone(), self.marked.is_some())
        else {
            return;
        };
        self.selection = range;
        self.replace(None, "", false, cx);
    }

    fn newline(&mut self, _: &QueryNewline, _: &mut Window, cx: &mut Context<Self>) {
        // Enter confirms an active IME candidate; only insert a newline
        // after the marked composition has been committed.
        if self.multiline_mode && self.marked.is_none() {
            self.replace(None, "\n", false, cx);
        }
    }

    fn select_all(&mut self, _: &QuerySelectAll, _: &mut Window, cx: &mut Context<Self>) {
        if self.marked.is_some() {
            return;
        }
        self.selection = 0..self.text.len();
        cx.notify();
    }

    fn paste(&mut self, _: &QueryPaste, _: &mut Window, cx: &mut Context<Self>) {
        if self.marked.is_some() {
            return;
        }
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
                self.terminal_mode,
                focused,
                text.chars().count()
            );
        }
        if !accepts_native_text_event(focused) {
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
                self.terminal_mode,
                focused,
                text.chars().count()
            );
        }
        if !accepts_native_text_event(focused) {
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
            .key_context(if self.multiline_mode {
                "AleraGpuiProfileMultiline"
            } else {
                "AleraGpuiQuickOpen"
            })
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::newline))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::paste))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    if std::env::var_os("ALERA_GPUI_TSF_TRACE").is_some() {
                        eprintln!(
                            "Alera GPUI TSF: input click terminal={} focused_before={}",
                            this.terminal_mode,
                            this.focus.is_focused(window)
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
            .when(self.multiline_mode, |input| input.min_h(px(108.0)))
            .child(if self.text.is_empty() {
                self.placeholder.to_string()
            } else {
                if self.multiline_mode {
                    self.text.clone()
                } else {
                    format!("⌕ {}", self.text)
                }
            })
            .child(QuickOpenCapture { input: cx.entity() })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        accepts_native_text_event, normalize_input_text, query_backspace_range,
        quick_open_enter_accepts_match, quick_open_move_selection, quick_open_target_is_active,
        remove_marked_preedit,
    };

    #[test]
    fn profile_multiline_preserves_newlines_without_changing_search_or_terminal() {
        assert_eq!(
            normalize_input_text("one\r\ntwo\nthree", true),
            "one\ntwo\nthree"
        );
        assert_eq!(
            normalize_input_text("one\r\ntwo\nthree", false),
            "one  two three"
        );
        assert_eq!(normalize_input_text("中\0文\n第二行", true), "中文\n第二行");
    }

    #[test]
    fn e04_quick_open_arrows_select_bounded_matches_without_stealing_ime_keys() {
        assert_eq!(quick_open_move_selection("down", false, 0, 3), Some(1));
        assert_eq!(quick_open_move_selection("down", false, 1, 3), Some(2));
        assert_eq!(quick_open_move_selection("down", false, 2, 3), Some(2));
        assert_eq!(quick_open_move_selection("up", false, 2, 3), Some(1));
        assert_eq!(quick_open_move_selection("up", false, 0, 3), Some(0));
        assert_eq!(
            quick_open_move_selection("DOWN", false, usize::MAX, 3),
            Some(2)
        );
        assert_eq!(
            quick_open_move_selection("up", false, usize::MAX, 3),
            Some(1)
        );
        assert_eq!(quick_open_move_selection("down", true, 0, 3), None);
        assert_eq!(quick_open_move_selection("up", true, 1, 3), None);
        assert_eq!(quick_open_move_selection("down", false, 0, 0), None);
        assert_eq!(quick_open_move_selection("enter", false, 0, 3), None);
    }

    #[test]
    fn e01_quick_open_dialog_only_closes_for_the_actual_target_editor() {
        let workspace = tempfile::tempdir().unwrap();
        let first = workspace.path().join("first.txt");
        let second = workspace.path().join("second.txt");
        std::fs::write(&first, "first").unwrap();
        std::fs::write(&second, "second").unwrap();
        assert!(!quick_open_target_is_active(&first, None));
        assert!(!quick_open_target_is_active(&first, Some(second.as_path())));
        assert!(quick_open_target_is_active(
            &first,
            Some(dunce::canonicalize(&first).unwrap().as_path()),
        ));
    }

    #[test]
    fn u03_search_backspace_defers_to_ime_during_zhuyin_composition() {
        let committed = "已搜尋";
        let marked = "ㄇ";
        let text = format!("{committed}{marked}");
        let caret = text.len();
        assert_eq!(query_backspace_range(&text, caret..caret, true), None);
        assert_eq!(
            query_backspace_range(&text, committed.len()..caret, true),
            None
        );
        // Once the OS commits text, Backspace removes exactly one grapheme.
        let completed = "已搜尋🌏";
        let caret = completed.len();
        assert_eq!(
            query_backspace_range(completed, caret..caret, false),
            Some("已搜尋".len()..caret)
        );
        assert_eq!(
            query_backspace_range(completed, 0..caret, false),
            Some(0..caret)
        );
        assert_eq!(query_backspace_range(completed, 1..1, false), None);
    }

    #[test]
    fn e01_quick_open_enter_ignores_ime_candidate_confirmation() {
        assert!(!quick_open_enter_accepts_match("enter", true, true));
        assert!(!quick_open_enter_accepts_match("return", true, true));
        assert!(!quick_open_enter_accepts_match("enter", false, false));
        assert!(!quick_open_enter_accepts_match("escape", false, true));
        assert!(quick_open_enter_accepts_match("Enter", false, true));
        assert!(quick_open_enter_accepts_match("return", false, true));
    }

    #[test]
    fn both_terminal_and_explorer_reject_delayed_ime_events_after_blur() {
        assert!(accepts_native_text_event(true));
        assert!(!accepts_native_text_event(false));
    }

    #[test]
    fn blur_discards_only_uncommitted_zhuyin_without_erasing_committed_query() {
        let mut query = "已搜尋ㄇ檔案".to_owned();
        let start = "已搜尋".len();
        assert_eq!(
            remove_marked_preedit(&mut query, Some(start..start + "ㄇ".len())),
            Some(start)
        );
        assert_eq!(query, "已搜尋檔案");
        assert_eq!(remove_marked_preedit(&mut query, None), None);
        assert_eq!(query, "已搜尋檔案");
        assert_eq!(remove_marked_preedit(&mut query, Some(0..1)), None);
        assert_eq!(query, "已搜尋檔案");
    }
}
