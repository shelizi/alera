//! The independent Native settings view uses the same on-disk owner as
//! Host-authorized MCP commands. A visible screen must precede open_settings
//! success, and every local change is a durable compare-and-set.
use super::*;

impl Workbench {
    pub(super) fn show_native_settings_section(
        &mut self,
        section: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), &'static str> {
        if !matches!(section, "application" | "terminal" | "editor") {
            return Err("ui_not_ready");
        }
        let store = self.native_settings_store.as_ref().ok_or("ui_not_ready")?;
        let settings = store.read().map_err(|_| "ui_operation_failed")?;
        self.install_native_settings(settings, cx);
        self.settings_section = Some(section.to_owned());
        cx.notify();
        Ok(())
    }

    fn native_settings_bool_row(
        &self,
        label: &'static str,
        path: &'static str,
        value: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .p_3()
            .bg(rgb(PANEL))
            .child(div().text_sm().child(label))
            .child(
                div()
                    .id(path)
                    .cursor_pointer()
                    .text_sm()
                    .text_color(rgb(TEXT))
                    .child(if value {
                        "On · change"
                    } else {
                        "Off · change"
                    })
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.change_native_setting(path, serde_json::json!(!value), cx);
                    })),
            )
            .into_any_element()
    }

    fn native_settings_number_row(
        &self,
        label: &'static str,
        path: &'static str,
        value: f64,
        step: f64,
        min: f64,
        max: f64,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let previous = (value - step).max(min);
        let next = (value + step).min(max);
        let integer = path != "terminal.fontSize";
        div()
            .flex()
            .items_center()
            .justify_between()
            .p_3()
            .bg(rgb(PANEL))
            .child(div().text_sm().child(label))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .id("native-setting-down")
                            .cursor_pointer()
                            .text_color(rgb(TEXT))
                            .child("−")
                            .on_click(cx.listener(move |view, _, _, cx| {
                                let candidate = if integer {
                                    serde_json::json!(previous as u64)
                                } else {
                                    serde_json::json!(previous)
                                };
                                view.change_native_setting(path, candidate, cx);
                            })),
                    )
                    .child(div().text_sm().child(format!("{value}")))
                    .child(
                        div()
                            .id("native-setting-up")
                            .cursor_pointer()
                            .text_color(rgb(TEXT))
                            .child("+")
                            .on_click(cx.listener(move |view, _, _, cx| {
                                let candidate = if integer {
                                    serde_json::json!(next as u64)
                                } else {
                                    serde_json::json!(next)
                                };
                                view.change_native_setting(path, candidate, cx);
                            })),
                    ),
            )
            .into_any_element()
    }

    /// Render the requested section above the workbench, not merely set a
    /// model flag. Returns None while settings are closed.
    pub(super) fn render_native_settings_panel(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let section = self.settings_section.as_deref()?;
        let s = &self.native_settings;
        let rows = match section {
            "application" => div()
                .flex()
                .flex_col()
                .gap_2()
                .child(self.native_settings_bool_row(
                    "Automatically fetch Git references",
                    "general.autoGitFetchEnabled",
                    s.auto_git_fetch_enabled,
                    cx,
                ))
                .child(self.native_settings_number_row(
                    "Auto Git fetch interval (minutes)",
                    "general.autoGitFetchIntervalMinutes",
                    s.auto_git_fetch_interval_minutes as f64,
                    1.0,
                    1.0,
                    60.0,
                    cx,
                ))
                .child(div().mt_2().text_sm().text_color(rgb(MUTED)).child(format!(
                    "MCP read-only safety: project removal confirmation: {}; workspace removal confirmation: {}; automatic archive after {} day(s)",
                    s.confirm_project_removal,
                    s.confirm_workspace_removal,
                    s.auto_archive_workspaces_after_days,
                )))
                .into_any_element(),
            "terminal" => div()
                .child(self.native_settings_number_row(
                    "Terminal text size (px)",
                    "terminal.fontSize",
                    s.terminal_font_size,
                    1.0,
                    8.0,
                    48.0,
                    cx,
                ))
                .into_any_element(),
            "editor" => div()
                .flex()
                .flex_col()
                .gap_2()
                .child(self.native_settings_bool_row(
                    "Autosave idle file-backed editor buffers after 3 seconds",
                    "editor.autosaveEnabled",
                    s.editor_autosave_enabled,
                    cx,
                ))
                .child(self.native_settings_number_row(
                    "Spaces inserted with Tab",
                    "editor.tabSize",
                    s.editor_tab_size as f64,
                    1.0,
                    1.0,
                    16.0,
                    cx,
                ))
                .into_any_element(),
            _ => return None,
        };
        let load_error = self.native_settings_error.clone();
        Some(
            div()
                .id("native-settings-panel")
                .absolute()
                .inset_0()
                .flex()
                .flex_col()
                .bg(rgb(BG))
                .text_color(rgb(TEXT))
                .p_4()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .items_center()
                        .child(div().text_sm().font_weight(gpui::FontWeight::BOLD)
                            .child(format!("Alera Native Settings · {section}")))
                        .child(
                            div()
                                .id("native-settings-close")
                                .cursor_pointer()
                                .text_sm()
                                .child("Close ✕")
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.settings_section = None;
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .gap_3()
                        .children(
                            ["application", "terminal", "editor"]
                                .into_iter()
                                .map(|target| {
                                    div()
                                        .id(target)
                                        .cursor_pointer()
                                        .text_sm()
                                        .text_color(rgb(if section == target { TEXT } else { MUTED }))
                                        .child(target)
                                        .on_click(cx.listener(move |view, _, _, cx| {
                                            let _ = view.show_native_settings_section(target, cx);
                                        }))
                                }),
                        ),
                )
                .child(div().id("native-settings-rows").flex_1().overflow_y_scroll().child(rows))
                .children(load_error.map(|error| {
                    div().text_sm().child(format!("Settings load error: {error}"))
                }))
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child("Native preview preferences live in a separate app state directory and never change Alera Classic."),
                )
                .into_any_element(),
        )
    }
}
