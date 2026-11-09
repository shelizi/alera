//! Authenticated Classic/Native shared settings bridge. All CLI calls run on
//! GPUI's background executor and are field-scoped. The older Native JSON file
//! still owns Explorer visibility and Native-only state, never shared scalars.
use super::*;

impl Workbench {
    fn reject_shared_mcp_setting(
        &mut self,
        request_id: &str,
        code: &'static str,
        cx: &mut Context<Self>,
    ) {
        self.mcp_ui_status = format!("MCP: shared settings rejected ({code})");
        if let Some(bridge) = self.mcp_ui_bridge.as_mut() {
            let _ = bridge.complete(request_id, false, None, Some(code));
        }
        cx.notify();
    }

    pub(super) fn apply_shared_mcp_setting(
        &mut self,
        command: NativeMcpUiCommand,
        cx: &mut Context<Self>,
    ) {
        let authorized = self.mcp_host_workspaces.iter().any(|workspace| {
            workspace.id == command.workspace_id
                && dunce::canonicalize(&workspace.path).is_ok_and(|path| path.is_dir())
        });
        if !authorized {
            self.reject_shared_mcp_setting(&command.request_id, "workspace_missing", cx);
            return;
        }
        let Some(path) = command
            .arguments
            .get("path")
            .and_then(|value| value.as_str())
        else {
            self.reject_shared_mcp_setting(&command.request_id, "ui_operation_failed", cx);
            return;
        };
        if !NativeSettings::writable(path) {
            self.reject_shared_mcp_setting(&command.request_id, "ui_not_ready", cx);
            return;
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.reject_shared_mcp_setting(&command.request_id, "ui_not_ready", cx);
            return;
        };
        let operation = if command.action == "get_setting" {
            None
        } else if command.action == "update_setting" {
            match (
                command.arguments.get("expectedValue"),
                command.arguments.get("value"),
            ) {
                (Some(expected), Some(value)) => Some((expected.clone(), value.clone())),
                _ => {
                    self.reject_shared_mcp_setting(&command.request_id, "ui_operation_failed", cx);
                    return;
                }
            }
        } else {
            self.reject_shared_mcp_setting(&command.request_id, "ui_not_ready", cx);
            return;
        };
        let path = path.to_owned();
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    match operation {
                        Some((expected, next)) => {
                            bridge.desktop_settings_compare_and_set(&path, &expected, &next)
                        }
                        None => bridge.desktop_settings_snapshot(),
                    }
                })
                .await;
            view.update(cx, |view, cx| match result {
                Ok(snapshot) => {
                    let path = command.arguments["path"].as_str().unwrap_or_default();
                    let readback = snapshot["values"].get(path).cloned();
                    if let (Ok(()), Some(readback)) = (
                        view.install_shared_desktop_snapshot(&snapshot, cx),
                        readback,
                    ) {
                        if let Some(bridge) = view.mcp_ui_bridge.as_mut() {
                            if bridge
                                .complete_setting(&command.request_id, &readback)
                                .is_ok()
                            {
                                view.mcp_ui_status = format!(
                                    "MCP: Native {} shared readback confirmed",
                                    command.action
                                );
                                cx.notify();
                                return;
                            }
                        }
                    }
                    view.reject_shared_mcp_setting(&command.request_id, "ui_operation_failed", cx);
                }
                Err(_) => {
                    view.reject_shared_mcp_setting(&command.request_id, "ui_operation_failed", cx)
                }
            })
            .ok();
        })
        .detach();
    }

    fn install_shared_desktop_snapshot(
        &mut self,
        snapshot: &serde_json::Value,
        cx: &mut Context<Self>,
    ) -> std::io::Result<()> {
        let mut settings = self.native_settings.clone();
        let language = settings.apply_shared_snapshot(snapshot)?;
        let values = snapshot["values"].as_object().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "Shared values missing")
        })?;
        let extended = values
            .iter()
            .filter(|(path, _)| alera_core::desktop_settings_contract::extended_portable(path))
            .map(|(path, value)| (path.clone(), value.clone()))
            .collect();
        if self.agent_selected_profile.is_none() {
            self.agent_selected_profile = settings.default_agent_profile_id.clone();
        }
        self.settings_menu_language = language;
        self.settings_extra_values = extended;
        self.settings_menu_locale = UiLocale::from_preference(&self.settings_menu_language);
        let locale = self.settings_menu_locale;
        self.settings_menu_search.update(cx, |search, cx| {
            search.set_placeholder(
                alera_gpui_desktop::native_i18n::tr(locale, "Search settings"),
                cx,
            );
        });
        self.install_native_settings(settings, cx);
        Ok(())
    }

    pub(super) fn refresh_shared_desktop_settings(&mut self, cx: &mut Context<Self>) {
        if self.settings_shared_busy {
            return;
        }
        if self.runtime_bridge.is_none() {
            self.runtime_bridge = RuntimeCliBridge::discover().ok();
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.settings_shared_error =
                Some("Shared settings CLI unavailable (ALERA_CLI_PATH)".into());
            cx.notify();
            return;
        };
        let migration_needed = !self.settings_native_profile_migrated;
        let (old_native_profile, old_hidden, old_ignored) = if migration_needed {
            match self
                .native_settings_store
                .as_ref()
                .map(|store| store.read())
            {
                Some(Ok(stored)) => (
                    stored.default_agent_profile_id,
                    stored.explorer_show_hidden,
                    stored.explorer_show_ignored,
                ),
                Some(Err(error)) => {
                    self.settings_shared_error = Some(format!(
                        "Cannot safely migrate old Native preferences: {error}"
                    ));
                    cx.notify();
                    return;
                }
                None => (
                    None,
                    self.native_settings.explorer_show_hidden,
                    self.native_settings.explorer_show_ignored,
                ),
            }
        } else {
            (None, false, false)
        };
        self.settings_shared_busy = true;
        self.settings_shared_last_refresh = Instant::now();
        self.settings_shared_request = self.settings_shared_request.wrapping_add(1);
        let request = self.settings_shared_request;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    if migration_needed {
                        // Existing Classic values win. A GPUI-only user's
                        // previous Explorer choices are imported once.
                        bridge.desktop_settings_migrate_explorer(old_hidden, old_ignored)?;
                        bridge.desktop_settings_migrate_profile(old_native_profile.as_deref())?;
                    }
                    bridge.desktop_settings_snapshot()
                })
                .await;
            view.update(cx, |view, cx| {
                if view.settings_shared_request != request {
                    return;
                }
                view.settings_shared_busy = false;
                match result {
                    Ok(snapshot) => match view.install_shared_desktop_snapshot(&snapshot, cx) {
                        Ok(()) => {
                            view.settings_native_profile_migrated = true;
                            view.settings_shared_error = None;
                        }
                        Err(error) => view.settings_shared_error = Some(error.to_string()),
                    },
                    Err(error) => {
                        view.settings_shared_error =
                            Some(format!("Shared settings Host unavailable: {error}"));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Reject a write if the authoritative Host is unavailable. Do not fall
    /// back to the former JSON file: that would create split-brain preferences.
    pub(super) fn change_shared_desktop_setting(
        &mut self,
        path: &str,
        value: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        if !self.settings_native_profile_migrated {
            self.model
                .set_status("Load Rust Host settings before editing");
            cx.notify();
            return;
        }
        if self.settings_shared_busy {
            self.model
                .set_status("Settings are updating; retry after the current operation");
            cx.notify();
            return;
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.model
                .set_status("Shared settings need the running Classic Runtime Host");
            cx.notify();
            return;
        };
        let expected = if path == "general.language" {
            serde_json::json!(self.settings_menu_language)
        } else if alera_core::desktop_settings_contract::extended_portable(path) {
            if !alera_core::desktop_settings_contract::valid_extended_portable(path, &value) {
                self.model
                    .set_status("Invalid shared portable settings value");
                cx.notify();
                return;
            }
            match self.settings_extra_values.get(path) {
                Some(value) => value.clone(),
                None => {
                    self.model
                        .set_status("Load shared Rust settings before editing");
                    cx.notify();
                    return;
                }
            }
        } else {
            match self.native_settings.get(path) {
                Some(value) => value,
                None => {
                    self.model
                        .set_status("This setting is not supported by the shared Rust store");
                    cx.notify();
                    return;
                }
            }
        };
        let path = path.to_owned();
        let path_for_status = path.clone();
        self.settings_shared_busy = true;
        self.settings_shared_request = self.settings_shared_request.wrapping_add(1);
        let request = self.settings_shared_request;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    bridge.desktop_settings_compare_and_set(&path, &expected, &value)
                })
                .await;
            view.update(cx, |view, cx| {
                if view.settings_shared_request != request {
                    return;
                }
                view.settings_shared_busy = false;
                match result {
                    Ok(snapshot) => match view.install_shared_desktop_snapshot(&snapshot, cx) {
                        Ok(()) => {
                            view.settings_shared_error = None;
                            if path_for_status == "agents.defaultAgentProfileId" {
                                view.agent_selected_profile =
                                    view.native_settings.default_agent_profile_id.clone();
                            }
                            view.model.set_status(format!(
                                "Shared {path_for_status} saved in Rust Runtime Store"
                            ));
                        }
                        Err(error) => {
                            view.settings_shared_error = Some(error.to_string());
                            view.model
                                .set_status(format!("Shared settings readback failed: {error}"));
                        }
                    },
                    Err(error) => {
                        view.settings_shared_error = Some(error.to_string());
                        view.model
                            .set_status(format!("Shared settings unchanged: {error}"));
                        // Next housekeeping tick retries a fresh read after a
                        // stale CAS so the view cannot keep showing old values.
                        view.settings_shared_last_refresh = Instant::now()
                            .checked_sub(Duration::from_secs(10))
                            .unwrap_or_else(Instant::now);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}
