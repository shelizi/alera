//! Experimental Alera desktop shell powered by native GPUI (no Flutter/WebView).
mod model;

use gpui::{
    div, prelude::*, px, rgb, size, App, Application, Bounds, Context, Window, WindowBounds,
    WindowOptions,
};
use model::WorkbenchModel;

const BG: u32 = 0x15171b;
const PANEL: u32 = 0x1c1f24;
const TOOLBAR: u32 = 0x252931;
const BORDER: u32 = 0x343942;
const TEXT: u32 = 0xe1e4e8;
const MUTED: u32 = 0xa6adb7;
const ACTIVE: u32 = 0x303741;

struct Workbench {
    model: WorkbenchModel,
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
        let status = self.model.status().to_owned();
        let branch = self.model.branch().to_owned();
        let can_up = self.model.can_go_up();

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
                                            .id("up")
                                            .cursor_pointer()
                                            .text_color(rgb(MUTED))
                                            .child(if can_up { "↑ Up" } else { "Root" })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.model.go_up();
                                                cx.notify();
                                            })),
                                    ),
                            )
                            .child(div().flex_1().min_h_0().overflow_y_scroll().children(
                                entries.into_iter().enumerate().map(|(i, item)| {
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
                                            cx.notify();
                                        }))
                                }),
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .h(px(37.0))
                                    .px_4()
                                    .flex()
                                    .items_center()
                                    .bg(rgb(PANEL))
                                    .border_b_1()
                                    .border_color(rgb(BORDER))
                                    .child(filename),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_y_scroll()
                                    .p_4()
                                    .text_sm()
                                    .font_family("JetBrains Mono")
                                    .child(content),
                            ),
                    )
                    .child(
                        div()
                            .w(px(210.0))
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
                                    .text_sm()
                                    .text_color(rgb(MUTED))
                                    .child("Terminal · pending"),
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
    let root = std::env::args_os().nth(1).unwrap_or_else(|| {
        std::env::current_dir()
            .expect("current working directory")
            .into_os_string()
    });
    let model = match WorkbenchModel::new(root) {
        Ok(model) => model,
        Err(error) => {
            eprintln!("Alera GPUI: unable to open workspace: {error}");
            std::process::exit(1);
        }
    };

    Application::new().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1200.0), px(800.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| Workbench { model }),
        )
        .expect("create native GPUI desktop window");
        cx.activate(true);
    });
}
