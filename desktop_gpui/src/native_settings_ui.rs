//! The independent Native settings view uses the same on-disk owner as
//! Host-authorized MCP commands. A visible screen must precede open_settings
//! success, and every local change is a durable compare-and-set.
use super::*;
use alera_gpui_desktop::native_i18n::tr;
use alera_gpui_desktop::settings_menu_catalog::{
    has_connected_native_controls, matching_sections_localized, section as catalog_section,
    SettingsNavGroup, SettingsSection,
};
use gpui_kit::component::switch::Switch;

impl Workbench {
    fn change_native_two_row_tabs(&mut self, next: bool, cx: &mut Context<Self>) {
        self.change_shared_desktop_setting("editor.twoRowTabs", serde_json::json!(next), cx);
    }

    pub(super) fn open_kit_native_settings_dialog(
        owner: gpui::WeakEntity<Self>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let first_focus = std::cell::Cell::new(true);
        window.open_dialog(cx, move |dialog, window, cx| {
            if first_focus.replace(false) {
                if let Some(entity) = owner.upgrade() {
                    let focus = entity.read(cx).settings_modal_focus.clone();
                    window.focus(&focus, cx);
                }
            }
            let viewport = window.viewport_size();
            let locale = owner
                .upgrade()
                .map(|entity| entity.read(cx).settings_menu_locale)
                .unwrap_or_default();
            let metrics = NativeModalSize::Settings
                .resolve(f32::from(viewport.width), f32::from(viewport.height));
            let content = owner
                .upgrade()
                .and_then(|entity| {
                    entity.update(cx, |view, cx| {
                        view.render_native_settings_panel(
                            f32::from(viewport.width),
                            f32::from(viewport.height),
                            cx,
                        )
                    })
                })
                .unwrap_or_else(|| div().into_any_element());
            let for_cancel = owner.clone();
            dialog
                .title(tr(locale, "Settings"))
                .w(px(metrics.width))
                .centered()
                .close_button(false)
                .overlay_closable(true)
                .on_cancel(move |_, _, cx| {
                    let _ = for_cancel.update(cx, |view, cx| {
                        view.settings_section = None;
                        view.settings_kit_dialog_open = false;
                        cx.notify();
                    });
                    true
                })
                .on_ok(|_, _, _| false)
                .child(content)
        });
    }

    pub(super) fn show_native_settings_section(
        &mut self,
        section: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), &'static str> {
        if section != "historySync" && catalog_section(section).is_none() {
            return Err("ui_not_ready");
        }
        // Existing connected Native controls now read the same Rust Runtime
        // Store as Classic. Never overlay them with stale native-settings.json.
        if has_connected_native_controls(section) {
            self.refresh_shared_desktop_settings(cx);
        }
        if self.settings_section.is_none() {
            self.settings_menu_search
                .update(cx, |input, cx| input.clear(cx));
        }
        self.explorer_context_menu = None;
        self.mixed_tab_context = None;
        self.quick_open = None;
        if self.explorer_action.is_some() {
            self.cancel_explorer_action(cx);
        }
        self.settings_section = Some(section.to_owned());
        match section {
            "agents" => self.refresh_terminals(cx),
            "historySync" => self.refresh_shared_history_sync(cx),
            "agentProfiles" => self.refresh_agent_profiles(cx),
            _ => {}
        }
        cx.notify();
        Ok(())
    }

    pub(super) fn dismiss_native_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings_section = None;
        self.settings_kit_dialog_open = false;
        window.close_dialog(cx);
        if self.center_view == CenterView::Editor {
            if let Some(editor) = self.focused_editor() {
                window.focus(&editor.read(cx).focus_handle(cx), cx);
            } else {
                window.focus(&self.explorer_keyboard_focus, cx);
            }
        } else {
            window.focus(&self.terminal_input.read(cx).focus_handle(cx), cx);
        }
        cx.notify();
    }

    fn native_settings_nav_item(
        &self,
        id: &'static str,
        label: &'static str,
        active: bool,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        div()
            .id(id)
            .cursor_pointer()
            .rounded_md()
            .flex()
            .items_center()
            .px_3()
            .py_2()
            .bg(rgb(if active { ACTIVE } else { PANEL }))
            .text_color(rgb(if active { TEXT } else { MUTED }))
            .text_sm()
            .when(!compact, |row| row.w_full())
            .child(tr(self.settings_menu_locale, label))
            .on_click(cx.listener(move |view, _, _window, cx| {
                if let Err(code) = view.show_native_settings_section(id, cx) {
                    view.model
                        .set_status(format!("Native settings unavailable: {code}"));
                }
                cx.notify();
            }))
            .into_any_element()
    }

    /// Store only a Host-listed Agent Profile ID in the shared Rust owner.
    /// Never copy commands, credentials or Managed config into Native JSON.
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
        self.change_shared_desktop_setting(
            "agents.defaultAgentProfileId",
            serde_json::json!(next),
            cx,
        );
    }

    /// Native-only executable override; Host and Classic preferences remain
    /// untouched. Validate/write/read back under the store's local CAS lock
    /// on a worker thread, never while painting or holding a GPUI Window.
    fn save_native_agent_executable_path(&mut self, clear: bool, cx: &mut Context<Self>) {
        if self.agent_override_busy {
            return;
        }
        let Some(id) = self.agent_override_selected_id.clone() else {
            self.agent_override_status = "Select an Agent type first".into();
            cx.notify();
            return;
        };
        let Some(store) = self.native_settings_store.clone() else {
            self.agent_override_status = "Native settings are not available".into();
            cx.notify();
            return;
        };
        let proposed = (!clear)
            .then(|| self.agent_override_input.read(cx).query().trim().to_owned())
            .filter(|path| !path.is_empty());
        let expected = self
            .native_settings
            .agent_executable_paths
            .get(&id)
            .cloned();
        self.agent_override_request = self.agent_override_request.wrapping_add(1);
        let request = self.agent_override_request;
        self.agent_override_busy = true;
        self.agent_override_status = "Validating local Agent executable…".into();
        cx.spawn(async move |view, cx| {
            let result = cx.background_spawn(async move {
                store.compare_and_set_agent_executable_path(
                    &id, expected.as_deref(), proposed.as_deref(),
                )
            }).await;
            view.update(cx, |view, cx| {
                if view.agent_override_request != request {
                    return;
                }
                view.agent_override_busy = false;
                match result {
                    Ok(updated) => {
                        view.native_settings = updated;
                        if clear {
                            view.agent_override_input.update(cx, |input, cx| input.clear(cx));
                        }
                        view.agent_override_status = "Agent executable preference saved".into();
                        view.model.set_status("Local Agent executable setting updated");
                        // Cancel any older PATH scan started before the setting
                        // was saved; it must not overwrite the new detection.
                        view.installed_agent_clis_loading = false;
                        view.installed_agent_clis_request =
                            view.installed_agent_clis_request.wrapping_add(1);
                        view.refresh_installed_agent_clis(cx);
                    }
                    Err(error) => {
                        view.agent_override_status = format!(
                            "Could not save Agent executable: {error}. Use a real absolute executable file.",
                        );
                    }
                }
                cx.notify();
            }).ok();
        }).detach();
        cx.notify();
    }

    pub(super) fn native_settings_bool_row(
        &self,
        label: &'static str,
        path: &'static str,
        value: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        div()
            .p_3()
            .bg(rgb(PANEL))
            .child(
                Switch::new(path)
                    .label(tr(self.settings_menu_locale, label))
                    .checked(value)
                    .on_change(cx.listener(move |view, checked, _, cx| {
                        view.change_native_setting(path, serde_json::json!(*checked), cx);
                    })),
            )
            .into_any_element()
    }

    pub(super) fn native_settings_number_row(
        &self,
        label: &'static str,
        path: &'static str,
        value: f64,
        step: f64,
        min: f64,
        max: f64,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        // Decimal controls should not accumulate IEEE-754 display artifacts
        // (e.g. 0.7000000000000001 after two opacity clicks).
        let previous = ((value - step).max(min) * 100.0).round() / 100.0;
        let next = ((value + step).min(max) * 100.0).round() / 100.0;
        let integer = !matches!(
            path,
            "terminal.fontSize" | "terminal.lineHeight" | "terminal.backgroundOpacity"
        );
        let control_key: u32 = match path {
            "general.autoGitFetchIntervalMinutes" => 1,
            "terminal.fontSize" => 2,
            "editor.tabSize" => 3,
            "terminal.lineHeight" => 4,
            "terminal.backgroundOpacity" => 5,
            "terminal.tuiScrollSensitivity" => 6,
            "editor.autosaveDelaySeconds" => 7,
            _ => 0,
        };
        div()
            .flex()
            .items_center()
            .justify_between()
            .p_3()
            .bg(rgb(PANEL))
            .child(div().text_sm().child(tr(self.settings_menu_locale, label)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        native_components::action_button(
                            ("native-setting-down", control_key),
                            "−",
                            false,
                            false,
                        )
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
                        native_components::action_button(
                            ("native-setting-up", control_key),
                            "+",
                            false,
                            false,
                        )
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
        viewport_width: f32,
        viewport_height: f32,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let requested = self.settings_section.as_deref()?;
        let locale = self.settings_menu_locale;
        let query = self.settings_menu_search.read(cx).query().to_owned();
        let visible = matching_sections_localized(&query, locale);
        let selected: Option<&'static SettingsSection> = catalog_section(requested)
            .filter(|selected| visible.iter().any(|entry| entry.id == selected.id))
            .or_else(|| visible.first().copied());
        let section = selected.map(|selected| selected.id).unwrap_or("");
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
                    "Automatically Fetch Git Remotes",
                    "general.autoGitFetchEnabled",
                    s.auto_git_fetch_enabled,
                    cx,
                ))
                .child(self.native_settings_number_row(
                    "Auto Git Fetch Interval",
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
                    "Font Size",
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
                    "Autosave",
                    "editor.autosaveEnabled",
                    s.editor_autosave_enabled,
                    cx,
                ))
                .child(
                    div()
                        .p_3()
                        .bg(rgb(PANEL))
                        .child(
                            Switch::new("native-editor-two-row-tabs")
                                .label(tr(locale, "Two-row tabs"))
                                .checked(s.editor_two_row_tabs)
                                .on_change(cx.listener(|view, checked, _, cx| {
                                    view.change_native_two_row_tabs(*checked, cx);
                                })),
                        ),
                )
                .child(self.native_settings_number_row(
                    "Tab Size",
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
                        .child(div().text_sm().child(tr(locale, "Running Agent sessions (Runtime Host)")))
                        .child(
                            div()
                                .id("native-agents-refresh")
                                .cursor_pointer()
                                .text_sm()
                                .child(tr(locale, "Refresh"))
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
                        .on_click(cx.listener(move |view, _, window, cx| {
                            view.select_terminal(handle.clone(), cx);
                            if view
                                .terminals
                                .active()
                                .is_some_and(|active| active.handle == handle)
                            {
                                view.dismiss_native_settings(window, cx);
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
                        .child(div().text_sm().child(tr(locale, "Host Agent Profiles")))
                        .child(
                            div()
                                .id("native-agent-profiles-refresh")
                                .cursor_pointer()
                                .text_sm()
                                .child(tr(locale, "Refresh"))
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
                        if profile.managed { tr(locale, "Managed") } else { tr(locale, "Command") }
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
                .child(div().mt_3().text_sm().child("Local Agent executables (Classic parity)"))
                .child(div().text_xs().text_color(rgb(MUTED)).child(
                    "Choose an Agent, then set an absolute executable file path. Blank uses PATH. Native-only; does not change Classic or Host Profiles.",
                ))
                .children(alera_core::agent_descriptor::AGENT_DESCRIPTORS
                    .iter()
                    .enumerate()
                    .map(|(index, descriptor)| {
                        let id = descriptor.id.to_owned();
                        let is_selected = self.agent_override_selected_id.as_deref() == Some(descriptor.id);
                        let path = self.native_settings.agent_executable_paths.get(descriptor.id);
                        let label = format!(
                            "{} {} · {}",
                            if is_selected { "●" } else { "○" },
                            descriptor.display_name,
                            path.map(|path| {
                                if path.chars().count() > 72 {
                                    format!("{}…", path.chars().take(70).collect::<String>())
                                } else {
                                    path.clone()
                                }
                            }).unwrap_or_else(|| "Auto PATH".into()),
                        );
                        div()
                            .id(("agent-executable-row", index))
                            .cursor_pointer()
                            .text_xs()
                            .text_color(rgb(if is_selected { TEXT } else { MUTED }))
                            .hover(|style| style.bg(rgb(ACTIVE)))
                            .p_1()
                            .child(label)
                            .on_click(cx.listener(move |view, _, _, cx| {
                                let path = view.native_settings.agent_executable_paths.get(&id)
                                    .cloned().unwrap_or_default();
                                view.agent_override_selected_id = Some(id.clone());
                                view.agent_override_input.update(cx, |input, cx| {
                                    input.set_query(&path, cx);
                                });
                                view.agent_override_status = format!("Selected {id}: edit or clear its local executable");
                                cx.notify();
                            }))
                    }))
                .child(self.agent_override_input.clone())
                .child(div()
                    .flex()
                    .gap_3()
                    .child(div()
                        .id("native-agent-executable-save")
                        .cursor_pointer()
                        .text_sm()
                        .text_color(rgb(if self.agent_override_busy { MUTED } else { TEXT }))
                        .child("Save executable path")
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.save_native_agent_executable_path(false, cx);
                        })))
                    .child(div()
                        .id("native-agent-executable-clear")
                        .cursor_pointer()
                        .text_sm()
                        .text_color(rgb(TEXT))
                        .child("Clear override")
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.save_native_agent_executable_path(true, cx);
                        }))))
                .child(div().text_xs().text_color(rgb(MUTED)).child(
                    if self.agent_override_busy {
                        "Saving Native Agent executable…".to_owned()
                    } else {
                        self.agent_override_status.clone()
                    },
                ))
                .into_any_element(),
            "historySync" => self.render_history_sync_settings(cx),
            _ => div().into_any_element(),
        };
        let metrics = NativeModalSize::Settings.resolve(viewport_width, viewport_height);
        let compact = metrics.stacked_navigation;
        // Existing shared settings are rendered directly through the catalog.
        // The separately Host-owned History Sync and Agent Profile panels
        // remain connected Native controls rather than inert catalog previews.
        let has_native_controls = matches!(section, "agents" | "agentProfiles" | "historySync");
        let navigation = div()
            .id("native-settings-sidebar")
            .flex()
            .flex_col()
            .bg(rgb(PANEL))
            .when(compact, |sidebar| {
                sidebar
                    .w_full()
                    .h(px(215.0))
                    .border_b_1()
                    .border_color(rgb(BORDER))
            })
            .when(!compact, |sidebar| {
                sidebar
                    .w(px(260.0))
                    .h_full()
                    .border_r_1()
                    .border_color(rgb(BORDER))
            })
            .child(
                div()
                    .h(px(48.0))
                    .px_4()
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(tr(locale, "Settings")),
            )
            .child(self.render_settings_language_preview(cx))
            .child(
                div()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(BORDER))
                            .bg(rgb(TOOLBAR))
                            .px_2()
                            .child(self.settings_menu_search.clone()),
                    ),
            )
            .child(
                div()
                    .id("native-settings-nav")
                    .flex()
                    .gap_1()
                    .p_2()
                    .when(compact, |nav| nav.flex_row().overflow_x_scroll())
                    .when(!compact, |nav| {
                        nav.flex_col().flex_1().min_h_0().overflow_y_scroll()
                    })
                    .children(
                        [SettingsNavGroup::Preferences, SettingsNavGroup::Resources]
                            .into_iter()
                            .enumerate()
                            .map(|(group_index, group)| {
                                let items = visible
                                    .iter()
                                    .copied()
                                    .filter(|item| item.nav_group == group)
                                    .collect::<Vec<_>>();
                                div()
                                    .id(("native-settings-nav-group", group_index))
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .px_3()
                                            .py_1()
                                            .text_xs()
                                            .text_color(rgb(MUTED))
                                            .child(tr(locale, group.title())),
                                    )
                                    .children(items.into_iter().map(|item| {
                                        self.native_settings_nav_item(
                                            item.id,
                                            item.title,
                                            section == item.id,
                                            compact,
                                            cx,
                                        )
                                    }))
                            }),
                    ),
            );
        let heading = selected
            .map(|selected| tr(locale, selected.title))
            .unwrap_or_else(|| tr(locale, "Settings"));
        let preview = selected
            .map(|selected| self.render_settings_catalog_preview(selected, locale, cx))
            .unwrap_or_else(|| {
                div()
                    .text_sm()
                    .text_color(rgb(MUTED))
                    .child(tr(locale, "No matching settings."))
                    .into_any_element()
            });
        let content = div()
            .id("native-settings-content")
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .px_4()
                    .py_3()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_lg()
                                    .child(heading),
                            )
                            .child(
                                icon_button(
                                    "native-settings-close",
                                    UiIcon::Close,
                                    "Close settings",
                                    false,
                                )
                                .on_click(cx.listener(
                                    |view, _, window, cx| {
                                        view.dismiss_native_settings(window, cx);
                                    },
                                )),
                            ),
                    )
                    .children(selected.map(|selected| {
                        div()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child(tr(locale, selected.description))
                    }))
                    .children(selected.filter(|selected| selected.show_group_chips).map(
                        |selected| {
                            div().flex().flex_wrap().gap_2().children(
                                selected.groups.iter().enumerate().map(|(index, group)| {
                                    div()
                                        .id(("native-settings-group-chip", index))
                                        .rounded_md()
                                        .border_1()
                                        .border_color(rgb(BORDER))
                                        .bg(rgb(PANEL))
                                        .px_2()
                                        .py_1()
                                        .text_xs()
                                        .text_color(rgb(MUTED))
                                        .child(tr(locale, group.title))
                                }),
                            )
                        },
                    )),
            )
            .child(
                div()
                    .id("native-settings-rows")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(preview)
                    .children(has_native_controls.then(|| {
                        div()
                            .id("native-settings-connected-controls")
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(tr(locale, "Connected Native Controls")),
                            )
                            .child(rows)
                    })),
            )
            .children(self.native_settings_error.clone().map(|error| {
                div()
                    .px_4()
                    .text_sm()
                    .text_color(rgb(MUTED))
                    .child(format!("Settings load error: {error}"))
            }))
            .children(self.settings_shared_error.clone().map(|error| {
                div()
                    .px_4()
                    .py_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .text_color(rgb(MUTED))
                            .child(format!("Classic / GPUI settings sync unavailable: {error}")),
                    )
                    .child(
                        native_components::action_button(
                            "native-settings-host-retry",
                            "Retry",
                            false,
                            false,
                        )
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.refresh_shared_desktop_settings(cx);
                        })),
                    )
            }))
            .child(
                div()
                    .border_t_1()
                    .border_color(rgb(BORDER))
                    .px_4()
                    .py_2()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(if section == "historySync" {
                        "Agent History Sync is shared through the Runtime Host.".to_owned()
                    } else {
                        tr(locale,
                            "Connected controls save to shared Rust Core · other items are unavailable in GPUI",
                        ).to_string()
                    }),
            );
        let body = div()
            .id("native-settings-panel")
            .flex()
            .flex_1()
            // Reserve the Kit dialog title/chrome height while keeping the
            // classic 92%-of-viewport Settings body scrollable.
            .h(px((metrics.height - 72.0).max(0.0)))
            .min_h_0()
            .key_context("AleraNativeModal")
            .track_focus(&self.settings_modal_focus)
            .when(compact, |body| body.flex_col())
            .when(!compact, |body| body.flex_row())
            .child(navigation)
            .child(content);
        Some(body.into_any_element())
    }
}
