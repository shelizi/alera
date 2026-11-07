//! Experimental Alera desktop shell powered by native GPUI (no Flutter/WebView).
mod editor_ui;

use alera_gpui_desktop::editor::{EditorDocument, EditorDraftStore};
use alera_gpui_desktop::model::WorkbenchModel;
use alera_gpui_desktop::runtime_bridge::RuntimeCliBridge;
use alera_gpui_desktop::session::PreviewSessionStore;
use alera_gpui_desktop::terminal::{TerminalDock, TerminalPollGate, TerminalViewMode};
use editor_ui::{bind_editor_keys, NativeEditor};
use gpui::{
    div, prelude::*, px, rgb, size, App, Application, Bounds, Context, Entity, PathPromptOptions,
    Window, WindowBounds, WindowOptions,
};
use std::path::PathBuf;
use std::time::Duration;

const BG: u32 = 0x15171b;
const PANEL: u32 = 0x1c1f24;
const TOOLBAR: u32 = 0x252931;
const BORDER: u32 = 0x343942;
const TEXT: u32 = 0xe1e4e8;
const MUTED: u32 = 0xa6adb7;
const ACTIVE: u32 = 0x303741;

struct Workbench {
    model: WorkbenchModel,
    editor: Option<Entity<NativeEditor>>,
    editor_discard_pending: bool,
    session_store: Option<PreviewSessionStore>,
    recent_workspaces: Vec<PathBuf>,
    show_recent_workspaces: bool,
    picking_workspace: bool,
    workspace_generation: u64,
    runtime_bridge: Option<RuntimeCliBridge>,
    runtime_status: String,
    runtime_loading: bool,
    terminals: TerminalDock,
    terminal_list_loading: bool,
    terminal_read_loading: Option<String>,
    terminal_poll: TerminalPollGate,
    terminal_tick: u64,
}

impl Workbench {
    fn editor_dirty(&self, cx: &mut Context<Self>) -> bool {
        self.editor
            .as_ref()
            .is_some_and(|editor| editor.read(cx).has_unsaved_changes())
    }

    fn open_editor(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.model.selected_path().map(PathBuf::from) else {
            self.model.set_status("Choose a text file before editing");
            cx.notify();
            return;
        };
        if let Some(existing) = &self.editor {
            if existing.read(cx).document.path() == Some(path.as_path()) {
                self.model
                    .set_status("The selected file is already in the native editor");
                cx.notify();
                return;
            }
        }
        if self.editor_dirty(cx) {
            self.model
                .set_status("Save or explicitly discard current edits before opening another file");
            cx.notify();
            return;
        }
        match EditorDocument::open(self.model.root(), &path) {
            Ok(mut document) => {
                let drafts = self
                    .session_store
                    .as_ref()
                    .map(|store| EditorDraftStore::new(store.state_dir()));
                if let Some(store) = &drafts {
                    if let Err(error) = store.restore(&mut document) {
                        self.model.set_status(format!(
                            "Recovery draft invalid for {}: {error}. Preserved for investigation.",
                            path.display()
                        ));
                        cx.notify();
                        return;
                    }
                }
                let recovered = document.is_dirty();
                self.editor = Some(cx.new(|cx| NativeEditor::new(document, drafts, cx)));
                self.editor_discard_pending = false;
                self.model.set_status(if recovered {
                    "Recovered unsaved editor changes; disk file was not changed"
                } else {
                    "Native text editor opened. Click a line to focus and type"
                });
            }
            Err(error) => self
                .model
                .set_status(format!("Cannot edit {}: {error}", path.display())),
        }
        cx.notify();
    }

    fn save_editor(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = &self.editor else {
            return;
        };
        let path = editor.read(cx).document.path().map(PathBuf::from);
        let saved = editor.update(cx, |editor, cx| editor.save(cx));
        if saved {
            if self.model.selected_path() == path.as_deref() {
                self.model.reload_active();
            }
            self.editor_discard_pending = false;
            self.model
                .set_status("Native editor saved the selected document");
        } else {
            self.model
                .set_status("Save blocked; see native editor status");
        }
        cx.notify();
    }

    fn close_editor(&mut self, discard: bool, cx: &mut Context<Self>) {
        if self.editor_dirty(cx) {
            if !discard {
                self.editor_discard_pending = false;
                self.model
                    .set_status("Unsaved editor changes: Save or use Discard twice");
                cx.notify();
                return;
            }
            if !self.editor_discard_pending {
                self.editor_discard_pending = true;
                self.model
                    .set_status("Click Confirm discard to lose unsaved editor content");
                cx.notify();
                return;
            }
        }
        if let Some(editor) = &self.editor {
            if discard {
                if let Err(error) = editor.read(cx).discard_recovery() {
                    self.model
                        .set_status(format!("Cannot clear local draft: {error}"));
                    cx.notify();
                    return;
                }
            }
            if self.model.selected_path() == editor.read(cx).document.path() {
                self.model.reload_active();
            }
        }
        self.editor = None;
        self.editor_discard_pending = false;
        self.model
            .set_status("Native editor closed; read-only preview remains available");
        cx.notify();
    }

    fn persist_session(&mut self) {
        if let Some(store) = &self.session_store {
            if let Err(error) = store.save(&self.model) {
                eprintln!("Alera GPUI: unable to persist preview session: {error}");
                self.model
                    .set_status(format!("Preview state not saved: {error}"));
            } else if let Ok(recent) = store.recent_workspaces() {
                self.recent_workspaces = recent;
            }
        }
    }

    fn switch_workspace(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        if self.editor_dirty(cx) {
            self.model.set_status(
                "Save or explicitly discard native editor changes before switching workspace",
            );
            cx.notify();
            return;
        }
        let canonical = match dunce::canonicalize(&root) {
            Ok(root) => root,
            Err(error) => {
                self.model
                    .set_status(format!("Cannot open workspace {}: {error}", root.display()));
                cx.notify();
                return;
            }
        };
        if canonical == self.model.root() {
            self.show_recent_workspaces = false;
            self.model.set_status("Workspace already open");
            cx.notify();
            return;
        }
        let result = if let Some(store) = &self.session_store {
            store.switch_workspace(&self.model, &canonical)
        } else {
            WorkbenchModel::new(&canonical)
        };
        let next = match result {
            Ok(next) => next,
            Err(error) => {
                self.model
                    .set_status(format!("Workspace switch failed: {error}"));
                cx.notify();
                return;
            }
        };
        self.model = next;
        self.editor = None;
        self.editor_discard_pending = false;
        self.workspace_generation = self.workspace_generation.wrapping_add(1);
        // Terminal preview tabs are local UI state, never PTY owners. Drop
        // them on workspace changes to prevent cross-project output leakage.
        // Existing Host sessions remain alive and under Host ownership.
        self.terminals = TerminalDock::default();
        self.terminal_poll = TerminalPollGate::default();
        self.terminal_list_loading = false;
        self.terminal_read_loading = None;
        self.terminal_tick = 0;
        self.show_recent_workspaces = false;
        if let Some(store) = &self.session_store {
            match store.recent_workspaces() {
                Ok(recent) => self.recent_workspaces = recent,
                Err(error) => self
                    .model
                    .set_status(format!("Recent workspace list: {error}")),
            }
        } else {
            self.recent_workspaces = vec![canonical];
        }
        self.refresh_terminals(cx);
        cx.notify();
    }

    fn pick_workspace(&mut self, cx: &mut Context<Self>) {
        if self.picking_workspace {
            return;
        }
        self.picking_workspace = true;
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open Alera workspace".into()),
        });
        cx.spawn(async move |view, cx| {
            let result = selection.await;
            view.update(cx, |view, cx| {
                view.picking_workspace = false;
                match result {
                    Ok(Ok(Some(paths))) => {
                        if let Some(path) = paths.into_iter().next() {
                            view.switch_workspace(path, cx);
                        }
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(error)) => view
                        .model
                        .set_status(format!("Workspace picker failed: {error}")),
                    Err(error) => view
                        .model
                        .set_status(format!("Workspace picker canceled: {error}")),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn refresh_runtime(&mut self, cx: &mut Context<Self>) {
        if self.runtime_loading {
            return;
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.runtime_status =
                "CLI unavailable. Set ALERA_CLI_PATH to the existing Alera CLI.".into();
            cx.notify();
            return;
        };
        self.runtime_loading = true;
        self.runtime_status = "Checking existing Runtime Host…".into();
        cx.notify();
        // CLI handles authentication and the bounded probe runs off the UI
        // thread. The experimental frontend never starts or owns a PTY.
        cx.spawn(async move |view, cx| {
            let result = cx.background_spawn(async move { bridge.probe() }).await;
            view.update(cx, |view, cx| {
                view.runtime_loading = false;
                view.runtime_status = match result {
                    Ok(status) => status.label(),
                    Err(error) => format!("Runtime check failed: {error}"),
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn refresh_terminals(&mut self, cx: &mut Context<Self>) {
        if self.terminal_list_loading {
            return;
        }
        if self.runtime_bridge.is_none() {
            self.runtime_bridge = RuntimeCliBridge::discover().ok();
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.terminals.update_sessions(Vec::new());
            self.terminals.status = "Cannot list sessions: install experimental Alera CLI".into();
            cx.notify();
            return;
        };
        self.terminal_list_loading = true;
        let generation = self.workspace_generation;
        self.terminals.status = "Listing existing Host sessions…".into();
        cx.notify();
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move { bridge.list_terminals() })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation {
                    return;
                }
                view.terminal_list_loading = false;
                match result {
                    Ok(sessions) => {
                        let was_disconnected = view.terminals.sessions.is_empty();
                        view.terminals.update_sessions(sessions);
                        if was_disconnected && !view.terminals.sessions.is_empty() {
                            view.terminal_poll.reset();
                        }
                    }
                    Err(error) => {
                        view.terminals.update_sessions(Vec::new());
                        view.terminals.status = format!("Session refresh failed: {error}");
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn select_terminal(&mut self, handle: String, cx: &mut Context<Self>) {
        if self.terminals.open(&handle) {
            self.refresh_terminal_output(cx);
        } else {
            cx.notify();
        }
    }

    fn refresh_terminal_output(&mut self, cx: &mut Context<Self>) {
        if self.terminal_read_loading.is_some() {
            return;
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.terminals.status = "CLI unavailable".into();
            cx.notify();
            return;
        };
        let Some(ticket) = self.terminals.active_ticket() else {
            return;
        };
        let handle = ticket.handle.clone();
        let cursor = ticket.cursor;
        let generation = self.workspace_generation;
        self.terminal_read_loading = Some(handle.clone());
        cx.notify();
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move { bridge.read_terminal(&handle, cursor) })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation {
                    return;
                }
                view.terminal_read_loading = None;
                match result {
                    Ok(chunk) => {
                        if view.terminals.active_ticket().as_ref() == Some(&ticket) {
                            // Successful reads with no new output still
                            // reset previous failure backoff.
                            view.terminal_poll.reset();
                        }
                        view.terminals.apply_ticket_chunk(&ticket, chunk);
                    }
                    Err(error) => {
                        if view.terminals.active_ticket().as_ref() == Some(&ticket) {
                            view.terminal_poll.failed();
                            view.terminals.status = format!("Terminal read failed: {error}");
                        }
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn start_terminal_polling(&mut self, cx: &mut Context<Self>) {
        // A single lifecycle-bound timer: no timer per tab, no stale callback
        // after the Workbench entity/window has been destroyed.
        cx.spawn(async move |view, cx| loop {
            gpui::Timer::after(Duration::from_millis(1500)).await;
            if view
                .update(cx, |view, cx| {
                    view.terminal_tick = view.terminal_tick.wrapping_add(1);
                    if view.terminal_tick % 10 == 1 {
                        view.refresh_terminals(cx);
                    }
                    if view.terminal_poll.tick()
                        && view.terminal_read_loading.is_none()
                        && view.terminals.active_ticket().is_some()
                    {
                        view.refresh_terminal_output(cx);
                    }
                })
                .is_err()
            {
                break;
            }
        })
        .detach();
    }
}

impl Render for Workbench {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let folder_name = self
            .model
            .directory()
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| self.model.directory().display().to_string());
        let filename = self
            .model
            .selected_path()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "Welcome".into());
        let content = if self.model.selected_path().is_some() {
            self.model.content().to_owned()
        } else {
            "Select a text file from Explorer to preview it.\n\nAlera GPUI runs without Flutter or WebView.\nTerminal, editable documents, Diff and Agent surfaces are not migrated yet.".into()
        };
        let entries = self.model.entries().to_vec();
        let tabs: Vec<_> = self
            .model
            .tabs()
            .iter()
            .enumerate()
            .map(|(index, tab)| (index, tab.title.clone()))
            .collect();
        let active_tab = self.model.active_tab();
        let editor = self.editor.clone();
        let editor_dirty = self.editor_dirty(cx);
        let discard_pending = self.editor_discard_pending;
        let editor_title = self
            .editor
            .as_ref()
            .and_then(|editor| {
                editor
                    .read(cx)
                    .document
                    .path()
                    .map(|path| path.display().to_string())
            })
            .unwrap_or_else(|| filename.clone());
        let status = self.model.status().to_owned();
        let branch = self.model.branch().to_owned();
        let can_up = self.model.can_go_up();
        let show_recent_workspaces = self.show_recent_workspaces;
        let picking_workspace = self.picking_workspace;
        let workspace_root = self.model.root().to_path_buf();
        let recent_workspaces: Vec<_> = self
            .recent_workspaces
            .iter()
            .take(12)
            .cloned()
            .enumerate()
            .collect();
        let runtime_status = self.runtime_status.clone();
        let runtime_loading = self.runtime_loading;
        let terminal_list_loading = self.terminal_list_loading;
        let terminal_read_loading = self.terminal_read_loading.is_some();
        let terminal_sessions: Vec<_> = self
            .terminals
            .sessions
            .iter()
            .map(|session| (session.handle.clone(), session.label()))
            .collect();
        let terminal_tabs: Vec<_> = self
            .terminals
            .tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| (i, tab.title.clone(), tab.running))
            .collect();
        let terminal_active = self.terminals.active_tab;
        let terminal_output = self
            .terminals
            .active()
            .map(|tab| match self.terminals.view_mode {
                TerminalViewMode::Screen => tab.screen.contents(),
                TerminalViewMode::Transcript => tab.output.clone(),
            })
            .unwrap_or_default();
        let terminal_status = self.terminals.status.clone();
        let has_terminal_preview = self.terminals.active().is_some();
        let terminal_auto = self.terminal_poll.enabled();
        let terminal_is_vt = self.terminals.view_mode == TerminalViewMode::Screen;
        let terminal_screen_rows = if terminal_is_vt {
            self.terminals
                .active()
                .map(|tab| tab.screen.styled_rows())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let terminal_status = if terminal_is_vt {
            if let Some(tab) = self.terminals.active() {
                let (rows, cols) = tab.screen.dimensions();
                let (cursor_row, cursor_col) = tab.screen.cursor();
                format!(
                    "{terminal_status} · {cols}×{rows} · Cursor {}:{}{}",
                    cursor_row + 1,
                    cursor_col + 1,
                    if tab.screen.alternate_screen() {
                        " · Alt"
                    } else {
                        ""
                    }
                )
            } else {
                terminal_status
            }
        } else {
            terminal_status
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .child(
                div()
                    .h(px(46.0))
                    .px_4()
                    .bg(rgb(TOOLBAR))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().font_weight(gpui::FontWeight::BOLD).child("ALERA"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(MUTED))
                            .child("GPUI · Native Desktop Preview"),
                    )
                    .child(div().text_sm().child(branch.clone())),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(
                        div()
                            .w(px(265.0))
                            .flex()
                            .flex_col()
                            .bg(rgb(PANEL))
                            .border_r_1()
                            .border_color(rgb(BORDER))
                            .child(
                                div()
                                    .px_3()
                                    .py_3()
                                    .flex()
                                    .justify_between()
                                    .items_center()
                                    .child(div().text_sm().child(folder_name))
                                    .child(
                                        div()
                                            .flex()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .id("up")
                                                    .cursor_pointer()
                                                    .text_color(rgb(MUTED))
                                                    .child(if can_up { "↑ Up" } else { "Root" })
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.model.go_up();
                                                        this.persist_session();
                                                        cx.notify();
                                                    })),
                                            )
                                            .child(
                                                div()
                                                    .id("explorer-refresh")
                                                    .cursor_pointer()
                                                    .text_color(rgb(MUTED))
                                                    .child("↻")
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.model.refresh_directory();
                                                        cx.notify();
                                                    })),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .px_3()
                                    .py_2()
                                    .border_b_1()
                                    .border_color(rgb(BORDER))
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .flex()
                                            .justify_between()
                                            .items_center()
                                            .child(
                                                div()
                                                    .id("recent-workspaces-toggle")
                                                    .cursor_pointer()
                                                    .text_sm()
                                                    .child(if show_recent_workspaces {
                                                        "Recent workspaces ▾"
                                                    } else {
                                                        "Recent workspaces ▸"
                                                    })
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.show_recent_workspaces =
                                                            !this.show_recent_workspaces;
                                                        cx.notify();
                                                    })),
                                            )
                                            .child(
                                                div()
                                                    .id("open-workspace-folder")
                                                    .cursor_pointer()
                                                    .text_sm()
                                                    .text_color(rgb(MUTED))
                                                    .child(if picking_workspace { "Opening…" } else { "+ Open" })
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.pick_workspace(cx);
                                                    })),
                                            ),
                                    )
                                    .child(if show_recent_workspaces {
                                        div()
                                            .id("recent-workspaces-scroll")
                                            .max_h(px(158.0))
                                            .overflow_y_scroll()
                                            .children(recent_workspaces.into_iter().map(|(index, path)| {
                                                let active = workspace_root == path;
                                                let name = path
                                                    .file_name()
                                                    .map(|name| name.to_string_lossy().to_string())
                                                    .unwrap_or_else(|| path.display().to_string());
                                                let detail = path.display().to_string();
                                                div()
                                                    .id(("recent-workspace", index))
                                                    .cursor_pointer()
                                                    .py_1()
                                                    .px_1()
                                                    .overflow_hidden()
                                                    .hover(|style| style.bg(rgb(ACTIVE)))
                                                    .child(
                                                        div()
                                                            .text_sm()
                                                            .text_color(rgb(if active { TEXT } else { MUTED }))
                                                            .child(format!("{} {name}", if active { "●" } else { "○" })),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(rgb(MUTED))
                                                            .child(detail),
                                                    )
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.switch_workspace(path.clone(), cx);
                                                    }))
                                            }))
                                    } else {
                                        div().id("recent-workspaces-collapsed").h(px(0.0))
                                    }),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_h_0()
                                    .id("explorer-scroll")
                                    .overflow_y_scroll()
                                    .children(entries.into_iter().enumerate().map(|(i, item)| {
                                        div()
                                            .id(i)
                                            .px_3()
                                            .py_1()
                                            .cursor_pointer()
                                            .hover(|style| style.bg(rgb(ACTIVE)))
                                            .child(format!(
                                                "{} {}",
                                                if item.is_directory { "▸" } else { "·" },
                                                item.name
                                            ))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.model.open_entry(i);
                                                this.persist_session();
                                                cx.notify();
                                            }))
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .h(px(38.0))
                                    .flex()
                                    .items_center()
                                    .bg(rgb(PANEL))
                                    .border_b_1()
                                    .border_color(rgb(BORDER))
                                    .child(
                                        div()
                                            .id("file-tabs")
                                            .flex_1()
                                            .min_w_0()
                                            .flex()
                                            .overflow_x_scroll()
                                            .children(tabs.into_iter().map(|(index, title)| {
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .w(px(165.0))
                                                    .h(px(37.0))
                                                    .bg(rgb(if active_tab == Some(index) {
                                                        ACTIVE
                                                    } else {
                                                        PANEL
                                                    }))
                                                    .border_r_1()
                                                    .border_color(rgb(BORDER))
                                                    .child(
                                                        div()
                                                            .id(("file-tab", index))
                                                            .flex_1()
                                                            .min_w_0()
                                                            .px_2()
                                                            .cursor_pointer()
                                                            .text_sm()
                                                            .text_color(rgb(
                                                                if active_tab == Some(index) {
                                                                    TEXT
                                                                } else {
                                                                    MUTED
                                                                },
                                                            ))
                                                            .child(title)
                                                            .on_click(cx.listener(
                                                                move |this, _, _, cx| {
                                                                    this.model.activate_tab(index);
                                                                    this.persist_session();
                                                                    cx.notify();
                                                                },
                                                            )),
                                                    )
                                                    .child(
                                                        div()
                                                            .id(("close-tab", index))
                                                            .px_2()
                                                            .cursor_pointer()
                                                            .text_color(rgb(MUTED))
                                                            .child("×")
                                                            .on_click(cx.listener(
                                                                move |this, _, _, cx| {
                                                                    this.model.close_tab(index);
                                                                    this.persist_session();
                                                                    cx.notify();
                                                                },
                                                            )),
                                                    )
                                            })),
                                    )
                                    .child(
                                        div()
                                            .id("reload-preview")
                                            .px_3()
                                            .cursor_pointer()
                                            .text_color(rgb(MUTED))
                                            .child("↻")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.model.reload_active();
                                                cx.notify();
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .h(px(29.0))
                                    .px_4()
                                    .bg(rgb(BG))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .text_sm()
                                    .text_color(rgb(MUTED))
                                    .child(if editor.is_some() {
                                        format!("Editing: {}{}", editor_title, if editor_dirty { " ●" } else { "" })
                                    } else {
                                        format!("{} · read-only preview", filename)
                                    })
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_3()
                                            .child(
                                                div()
                                                    .id("native-editor-edit")
                                                    .cursor_pointer()
                                                    .text_color(rgb(TEXT))
                                                    .child("Edit")
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.open_editor(cx);
                                                    })),
                                            )
                                            .when(editor.is_some(), |toolbar| {
                                                toolbar
                                                    .child(
                                                        div()
                                                            .id("native-editor-save")
                                                            .cursor_pointer()
                                                            .text_color(rgb(TEXT))
                                                            .child("Save")
                                                            .on_click(cx.listener(|this, _, _, cx| {
                                                                this.save_editor(cx);
                                                            })),
                                                    )
                                                    .child(
                                                        div()
                                                            .id("native-editor-close")
                                                            .cursor_pointer()
                                                            .child("Close")
                                                            .on_click(cx.listener(|this, _, _, cx| {
                                                                this.close_editor(false, cx);
                                                            })),
                                                    )
                                                    .child(
                                                        div()
                                                            .id("native-editor-discard")
                                                            .cursor_pointer()
                                                            .text_color(rgb(MUTED))
                                                            .child(if discard_pending {
                                                                "Confirm discard"
                                                            } else {
                                                                "Discard"
                                                            })
                                                            .on_click(cx.listener(|this, _, _, cx| {
                                                                this.close_editor(true, cx);
                                                            })),
                                                    )
                                            }),
                                    ),
                            )
                            .child(
                                if let Some(editor) = editor {
                                    div()
                                        .flex_1()
                                        .min_h_0()
                                        .child(editor)
                                        .into_any_element()
                                } else {
                                    div()
                                        .flex_1()
                                        .min_h_0()
                                        .id("preview-scroll")
                                        .overflow_y_scroll()
                                        .p_4()
                                        .text_sm()
                                        .font_family("JetBrains Mono")
                                        .child(content)
                                        .into_any_element()
                                },
                            )
                            .child(if has_terminal_preview {
                                div()
                                    .h(px(245.0))
                                    .flex()
                                    .flex_col()
                                    .border_t_1()
                                    .border_color(rgb(BORDER))
                                    .bg(rgb(BG))
                                    .child(
                                        div()
                                            .h(px(36.0))
                                            .flex()
                                            .items_center()
                                            .bg(rgb(PANEL))
                                            .child(
                                                div()
                                                    .id("terminal-preview-tabs")
                                                    .flex_1()
                                                    .min_w_0()
                                                    .flex()
                                                    .overflow_x_scroll()
                                                    .children(terminal_tabs.into_iter().map(
                                                        |(index, title, running)| {
                                                            div()
                                                                .flex()
                                                                .items_center()
                                                                .h(px(35.0))
                                                                .bg(rgb(if terminal_active == Some(index) {
                                                                    ACTIVE
                                                                } else {
                                                                    PANEL
                                                                }))
                                                                .border_r_1()
                                                                .border_color(rgb(BORDER))
                                                                .child(
                                                                    div()
                                                                        .id(("term-tab", index))
                                                                        .px_2()
                                                                        .cursor_pointer()
                                                                        .text_sm()
                                                                        .child(format!(
                                                                            "{} {title}",
                                                                            if running { "●" } else { "○" }
                                                                        ))
                                                                        .on_click(cx.listener(move |this, _, _, cx| {
                                                                            if this.terminals.activate(index) {
                                                                                this.refresh_terminal_output(cx);
                                                                            }
                                                                            cx.notify();
                                                                        })),
                                                                )
                                                                .child(
                                                                    div()
                                                                        .id(("term-close", index))
                                                                        .px_2()
                                                                        .cursor_pointer()
                                                                        .text_color(rgb(MUTED))
                                                                        .child("×")
                                                                        .on_click(cx.listener(move |this, _, _, cx| {
                                                                            this.terminals.close(index);
                                                                            cx.notify();
                                                                        })),
                                                                )
                                                        },
                                                    )),
                                            )
                                            .child(
                                                div()
                                                    .id("terminal-vt-mode")
                                                    .px_2()
                                                    .cursor_pointer()
                                                    .text_sm()
                                                    .text_color(rgb(TEXT))
                                                    .child(if terminal_is_vt { "VT screen" } else { "Text log" })
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.terminals.toggle_view_mode();
                                                        cx.notify();
                                                    })),
                                            )
                                            .child(
                                                div()
                                                    .id("terminal-preview-autopoll")
                                                    .px_2()
                                                    .cursor_pointer()
                                                    .text_sm()
                                                    .text_color(rgb(if terminal_auto { TEXT } else { MUTED }))
                                                    .child(if terminal_auto { "Auto: On" } else { "Auto: Off" })
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.terminal_poll.toggle();
                                                        cx.notify();
                                                    })),
                                            )
                                            .child(
                                                div()
                                                    .id("terminal-preview-refresh")
                                                    .px_3()
                                                    .cursor_pointer()
                                                    .text_color(rgb(MUTED))
                                                    .child(if terminal_read_loading { "Reading…" } else { "Read more ↻" })
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.refresh_terminal_output(cx);
                                                    })),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .h(px(22.0))
                                            .px_3()
                                            .text_sm()
                                            .text_color(rgb(MUTED))
                                            .child(terminal_status),
                                    )
                                    .child(if terminal_is_vt {
                                        div()
                                            .id("terminal-colored-grid")
                                            .flex_1()
                                            .min_h_0()
                                            .overflow_x_scroll()
                                            .overflow_y_scroll()
                                            .px_3()
                                            .py_2()
                                            .font_family("JetBrains Mono")
                                            .text_sm()
                                            .whitespace_nowrap()
                                            .children(terminal_screen_rows.into_iter().enumerate().map(
                                                |(row_index, row)| {
                                                    div()
                                                        .id(("terminal-colored-row", row_index))
                                                        .whitespace_nowrap()
                                                        .line_height(px(17.0))
                                                        .child(
                                                            gpui::StyledText::new(row.text)
                                                                .with_highlights(row.spans.into_iter().map(|span| {
                                                                    let style = span.style;
                                                                    (
                                                                        span.range,
                                                                        gpui::HighlightStyle {
                                                                            color: Some(rgb(style.fg).into()),
                                                                            background_color: Some(rgb(style.bg).into()),
                                                                            font_weight: style.bold.then_some(gpui::FontWeight::BOLD),
                                                                            font_style: style.italic.then_some(gpui::FontStyle::Italic),
                                                                            underline: style.underline.then_some(gpui::UnderlineStyle {
                                                                                thickness: px(1.0),
                                                                                color: Some(rgb(style.fg).into()),
                                                                                wavy: false,
                                                                            }),
                                                                            ..Default::default()
                                                                        },
                                                                    )
                                                                })),
                                                        )
                                                },
                                            ))
                                    } else {
                                        div()
                                            .id("terminal-output-scroll")
                                            .flex_1()
                                            .min_h_0()
                                            .overflow_y_scroll()
                                            .px_3()
                                            .py_2()
                                            .font_family("JetBrains Mono")
                                            .text_sm()
                                            .child(if terminal_output.is_empty() {
                                                "No terminal content yet. Auto updates are enabled by default.".into()
                                            } else {
                                                terminal_output
                                            })
                                    })
                            } else {
                                div().h(px(0.0))
                            }),
                    )
                    .child(
                        div()
                            .w(px(272.0))
                            .flex()
                            .flex_col()
                            .bg(rgb(PANEL))
                            .border_l_1()
                            .border_color(rgb(BORDER))
                            .p_3()
                            .gap_3()
                            .child(div().font_weight(gpui::FontWeight::BOLD).child("Workspace"))
                            .child(
                                div()
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child("Runtime Host"),
                            )
                            .child(div().text_sm().text_color(rgb(MUTED)).child(runtime_status))
                            .child(
                                div()
                                    .id("runtime-status-refresh")
                                    .cursor_pointer()
                                    .text_sm()
                                    .text_color(rgb(TEXT))
                                    .child(if runtime_loading {
                                        "Checking…"
                                    } else {
                                        "Check Host"
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.refresh_runtime(cx);
                                    })),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .child("Terminal sessions"),
                                    )
                                    .child(
                                        div()
                                            .id("terminal-sessions-refresh")
                                            .cursor_pointer()
                                            .text_sm()
                                            .text_color(rgb(TEXT))
                                            .child(if terminal_list_loading {
                                                "Checking…"
                                            } else {
                                                "Reconnect ↻"
                                            })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.refresh_terminals(cx);
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(MUTED))
                                    .child("Read-only · existing Host · all workspaces"),
                            )
                            .child(
                                div()
                                    .id("terminal-sessions-scroll")
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_y_scroll()
                                    .children(terminal_sessions.into_iter().enumerate().map(|(index, (handle, label))| {
                                        div()
                                            .id(("terminal-session", index))
                                            .py_1()
                                            .px_1()
                                            .cursor_pointer()
                                            .text_sm()
                                            .hover(|style| style.bg(rgb(ACTIVE)))
                                            .child(label)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.select_terminal(handle.clone(), cx);
                                            }))
                                    })),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(MUTED))
                                    .child("Diff · pending"),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(MUTED))
                                    .child("Agents · pending"),
                            ),
                    ),
            )
            .child(
                div()
                    .h(px(27.0))
                    .px_3()
                    .flex()
                    .items_center()
                    .justify_between()
                    .bg(rgb(TOOLBAR))
                    .text_sm()
                    .text_color(rgb(MUTED))
                    .child(status)
                    .child(format!("{} · GPUI", branch)),
            )
    }
}

fn main() {
    let session_store = PreviewSessionStore::platform_default();
    let root = std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .or_else(|| {
            session_store.as_ref().and_then(|store| {
                store.last_workspace().unwrap_or_else(|error| {
                    eprintln!("Alera GPUI: cannot load last workspace: {error}");
                    None
                })
            })
        })
        .unwrap_or_else(|| std::env::current_dir().expect("current working directory"));
    let mut model = match WorkbenchModel::new(root) {
        Ok(model) => model,
        Err(error) => {
            eprintln!("Alera GPUI: unable to open workspace: {error}");
            std::process::exit(1);
        }
    };
    if let Some(store) = &session_store {
        // Preserve a malformed snapshot for possible recovery instead of
        // overwriting it with an empty workspace at startup.
        match store.restore(&mut model) {
            Ok(_) => {
                if let Err(error) = store.save(&model) {
                    eprintln!("Alera GPUI: cannot save preview session: {error}");
                }
            }
            Err(error) => {
                eprintln!("Alera GPUI: cannot restore preview session: {error}");
                model.set_status(format!("Previous preview session unavailable: {error}"));
            }
        }
    }
    let runtime_bridge = RuntimeCliBridge::discover().ok();
    let runtime_status = if runtime_bridge.is_some() {
        "Not checked · use existing Alera CLI".into()
    } else {
        "CLI not found · set ALERA_CLI_PATH".into()
    };
    let recent_workspaces = session_store
        .as_ref()
        .and_then(|store| store.recent_workspaces().ok())
        .filter(|roots| !roots.is_empty())
        .unwrap_or_else(|| vec![model.root().to_path_buf()]);

    Application::new().run(move |cx: &mut App| {
        bind_editor_keys(cx);
        let bounds = Bounds::centered(None, size(px(1200.0), px(800.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                let view = cx.new(|_| Workbench {
                    model,
                    editor: None,
                    editor_discard_pending: false,
                    session_store,
                    recent_workspaces,
                    show_recent_workspaces: false,
                    picking_workspace: false,
                    workspace_generation: 0,
                    runtime_bridge,
                    runtime_status,
                    runtime_loading: false,
                    terminals: TerminalDock::default(),
                    terminal_list_loading: false,
                    terminal_read_loading: None,
                    terminal_poll: TerminalPollGate::default(),
                    terminal_tick: 0,
                });
                view.update(cx, |view, cx| view.start_terminal_polling(cx));
                view
            },
        )
        .expect("create native GPUI desktop window");
        cx.activate(true);
    });
}
