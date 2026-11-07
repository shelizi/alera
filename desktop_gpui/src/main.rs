//! Experimental Alera desktop shell powered by native GPUI (no Flutter/WebView).
use alera_gpui_desktop::model::WorkbenchModel;
use alera_gpui_desktop::runtime_bridge::RuntimeCliBridge;
use alera_gpui_desktop::session::PreviewSessionStore;
use alera_gpui_desktop::terminal::TerminalDock;
use gpui::{
    div, prelude::*, px, rgb, size, App, Application, Bounds, Context, Window, WindowBounds,
    WindowOptions,
};

const BG: u32 = 0x15171b;
const PANEL: u32 = 0x1c1f24;
const TOOLBAR: u32 = 0x252931;
const BORDER: u32 = 0x343942;
const TEXT: u32 = 0xe1e4e8;
const MUTED: u32 = 0xa6adb7;
const ACTIVE: u32 = 0x303741;

struct Workbench {
    model: WorkbenchModel,
    session_store: Option<PreviewSessionStore>,
    runtime_bridge: Option<RuntimeCliBridge>,
    runtime_status: String,
    runtime_loading: bool,
    terminals: TerminalDock,
    terminal_list_loading: bool,
    terminal_read_loading: Option<String>,
}

impl Workbench {
    fn persist_session(&mut self) {
        if let Some(store) = &self.session_store {
            if let Err(error) = store.save(&self.model) {
                eprintln!("Alera GPUI: unable to persist preview session: {error}");
                self.model
                    .set_status(format!("Preview state not saved: {error}"));
            }
        }
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
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.terminals.status = "Cannot list sessions: install experimental Alera CLI".into();
            cx.notify();
            return;
        };
        self.terminal_list_loading = true;
        self.terminals.status = "Listing existing Host sessions…".into();
        cx.notify();
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move { bridge.list_terminals() })
                .await;
            view.update(cx, |view, cx| {
                view.terminal_list_loading = false;
                match result {
                    Ok(sessions) => view.terminals.update_sessions(sessions),
                    Err(error) => {
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
        let Some((handle, cursor)) = self.terminals.read_request() else {
            return;
        };
        self.terminal_read_loading = Some(handle.clone());
        cx.notify();
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move { bridge.read_terminal(&handle, cursor) })
                .await;
            view.update(cx, |view, cx| {
                view.terminal_read_loading = None;
                match result {
                    Ok(chunk) => {
                        view.terminals.apply_chunk(chunk);
                    }
                    Err(error) => {
                        view.terminals.status = format!("Terminal read failed: {error}");
                    }
                }
                cx.notify();
            })
            .ok();
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
        let status = self.model.status().to_owned();
        let branch = self.model.branch().to_owned();
        let can_up = self.model.can_go_up();
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
            .map(|tab| tab.output.clone())
            .unwrap_or_default();
        let terminal_status = self.terminals.status.clone();
        let has_terminal_preview = self.terminals.active().is_some();

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
                                    .h(px(24.0))
                                    .px_4()
                                    .bg(rgb(BG))
                                    .text_sm()
                                    .text_color(rgb(MUTED))
                                    .child(format!("{} · read-only preview", filename)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_h_0()
                                    .id("preview-scroll")
                                    .overflow_y_scroll()
                                    .p_4()
                                    .text_sm()
                                    .font_family("JetBrains Mono")
                                    .child(content),
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
                                    .child(
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
                                                "No output available. Select Read more to retry.".into()
                                            } else {
                                                terminal_output
                                            }),
                                    )
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
                                    .child("Read-only · existing Host only"),
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

    Application::new().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1200.0), px(800.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| Workbench {
                    model,
                    session_store,
                    runtime_bridge,
                    runtime_status,
                    runtime_loading: false,
                    terminals: TerminalDock::default(),
                    terminal_list_loading: false,
                    terminal_read_loading: None,
                })
            },
        )
        .expect("create native GPUI desktop window");
        cx.activate(true);
    });
}
