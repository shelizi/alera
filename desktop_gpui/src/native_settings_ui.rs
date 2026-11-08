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
        if !matches!(
            section,
            "application" | "terminal" | "editor" | "agents" | "agentProfiles"
        ) {
            return Err("ui_not_ready");
        }
        let store = self.native_settings_store.as_ref().ok_or("ui_not_ready")?;
        let settings = store.read().map_err(|_| "ui_operation_failed")?;
        self.install_native_settings(settings, cx);
        self.settings_section = Some(section.to_owned());
        match section {
            "agents" => self.refresh_terminals(cx),
            "agentProfiles" => self.refresh_agent_profiles(cx),
            _ => {}
        }
        cx.notify();
        Ok(())
    }

    /// Store only a Host-listed Agent Profile ID, never its launch command,
    /// credentials, or Managed configuration. This is a Native-local setting.
    fn change_native_default_agent_profile(
        &mut self,
        next: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if next
            .as_ref()
            .is_some_and(|id| !self.agent_profiles.iter().any(|profile| &profile.id == id))
        {
            self.model
                .set_status("Agent Profile is unavailable; refresh the Host catalog");
            cx.notify();
            return;
        }
        let result = self
            .native_settings_store
            .as_ref()
            .ok_or_else(|| std::io::Error::other("Native settings store unavailable"))
            .and_then(|store| {
                store.compare_and_set_default_agent_profile(
                    self.native_settings.default_agent_profile_id.as_deref(),
                    next.as_deref(),
                )
            });
        match result {
            Ok(settings) => {
                self.install_native_settings(settings, cx);
                if let Some(id) = next {
                    self.agent_selected_profile = Some(id);
                }
                self.model.set_status("Native default Agent Profile saved");
            }
            Err(error) => {
                self.model.set_status(format!(
                    "Native Agent Profile preference unchanged: {error}"
                ));
            }
        }
        cx.notify();
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
        let control_key: u32 = match path {
            "general.autoGitFetchIntervalMinutes" => 1,
            "terminal.fontSize" => 2,
            "editor.tabSize" => 3,
            _ => 0,
        };
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
                            .id(("native-setting-down", control_key))
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
                            .id(("native-setting-up", control_key))
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
        let agent_profiles = self.agent_profiles.clone();
        let selected_profile = self.agent_selected_profile.clone();
        let default_profile = s.default_agent_profile_id.clone();
        let active_agents = self
            .terminals
            .sessions
            .iter()
            .filter(|session| session.running && session.agent_type.is_some())
            .cloned()
            .collect::<Vec<_>>();
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
            "agents" => div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(div().text_sm().child("Running Agent sessions (Runtime Host)"))
                        .child(
                            div()
                                .id("native-agents-refresh")
                                .cursor_pointer()
                                .text_sm()
                                .child("Refresh")
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.refresh_terminals(cx);
                                })),
                        ),
                )
                .child(div().text_xs().text_color(rgb(MUTED)).child(
                    "Attach a Host Agent session without restarting it; sessions show their originating workspace.",
                ))
                .children(active_agents.into_iter().enumerate().map(|(index, agent)| {
                    let handle = agent.handle.clone();
                    div()
                        .id(("native-agent-session", index))
                        .cursor_pointer()
                        .p_3()
                        .bg(rgb(PANEL))
                        .text_sm()
                        .child(format!(
                            "{} · Workspace {} · Tab {}",
                            agent.label(),
                            agent.workspace_id,
                            agent.tab_id
                        ))
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.select_terminal(handle.clone(), cx);
                            if view
                                .terminals
                                .active()
                                .is_some_and(|active| active.handle == handle)
                            {
                                view.settings_section = None;
                            } else {
                                view.model.set_status("Agent session is no longer available");
                            }
                            cx.notify();
                        }))
                }))
                .child(
                    div().text_xs().text_color(rgb(MUTED)).child(
                        if self.terminal_list_loading {
                            "Loading Host sessions…"
                        } else if self
                            .terminals
                            .sessions
                            .iter()
                            .any(|session| session.running && session.agent_type.is_some())
                        {
                            "Showing only Host-confirmed running Agent sessions"
                        } else {
                            "No running Host Agents. Launch one from the Projects Agent panel."
                        },
                    ),
                )
                .into_any_element(),
            "agentProfiles" => div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(div().text_sm().child("Host Agent Profiles"))
                        .child(
                            div()
                                .id("native-agent-profiles-refresh")
                                .cursor_pointer()
                                .text_sm()
                                .child("Refresh")
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.refresh_agent_profiles(cx);
                                })),
                        ),
                )
                .child(div().text_xs().text_color(rgb(MUTED)).child(
                    "Select a profile for the next Agent launch or save its ID as Native's default. Host-owned launch configurations stay in the Host.",
                ))
                .children(agent_profiles.into_iter().enumerate().map(|(index, profile)| {
                    let id = profile.id;
                    let label = format!(
                        "{} · {} · {}",
                        profile.name,
                        profile.agent_type,
                        if profile.managed { "Managed" } else { "Command" }
                    );
                    let selected = selected_profile.as_deref() == Some(id.as_str());
                    let is_default = default_profile.as_deref() == Some(id.as_str());
                    let selection_id = id.clone();
                    div()
                        .id(("native-profile-row", index))
                        .flex()
                        .items_center()
                        .justify_between()
                        .p_3()
                        .bg(rgb(PANEL))
                        .gap_3()
                        .child(
                            div()
                                .id(("native-profile-select", index))
                                .cursor_pointer()
                                .text_sm()
                                .text_color(rgb(if selected { TEXT } else { MUTED }))
                                .child(format!("{} {label}", if selected { "●" } else { "○" }))
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    view.agent_selected_profile = Some(selection_id.clone());
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .id(("native-profile-default", index))
                                .cursor_pointer()
                                .text_xs()
                                .text_color(rgb(TEXT))
                                .child(if is_default { "Clear default" } else { "Make default" })
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    let next = (!is_default).then(|| id.clone());
                                    view.change_native_default_agent_profile(next, cx);
                                })),
                        )
                }))
                .children(
                    default_profile
                        .as_ref()
                        .filter(|id| !self.agent_profiles.iter().any(|profile| profile.id == **id))
                        .map(|id| {
                            div()
                                .id("native-missing-default-profile-clear")
                                .cursor_pointer()
                                .text_sm()
                                .text_color(rgb(TEXT))
                                .child(format!(
                                    "Saved default {id} is no longer in the Host catalog · Clear"
                                ))
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.change_native_default_agent_profile(None, cx);
                                }))
                        }),
                )
                .child(div().text_xs().text_color(rgb(MUTED)).child(
                    if self.agent_profiles_loading {
                        "Loading Host Agent Profiles…".to_owned()
                    } else {
                        self.agent_status.clone()
                    },
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
                            ["application", "terminal", "editor", "agents", "agentProfiles"]
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
