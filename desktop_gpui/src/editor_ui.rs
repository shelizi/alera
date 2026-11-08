//! First experimental native GPUI file editor surface.
//!
//! The OS IME enters through EntityInputHandler; the pure EditorDocument
//! owns Unicode indexing and conflict-safe writes. A native uniform_list
//! virtualizes displayed rows beyond line 600; this still is NOT a full IDE
//! editor. Mouse and Traditional Chinese IME need independent GUI acceptance.
use alera_gpui_desktop::editor::{
    byte_to_utf16, edge_scroll_offset, utf16_to_byte, EditorDocument, EditorLineIndex,
};
use alera_gpui_desktop::editor_session::{OpenEffect, RECOVERY_PLAINTEXT_NOTICE};
use alera_gpui_desktop::editor_tabs::{EditorSaveGuard, EditorTabDragIdentity, EditorTabs};
use alera_gpui_desktop::visible_line_hit::{
    apply_pointer_hit, VisibleLineGeometry, VisibleLineHitSet,
};
use alera_gpui_desktop::visual_tokens::{BG as PANEL_BG, MUTED as MUTED_TEXT, TEXT as EDITOR_TEXT};
use gpui::{
    actions, div, fill, point, prelude::*, px, relative, rgb, size, uniform_list, App, Bounds,
    ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, Focusable, GlobalElementId, InspectorElementId, KeyBinding,
    LayoutId, MouseButton, PathPromptOptions, Pixels, Point, ScrollHandle, ScrollStrategy,
    ShapedLine, Style, TextRun, UTF16Selection, UniformListScrollHandle, Window,
};
use std::collections::BTreeMap;
use std::io;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

#[path = "editor_find_ui.rs"]
mod editor_find_ui;
use editor_find_ui::EditorFindUi;

const EDITOR_LINE_HEIGHT: f32 = 19.0;
// Match the Flutter desktop's neutral theme rather than the old blue-tinted
// GPUI prototype. Keep selection distinct from the editor surface.
const SELECTED_BG: u32 = 0x3b3b3b;
const COMPOSITION_COLOR: u32 = 0xe0e0e0;
const CURSOR_COLOR: u32 = EDITOR_TEXT;

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
        DocumentStart,
        DocumentEnd,
        SelectDocumentStart,
        SelectDocumentEnd,
        UndoEdit,
        RedoEdit,
        Backspace,
        Delete,
        Newline,
        SelectAll,
        SaveFile,
        Paste,
        Copy,
        Cut,
        NextEditorTab,
        PreviousEditorTab,
        NewUntitledTab,
        CloseCurrentEditorTab
    ]
);

pub fn bind_editor_keys(cx: &mut App) {
    editor_find_ui::bind_find_keys(cx);
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
        KeyBinding::new("ctrl-home", DocumentStart, Some("AleraGpuiEditor")),
        KeyBinding::new("ctrl-end", DocumentEnd, Some("AleraGpuiEditor")),
        KeyBinding::new(
            "ctrl-shift-home",
            SelectDocumentStart,
            Some("AleraGpuiEditor"),
        ),
        KeyBinding::new("ctrl-shift-end", SelectDocumentEnd, Some("AleraGpuiEditor")),
        KeyBinding::new("cmd-up", DocumentStart, Some("AleraGpuiEditor")),
        KeyBinding::new("cmd-down", DocumentEnd, Some("AleraGpuiEditor")),
        KeyBinding::new("cmd-shift-up", SelectDocumentStart, Some("AleraGpuiEditor")),
        KeyBinding::new("cmd-shift-down", SelectDocumentEnd, Some("AleraGpuiEditor")),
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
        KeyBinding::new("ctrl-tab", NextEditorTab, Some("AleraGpuiEditor")),
        KeyBinding::new("ctrl-shift-tab", PreviousEditorTab, Some("AleraGpuiEditor")),
        KeyBinding::new("cmd-alt-right", NextEditorTab, Some("AleraGpuiEditor")),
        KeyBinding::new("cmd-alt-left", PreviousEditorTab, Some("AleraGpuiEditor")),
        KeyBinding::new("ctrl-n", NewUntitledTab, Some("AleraGpuiEditor")),
        KeyBinding::new("cmd-n", NewUntitledTab, Some("AleraGpuiEditor")),
        KeyBinding::new("ctrl-w", CloseCurrentEditorTab, Some("AleraGpuiEditor")),
        KeyBinding::new("cmd-w", CloseCurrentEditorTab, Some("AleraGpuiEditor")),
    ]);
}

#[derive(Clone)]
struct EditorTabDrag {
    identity: Option<EditorTabDragIdentity>,
}

struct EditorTabDragPreview {
    title: String,
}

impl gpui::Render for EditorTabDragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
        div()
            .bg(rgb(PANEL_BG))
            .px_2()
            .py_1()
            .text_sm()
            .child(self.title.clone())
    }
}

pub struct NativeEditor {
    find: EditorFindUi,
    tabs: EditorTabs,
    tab_context: Option<EditorSaveGuard>,
    workspace_root: PathBuf,
    picking_save_as: bool,
    focus: FocusHandle,
    status: String,
    line_geometry: BTreeMap<usize, EditorLineGeometry>,
    scroll: ScrollHandle,
    list_scroll: UniformListScrollHandle,
    mouse_selecting: bool,
    last_drag_position: Option<Point<Pixels>>,
    edge_scroll_running: bool,
    edge_scroll_ticks: u16,
}

/// Geometry is produced by GPUI's actual native font shaper during paint.
/// It is not inferred from byte offsets, fixed-width guesses, or the scroll
/// position. ContentMask is the visible part of the scroll container.
fn painted_frame(row: &EditorLineGeometry) -> VisibleLineGeometry {
    VisibleLineGeometry {
        start: row.start,
        end: row.end,
        top: f32::from(row.bounds.top()),
        bottom: f32::from(row.bounds.bottom()),
        left: f32::from(row.bounds.left()),
        clip_top: f32::from(row.clip.top()),
        clip_bottom: f32::from(row.clip.bottom()),
        clip_left: f32::from(row.clip.left()),
        clip_right: f32::from(row.clip.right()),
    }
}

fn same_painted_row(row: &EditorLineGeometry, frame: &VisibleLineGeometry) -> bool {
    row.start == frame.start
        && row.end == frame.end
        && f32::from(row.bounds.top()) == frame.top
        && f32::from(row.bounds.left()) == frame.left
}

struct EditorLineGeometry {
    start: usize,
    end: usize,
    bounds: Bounds<Pixels>,
    clip: Bounds<Pixels>,
    shaped: ShapedLine,
}

impl NativeEditor {
    pub fn new(tabs: EditorTabs, cx: &mut Context<Self>) -> Self {
        // Retain the workspace even after the user closes the final tab.
        let workspace_root = tabs
            .document()
            .expect("native editor requires an initial document")
            .root()
            .to_path_buf();
        let recovered = tabs.document().is_some_and(EditorDocument::is_dirty);
        let initial_line = tabs
            .document()
            .map(|document| document.line_column().0)
            .unwrap_or(0);
        let list_scroll = UniformListScrollHandle::new();
        let scroll = list_scroll.0.borrow().base_handle.clone();
        // File-backed documents currently open with the caret at EOF.
        // A long-file editor must reveal that actual position on first paint.
        list_scroll.scroll_to_item(initial_line, ScrollStrategy::Center);
        Self {
            find: EditorFindUi::new(cx),
            tabs,
            tab_context: None,
            workspace_root,
            picking_save_as: false,
            focus: cx.focus_handle(),
            status: if recovered {
                "Recovered local unsaved edit; review before saving".into()
            } else {
                "Native UTF-8 text editor (experimental)".into()
            },
            line_geometry: BTreeMap::new(),
            scroll,
            list_scroll,
            mouse_selecting: false,
            last_drag_position: None,
            edge_scroll_running: false,
            edge_scroll_ticks: 0,
        }
    }

    pub fn document(&self) -> &EditorDocument {
        self.doc()
    }

    pub fn status_text(&self) -> &str {
        &self.status
    }

    pub fn discard_armed(&self) -> bool {
        self.tabs.discard_armed()
    }

    pub fn open_in_place(
        &mut self,
        root: &Path,
        path: &Path,
        cx: &mut Context<Self>,
    ) -> io::Result<OpenEffect> {
        let effect = self.tabs.open_file(root, path)?;
        self.workspace_root = root.to_path_buf();
        if !matches!(effect, OpenEffect::AlreadyOpen) {
            self.line_geometry.clear();
            self.mouse_selecting = false;
            self.status = if matches!(effect, OpenEffect::Focused) {
                effect.status()
            } else if self.doc().is_dirty() {
                "Recovered local unsaved edit; review before saving".into()
            } else {
                "Native UTF-8 text editor (experimental)".into()
            };
            self.reveal_caret();
            cx.notify();
        }
        Ok(effect)
    }

    /// Locate a content-search hit in the active native editor. The path
    /// guard prevents a failed open from moving another document's caret.
    pub fn navigate_to_content_hit(
        &mut self,
        path: &Path,
        line: usize,
        column: usize,
        cx: &mut Context<Self>,
    ) -> io::Result<()> {
        if self.doc().path() != Some(path) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Search result does not match the active editor",
            ));
        }
        self.doc_mut().navigate_to_content_hit(line, column)?;
        self.stop_drag();
        self.line_geometry.clear();
        self.reveal_caret();
        self.status = format!("Navigated to content search result {line}:{column}");
        cx.notify();
        Ok(())
    }

    pub fn open_scratch(&mut self, root: &Path, cx: &mut Context<Self>) -> io::Result<OpenEffect> {
        let effect = self.tabs.open_scratch(root)?;
        self.tab_context = None;
        self.workspace_root = root.to_path_buf();
        self.line_geometry.clear();
        self.mouse_selecting = false;
        self.status = effect.status();
        self.reveal_caret();
        cx.notify();
        Ok(effect)
    }

    pub fn activate_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        self.tab_context = None;
        if self.tabs.activate(index) {
            self.line_geometry.clear();
            self.mouse_selecting = false;
            self.status = "Switched editor file".into();
            self.reveal_caret();
            cx.notify();
        }
    }

    fn new_untitled_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let root = self.workspace_root.clone();
        match self.open_scratch(&root, cx) {
            Ok(_) => window.focus(&self.focus),
            Err(error) => {
                self.status = format!("Cannot create new file: {error}");
                cx.notify();
            }
        }
    }

    fn new_untitled_action(
        &mut self,
        _: &NewUntitledTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.new_untitled_tab(window, cx);
    }

    fn close_current_tab_action(
        &mut self,
        _: &CloseCurrentEditorTab,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(index) = self.tabs.active_index() {
            // Ctrl+W closes clean tabs. Dirty drafts are never discarded by
            // a shortcut; the tab's explicit Close / Confirm remains required.
            self.close_editor_tab(index, false, cx);
        }
    }

    fn next_editor_tab(&mut self, _: &NextEditorTab, _: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.activate_next() {
            self.on_tab_navigation(cx);
        }
    }

    fn previous_editor_tab(
        &mut self,
        _: &PreviousEditorTab,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tabs.activate_previous() {
            self.on_tab_navigation(cx);
        }
    }

    fn on_tab_navigation(&mut self, cx: &mut Context<Self>) {
        self.tab_context = None;
        self.line_geometry.clear();
        self.mouse_selecting = false;
        self.status = "Switched editor tab".into();
        self.reveal_caret();
        cx.notify();
    }

    fn close_other_clean_tabs(&mut self, cx: &mut Context<Self>) {
        if let Some(keep) = self.tabs.active_index() {
            let outcome = self.tabs.close_clean_others(keep);
            self.status = format!(
                "Closed {} other tab(s); {} unsaved tab(s) kept",
                outcome.closed, outcome.blocked
            );
            self.line_geometry.clear();
            self.reveal_caret();
            cx.notify();
        }
    }

    fn close_clean_tabs_to_right(&mut self, cx: &mut Context<Self>) {
        if let Some(keep) = self.tabs.active_index() {
            let outcome = self.tabs.close_clean_to_right(keep);
            self.status = format!(
                "Closed {} tab(s) to right; {} unsaved tab(s) kept",
                outcome.closed, outcome.blocked
            );
            self.line_geometry.clear();
            self.reveal_caret();
            cx.notify();
        }
    }

    fn show_tab_context(&mut self, index: usize, cx: &mut Context<Self>) {
        // Flutter right-click targets the clicked tab without changing which
        // editor is currently focused. Keep the target revision-bound.
        self.tab_context = self.tabs.tab_guard_at(index);
        cx.notify();
    }

    fn close_context_tab(&mut self, cx: &mut Context<Self>) {
        let index = self
            .tab_context
            .as_ref()
            .and_then(|guard| self.tabs.index_for_guard(guard));
        self.tab_context = None;
        if let Some(index) = index {
            self.close_editor_tab(index, false, cx);
        } else {
            cx.notify();
        }
    }

    fn close_context_other_tabs(&mut self, cx: &mut Context<Self>) {
        let index = self
            .tab_context
            .as_ref()
            .and_then(|guard| self.tabs.index_for_guard(guard));
        self.tab_context = None;
        if let Some(index) = index {
            let result = self.tabs.close_clean_others(index);
            self.status = format!(
                "Closed {} other tab(s); {} unsaved tab(s) kept",
                result.closed, result.blocked
            );
            self.line_geometry.clear();
            if !self.tabs.is_empty() {
                self.reveal_caret();
            }
        }
        cx.notify();
    }

    fn close_context_tabs_to_right(&mut self, cx: &mut Context<Self>) {
        let index = self
            .tab_context
            .as_ref()
            .and_then(|guard| self.tabs.index_for_guard(guard));
        self.tab_context = None;
        if let Some(index) = index {
            let result = self.tabs.close_clean_to_right(index);
            self.status = format!(
                "Closed {} tab(s) to right; {} unsaved tab(s) kept",
                result.closed, result.blocked
            );
            self.line_geometry.clear();
            if !self.tabs.is_empty() {
                self.reveal_caret();
            }
        }
        cx.notify();
    }

    pub fn tabs_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn close_editor_tab(&mut self, index: usize, discard: bool, cx: &mut Context<Self>) {
        self.tab_context = None;
        let effect = self.tabs.close_tab(index, discard);
        self.status = effect.status();
        if effect.closed() {
            self.line_geometry.clear();
            self.mouse_selecting = false;
            if !self.tabs.is_empty() {
                self.reveal_caret();
            }
        }
        cx.notify();
    }

    pub fn close_editor(
        &mut self,
        discard: bool,
    ) -> alera_gpui_desktop::editor_session::CloseEffect {
        let effect = self.tabs.close_active(discard);
        if effect.closed() {
            self.line_geometry.clear();
            self.mouse_selecting = false;
        }
        effect
    }

    pub fn switch_allowed(&self) -> Result<(), &'static str> {
        self.tabs.can_switch_workspace()
    }

    fn doc(&self) -> &EditorDocument {
        self.tabs.document().expect("native editor is installed")
    }

    fn doc_mut(&mut self) -> &mut EditorDocument {
        self.tabs
            .document_mut()
            .expect("native editor is installed")
    }

    pub fn has_unsaved_changes(&self) -> bool {
        // Inactive tabs may contain unsaved edits even if the current tab
        // is clean. Explorer mutations must not overlook their buffers.
        self.tabs.can_switch_workspace().is_err()
    }

    /// GPUI's native uniform list uses absolute source line indices and only
    /// constructs visible rows, so navigation works beyond line 600.
    fn reveal_caret(&self) {
        let line = self.doc().line_column().0;
        self.list_scroll
            .scroll_to_item(line, ScrollStrategy::Center);
    }

    fn next_edge_scroll(&self) -> Option<f32> {
        let pointer = self.last_drag_position?;
        let viewport = self.scroll.bounds();
        edge_scroll_offset(
            f32::from(pointer.y),
            f32::from(viewport.top()),
            f32::from(viewport.bottom()),
            f32::from(self.scroll.offset().y),
            f32::from(self.scroll.max_offset().height),
        )
    }

    fn stop_drag(&mut self) {
        self.mouse_selecting = false;
        self.last_drag_position = None;
        self.edge_scroll_ticks = 0;
        // An active timer will observe !mouse_selecting and end itself.
    }

    fn ensure_drag_autoscroll(&mut self, cx: &mut Context<Self>) {
        if self.edge_scroll_running || self.next_edge_scroll().is_none() {
            return;
        }
        self.edge_scroll_running = true;
        self.edge_scroll_ticks = 0;
        // Only one 40ms timer per active pointer drag. It terminates after
        // release, at scroll bounds, on entity destruction or after a bounded
        // 10s fallback in case the OS loses the release event.
        cx.spawn(async move |editor, cx| loop {
            gpui::Timer::after(Duration::from_millis(40)).await;
            match editor.update(cx, |editor, cx| editor.tick_drag_autoscroll(cx)) {
                Ok(true) => {}
                Ok(false) | Err(_) => break,
            }
        })
        .detach();
    }

    fn tick_drag_autoscroll(&mut self, cx: &mut Context<Self>) -> bool {
        const MAX_TICKS: u16 = 250;
        if !self.mouse_selecting || self.edge_scroll_ticks >= MAX_TICKS {
            self.edge_scroll_running = false;
            return false;
        }
        let Some(offset_y) = self.next_edge_scroll() else {
            self.edge_scroll_running = false;
            return false;
        };
        self.edge_scroll_ticks += 1;
        let offset = self.scroll.offset();
        self.scroll.set_offset(point(offset.x, px(offset_y)));
        if let Some(position) = self.last_drag_position {
            let offset = self.pointer_index_at(position);
            let _ = apply_pointer_hit(self.doc_mut(), offset, true);
        }
        cx.notify();
        true
    }

    fn pointer_index_at(&self, position: Point<Pixels>) -> Option<usize> {
        let document = self.tabs.document()?;
        let stored: Vec<&EditorLineGeometry> = self.line_geometry.values().collect();
        let frames: Vec<VisibleLineGeometry> = stored.iter().copied().map(painted_frame).collect();
        let mut hits = VisibleLineHitSet::default();
        hits.replace(frames);
        let shaped = |frame: &VisibleLineGeometry| {
            stored.iter().find(|row| {
                row.start == frame.start
                    && row.end == frame.end
                    && f32::from(row.bounds.top()) == frame.top
                    && f32::from(row.bounds.left()) == frame.left
            })
        };
        hits.pointer_offset(
            document.content(),
            f32::from(position.x),
            f32::from(position.y),
            |frame, index| {
                shaped(frame)
                    .map(|row| f32::from(row.shaped.x_for_index(index)))
                    .unwrap_or(0.0)
            },
            |frame, local_x| {
                shaped(frame)
                    .map(|row| row.shaped.closest_index_for_x(px(local_x)))
                    .unwrap_or(0)
            },
        )
    }

    fn pointer_down(&mut self, event: &gpui::MouseDownEvent, cx: &mut Context<Self>) {
        let offset = self.pointer_index_at(event.position);
        match apply_pointer_hit(self.doc_mut(), offset, event.modifiers.shift) {
            Ok(true) => {
                self.mouse_selecting = true;
                self.last_drag_position = Some(event.position);
            }
            Ok(false) => {}
            Err(error) => self.status = format!("Pointer input blocked: {error}"),
        }
        cx.notify();
    }

    fn pointer_move(&mut self, event: &gpui::MouseMoveEvent, cx: &mut Context<Self>) {
        if !self.mouse_selecting {
            return;
        }
        if !event.dragging() {
            self.stop_drag();
            return;
        }
        self.last_drag_position = Some(event.position);
        let offset = self.pointer_index_at(event.position);
        if apply_pointer_hit(self.doc_mut(), offset, true).unwrap_or(false) {
            cx.notify();
        }
        self.ensure_drag_autoscroll(cx);
    }

    pub fn save(&mut self, cx: &mut Context<Self>) -> bool {
        let commit = self.tabs.save();
        let wrote = commit.wrote_file();
        self.status = commit.status();
        cx.notify();
        wrote
    }

    /// Flutter-like Save on an untitled tab opens the native Save As dialog.
    /// The response is scoped to the original editor tab; switching tabs,
    /// closing/reopening or reordering while the dialog is open cancels it.
    pub fn prompt_save_as(&mut self, cx: &mut Context<Self>) {
        if self.picking_save_as {
            self.status = "A Save As dialog is already open".into();
            cx.notify();
            return;
        }
        let Some(guard) = self.tabs.active_save_guard() else {
            self.status = "Open or create a file before saving".into();
            cx.notify();
            return;
        };
        self.picking_save_as = true;
        self.status = "Choose a workspace path for Save As".into();
        cx.notify();
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Save As".into()),
        });
        cx.spawn(async move |editor, cx| {
            let response = selection.await;
            editor
                .update(cx, |editor, cx| {
                    editor.picking_save_as = false;
                    match response {
                        Ok(Ok(Some(paths))) => {
                            if let Some(path) = paths.into_iter().next() {
                                match editor.tabs.save_as_if_current(&guard, &path, false) {
                                    Some(commit) => {
                                        if commit.wrote_file() {
                                            editor.line_geometry.clear();
                                        }
                                        editor.status = commit.status();
                                    }
                                    None => {
                                        editor.status =
                                            "Save As canceled: editor tab changed while choosing path".into();
                                    }
                                }
                            }
                        }
                        Ok(Ok(None)) => editor.status = "Save As canceled".into(),
                        Ok(Err(error)) => editor.status = format!("Save As failed: {error}"),
                        Err(error) => editor.status = format!("Save As canceled: {error}"),
                    }
                    cx.notify();
                })
                .ok();
        })
        .detach();
    }

    fn navigated(&self, cx: &mut Context<Self>) {
        self.reveal_caret();
        cx.notify();
    }

    fn move_left(&mut self, _: &MoveLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.doc_mut().move_left();
        self.navigated(cx);
    }
    fn move_right(&mut self, _: &MoveRight, _: &mut Window, cx: &mut Context<Self>) {
        self.doc_mut().move_right();
        self.navigated(cx);
    }
    fn move_up(&mut self, _: &MoveUp, _: &mut Window, cx: &mut Context<Self>) {
        self.doc_mut().move_up(false);
        self.navigated(cx);
    }
    fn move_down(&mut self, _: &MoveDown, _: &mut Window, cx: &mut Context<Self>) {
        self.doc_mut().move_down(false);
        self.navigated(cx);
    }
    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.doc_mut().select_left();
        self.navigated(cx);
    }
    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.doc_mut().select_right();
        self.navigated(cx);
    }
    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.doc_mut().move_up(true);
        self.navigated(cx);
    }
    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.doc_mut().move_down(true);
        self.navigated(cx);
    }
    fn move_home(&mut self, _: &MoveHome, _: &mut Window, cx: &mut Context<Self>) {
        self.doc_mut().move_home(false);
        self.navigated(cx);
    }
    fn move_end(&mut self, _: &MoveEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.doc_mut().move_end(false);
        self.navigated(cx);
    }
    fn select_home(&mut self, _: &SelectHome, _: &mut Window, cx: &mut Context<Self>) {
        self.doc_mut().move_home(true);
        self.navigated(cx);
    }
    fn select_end(&mut self, _: &SelectEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.doc_mut().move_end(true);
        self.navigated(cx);
    }
    fn document_start(&mut self, _: &DocumentStart, _: &mut Window, cx: &mut Context<Self>) {
        self.doc_mut().move_document_start(false);
        self.navigated(cx);
    }
    fn document_end(&mut self, _: &DocumentEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.doc_mut().move_document_end(false);
        self.navigated(cx);
    }
    fn select_document_start(
        &mut self,
        _: &SelectDocumentStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.doc_mut().move_document_start(true);
        self.navigated(cx);
    }
    fn select_document_end(
        &mut self,
        _: &SelectDocumentEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.doc_mut().move_document_end(true);
        self.navigated(cx);
    }
    fn undo_edit(&mut self, _: &UndoEdit, _: &mut Window, cx: &mut Context<Self>) {
        if self.doc_mut().undo() {
            self.edit_result(Ok(()), cx);
        }
    }
    fn redo_edit(&mut self, _: &RedoEdit, _: &mut Window, cx: &mut Context<Self>) {
        if self.doc_mut().redo() {
            self.edit_result(Ok(()), cx);
        }
    }
    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        let result = self.doc_mut().backspace();
        self.edit_result(result, cx);
    }
    fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        let result = self.doc_mut().delete_forward();
        self.edit_result(result, cx);
    }
    fn newline(&mut self, _: &Newline, _: &mut Window, cx: &mut Context<Self>) {
        if self.doc_mut().marked().is_some() {
            return;
        }
        let result = self.doc_mut().replace(None, "\n");
        self.edit_result(result, cx);
    }
    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        if self.doc_mut().marked().is_some() {
            return;
        }
        let size = self.doc_mut().content().len();
        let result = self.doc_mut().set_selection(0..size);
        if let Err(error) = result {
            self.status = format!("Selection rejected: {error}");
        }
        self.navigated(cx);
    }
    fn save_file(&mut self, _: &SaveFile, _: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.is_empty() {
            self.status = "Open or create a file before saving".into();
            cx.notify();
        } else if self.doc().path().is_none() {
            self.prompt_save_as(cx);
        } else {
            self.save(cx);
        }
    }
    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(value) = cx.read_from_clipboard().and_then(|item| item.text()) {
            let result = self.doc_mut().replace(None, &value);
            self.edit_result(result, cx);
        }
    }
    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        let copied = self.doc().copied_text().to_owned();
        if !copied.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(copied));
        }
    }
    fn cut(&mut self, _: &Cut, _: &mut Window, cx: &mut Context<Self>) {
        let copied = self.doc().copied_text().to_owned();
        if !copied.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(copied));
            let result = self.doc_mut().replace(None, "");
            self.edit_result(result, cx);
        }
    }
    fn edit_result(&mut self, result: std::io::Result<()>, cx: &mut Context<Self>) {
        match result {
            Err(error) => self.status = format!("Edit rejected: {error}"),
            Ok(()) => {
                if let Err(error) = self.tabs.persist_draft() {
                    self.status = format!("Local recovery save failed: {error}");
                }
                self.reveal_caret();
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
        let (content, actual) = self.doc_mut().text_for_utf16_range(range_utf16);
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
            range: self.doc_mut().utf16_selection(),
            reversed: self.doc_mut().selection_reversed(),
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.doc().utf16_marked()
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.doc_mut().unmark();
        self.edit_result(Ok(()), cx);
    }
    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result = self.doc_mut().replace(range_utf16, text);
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
            .doc_mut()
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
        let line_geometry = &self.line_geometry;
        let document = self.tabs.document()?;
        let content = document.content();
        let byte = utf16_to_byte(content, range_utf16.start);
        let mut hits = VisibleLineHitSet::default();
        hits.replace(line_geometry.values().map(painted_frame));
        let caret = hits.caret_bounds(content, byte, |frame, index| {
            line_geometry
                .values()
                .find(|row| same_painted_row(row, frame))
                .map(|row| f32::from(row.shaped.x_for_index(index)))
                .unwrap_or(f32::NAN)
        })?;
        Some(Bounds::new(
            point(px(caret.x), px(caret.top)),
            size(px(2.0), px(caret.bottom - caret.top)),
        ))
    }
    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        self.pointer_index_at(point)
            .map(|byte| byte_to_utf16(self.doc_mut().content(), byte))
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
            && self.editor.read(cx).document().selection().is_empty()
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

impl NativeEditor {
    /// Construct exactly one source row requested by GPUI's uniform list.
    /// The list never calls this for offscreen rows; selection and IME
    /// highlighting continue to use document-global byte ranges.
    fn render_row(
        &self,
        index: usize,
        span: Range<usize>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (raw, text_len, marked, selection, caret) = {
            let document = self.doc();
            (
                document.content()[span.clone()].to_owned(),
                document.content().len(),
                document.marked(),
                document.selection(),
                document.cursor(),
            )
        };
        let start = span.start;
        let end = span.end;
        let mut display = raw.clone();
        let mut highlights = Vec::new();
        if let Some(composition) = marked {
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
            if end < text_len && selection.start <= end && selection.end > end {
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
        let styled = gpui::StyledText::new(display).with_highlights(highlights);
        div()
            .id(("editable-line", index))
            .relative()
            .h(px(EDITOR_LINE_HEIGHT))
            .line_height(px(EDITOR_LINE_HEIGHT))
            .whitespace_nowrap()
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    window.focus(&this.focus);
                    this.pointer_down(event, cx);
                }),
            )
            .child(styled)
            .child(div().absolute().inset_0().child(EditorLineMetrics {
                editor: cx.entity(),
                index,
                start,
                end,
                text: raw,
                caret,
            }))
    }
}

impl Render for NativeEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.tabs.is_empty() {
            return div()
                .size_full()
                .bg(rgb(PANEL_BG))
                .p_4()
                .text_sm()
                .text_color(rgb(MUTED_TEXT))
                .key_context("AleraGpuiEditor")
                .track_focus(&self.focus)
                .on_action(cx.listener(Self::new_untitled_action))
                .child(self.status.clone())
                .child(
                    div()
                        .id("editor-empty-new-file")
                        .mt_3()
                        .cursor_pointer()
                        .text_color(rgb(EDITOR_TEXT))
                        .child("+ New File")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.new_untitled_tab(window, cx);
                        })),
                )
                .into_any_element();
        }
        self.line_geometry.clear();
        let line_offsets = Arc::new(EditorLineIndex::new(self.doc().content()));
        let line_count = line_offsets.line_count();
        let (line_index, column_index) = self.doc().line_column();
        let (undo_count, redo_count) = self.doc().history_depth();
        let dirty = self.doc().is_dirty();
        let composing = self.doc().marked().is_some();
        let recovery =
            if self.tabs.has_draft_store() && !self.status.contains(RECOVERY_PLAINTEXT_NOTICE) {
                format!(" · {RECOVERY_PLAINTEXT_NOTICE}")
            } else {
                String::new()
            };
        let tab_labels = self.tabs.tab_labels();
        let drag_identities: Vec<_> = (0..tab_labels.len())
            .map(|index| self.tabs.drag_identity_at(index))
            .collect();
        let active_tab = self.tabs.active_index();
        let context_target = self
            .tab_context
            .as_ref()
            .and_then(|guard| self.tabs.index_for_guard(guard));
        let status = format!(
            "{} · Ln {} / {} Col {} · Undo {} / Redo {} · {}{}{}",
            self.status,
            line_index + 1,
            line_count,
            column_index + 1,
            undo_count,
            redo_count,
            if dirty { "Modified" } else { "Saved" },
            if composing { " · composing" } else { "" },
            recovery
        );
        let find_toolbar = self.render_find_toolbar(window, cx);
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(PANEL_BG))
            .text_color(rgb(EDITOR_TEXT))
            .key_context("AleraGpuiEditorSurface")
            .on_action(cx.listener(Self::show_find))
            .on_action(cx.listener(Self::find_next))
            .on_action(cx.listener(Self::find_previous))
            .on_action(cx.listener(Self::hide_find))
            .on_action(cx.listener(Self::focus_find_input))
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
            .on_action(cx.listener(Self::document_start))
            .on_action(cx.listener(Self::document_end))
            .on_action(cx.listener(Self::select_document_start))
            .on_action(cx.listener(Self::select_document_end))
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
            .on_action(cx.listener(Self::next_editor_tab))
            .on_action(cx.listener(Self::previous_editor_tab))
            .on_action(cx.listener(Self::new_untitled_action))
            .on_action(cx.listener(Self::close_current_tab_action))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_1()
                    .child(
                        div()
                            .id("editor-tab-scroll")
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .overflow_x_scroll()
                            .children(tab_labels.into_iter().zip(drag_identities).enumerate().map(
                                |(index, (label, drag_identity))| {
                                    let active = active_tab == Some(index);
                                    let drag_title = label.title.clone();
                                    div()
                                        .id(("editor-file-tab", index))
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .child(
                                            div()
                                                .id(("editor-file-tab-select", index))
                                                .cursor_pointer()
                                                .text_sm()
                                                .text_color(rgb(if active {
                                                    EDITOR_TEXT
                                                } else {
                                                    MUTED_TEXT
                                                }))
                                                .child(format!(
                                                    "{}{}",
                                                    label.title,
                                                    if label.dirty { " ●" } else { "" }
                                                ))
                                                .on_mouse_down(
                                                    MouseButton::Right,
                                                    cx.listener(move |this, _, _, cx| {
                                                        this.show_tab_context(index, cx);
                                                    }),
                                                )
                                                .on_mouse_up(
                                                    MouseButton::Middle,
                                                    cx.listener(move |this, _, _, cx| {
                                                        this.close_editor_tab(index, false, cx);
                                                    }),
                                                )
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.activate_tab(index, cx);
                                                        window.focus(&this.focus);
                                                    },
                                                ))
                                                .on_drag(
                                                    EditorTabDrag {
                                                        identity: drag_identity,
                                                    },
                                                    move |_, _, _, cx| {
                                                        cx.new(|_| EditorTabDragPreview {
                                                            title: drag_title.clone(),
                                                        })
                                                    },
                                                )
                                                .on_drop(cx.listener(
                                                    move |this, payload: &EditorTabDrag, _, cx| {
                                                        if let Some(identity) = &payload.identity {
                                                            if this
                                                                .tabs
                                                                .move_tab_identity(identity, index)
                                                            {
                                                                this.status =
                                                                    "Editor tab reordered".into();
                                                                this.on_tab_navigation(cx);
                                                            }
                                                        }
                                                    },
                                                )),
                                        )
                                        .child(
                                            div()
                                                .id(("editor-file-tab-close", index))
                                                .cursor_pointer()
                                                .text_sm()
                                                .text_color(rgb(MUTED_TEXT))
                                                .child(if label.armed {
                                                    "Confirm"
                                                } else {
                                                    "Close"
                                                })
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.close_editor_tab(index, true, cx);
                                                })),
                                        )
                                },
                            ))
                            .child(
                                div()
                                    .id("editor-tab-empty-area")
                                    .flex_1()
                                    .min_w(px(40.0))
                                    .h(px(28.0))
                                    .on_click(cx.listener(
                                        |this, event: &gpui::ClickEvent, window, cx| {
                                            if event.click_count() == 2 {
                                                this.new_untitled_tab(window, cx);
                                            }
                                        },
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .id("editor-tab-new-file")
                            .cursor_pointer()
                            .px_2()
                            .py_1()
                            .text_sm()
                            .child("+ New File")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.new_untitled_tab(window, cx);
                            })),
                    ),
            )
            // The actions are shown only for the tab actually right-clicked.
            // They do not change the active editor and never force-close a
            // dirty tab. Unlike a platform popup this slim inline menu stays
            // within GPUI's own tab surface.
            .child(
                div()
                    .id("editor-tab-context-actions")
                    .when(context_target.is_some(), |menu| {
                        menu.flex()
                            .items_center()
                            .gap_3()
                            .px_3()
                            .py_2()
                            .border_b_1()
                            .border_color(rgb(MUTED_TEXT))
                            .bg(rgb(PANEL_BG))
                            .text_sm()
                            .child(
                                div()
                                    .id("editor-tab-context-close")
                                    .cursor_pointer()
                                    .child("Close")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.close_context_tab(cx);
                                    })),
                            )
                            .child(
                                div()
                                    .id("editor-tab-context-close-others")
                                    .cursor_pointer()
                                    .child("Close Others")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.close_context_other_tabs(cx);
                                    })),
                            )
                            .child(
                                div()
                                    .id("editor-tab-context-close-right")
                                    .cursor_pointer()
                                    .child("Close to Right")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.close_context_tabs_to_right(cx);
                                    })),
                            )
                            .child(
                                div()
                                    .id("editor-tab-context-cancel")
                                    .cursor_pointer()
                                    .text_color(rgb(MUTED_TEXT))
                                    .child("Cancel")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.tab_context = None;
                                        cx.notify();
                                    })),
                            )
                    }),
            )
            .child(
                div()
                    .flex()
                    .gap_3()
                    .px_3()
                    .py_1()
                    .text_sm()
                    .text_color(rgb(MUTED_TEXT))
                    .child(
                        div()
                            .id("editor-tab-previous")
                            .cursor_pointer()
                            .child("◀ Tab")
                            .on_click(cx.listener(|this, _, _, cx| {
                                if this.tabs.activate_previous() {
                                    this.on_tab_navigation(cx);
                                }
                            })),
                    )
                    .child(
                        div()
                            .id("editor-tab-next")
                            .cursor_pointer()
                            .child("Tab ▶")
                            .on_click(cx.listener(|this, _, _, cx| {
                                if this.tabs.activate_next() {
                                    this.on_tab_navigation(cx);
                                }
                            })),
                    )
                    .child(
                        div()
                            .id("editor-tab-move-left")
                            .cursor_pointer()
                            .child("Move ←")
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(index) = this.tabs.active_index() {
                                    if index > 0 && this.tabs.move_tab(index, index - 1) {
                                        this.status = "Editor tab moved left".into();
                                        this.on_tab_navigation(cx);
                                    }
                                }
                            })),
                    )
                    .child(
                        div()
                            .id("editor-tab-move-right")
                            .cursor_pointer()
                            .child("Move →")
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(index) = this.tabs.active_index() {
                                    if this.tabs.move_tab(index, index + 1) {
                                        this.status = "Editor tab moved right".into();
                                        this.on_tab_navigation(cx);
                                    }
                                }
                            })),
                    )
                    .child(
                        div()
                            .id("editor-tab-close-others")
                            .cursor_pointer()
                            .child("Close others")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_other_clean_tabs(cx);
                            })),
                    )
                    .child(
                        div()
                            .id("editor-tab-close-right")
                            .cursor_pointer()
                            .child("Close right")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_clean_tabs_to_right(cx);
                            })),
                    ),
            )
            .child(find_toolbar)
            .on_mouse_move(cx.listener(|this, event, _, cx| {
                this.pointer_move(event, cx);
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.stop_drag()),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.stop_drag()),
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
                                        if this.doc_mut().undo() {
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
                                        if this.doc_mut().redo() {
                                            this.edit_result(Ok(()), cx);
                                        }
                                    })),
                            ),
                    ),
            )
            .child(
                div()
                    .key_context("AleraGpuiEditor")
                    .track_focus(&self.focus)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, _| window.focus(&this.focus)),
                    )
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .font_family("JetBrains Mono")
                    .text_sm()
                    .child(NativeInputCapture {
                        editor: cx.entity(),
                    })
                    .child(
                        uniform_list(
                            "gpui-editor-text-scroll",
                            line_count,
                            cx.processor(move |this, range: Range<usize>, _, cx| {
                                // Unlike a normal Div scroll, uniform_list
                                // re-renders only the visible rows even when
                                // the parent entity is not re-rendered.
                                this.line_geometry.clear();
                                let mut rows = Vec::with_capacity(range.len());
                                for index in range {
                                    if let Some(span) =
                                        line_offsets.line_range(this.doc_mut().content(), index)
                                    {
                                        rows.push(this.render_row(index, span, cx));
                                    }
                                }
                                rows
                            }),
                        )
                        .flex_1()
                        .min_h_0()
                        .w_full()
                        .px_3()
                        .py_2()
                        .track_scroll(self.list_scroll.clone()),
                    ),
            )
            .into_any_element()
    }
}
