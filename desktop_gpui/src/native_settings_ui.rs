//! The independent Native settings view uses the same on-disk owner as
//! Host-authorized MCP commands. A visible screen must precede open_settings
//! success, and every local change is a durable compare-and-set.
use super::*;
use alera_gpui_desktop::native_i18n::tr;
use alera_gpui_desktop::settings_menu_catalog::{
    first_matching_group_index_localized, has_connected_native_controls,
    matching_row_count_localized, matching_sections_localized, section as catalog_section,
    SettingsNavGroup, SettingsSection,
};
use alera_gpui_desktop::visual_tokens::ACCENT;
use gpui_kit::component::switch::Switch;

/// Match Classic Settings section icons rather than the catalog's
/// typography-only search placeholders.
fn classic_settings_category_icon(id: &str) -> UiIcon {
    match id {
        "configurationSync" => UiIcon::Refresh,
        "account" => UiIcon::User,
        "application" => UiIcon::Sliders,
        "agents" | "agentProfiles" => UiIcon::Bot,
        "quotas" => UiIcon::Gauge,
        "aiAssist" => UiIcon::Sparkles,
        "aiDictation" => UiIcon::Mic,
        "textActions" => UiIcon::Text,
        "editor" => UiIcon::Code,
        "terminal" => UiIcon::Terminal,
        "keyboard" => UiIcon::Keyboard,
        "projects" => UiIcon::Folder,
        "mobileDevices" => UiIcon::Smartphone,
        "remoteHosts" => UiIcon::Server,
        "historySync" => UiIcon::History,
        _ => UiIcon::Settings,
    }
}

/// Keep Classic's visible min/max without rewriting older Host values
/// that may lie outside the current UI range.
fn classic_numeric_neighbors(value: f64, step: f64, min: f64, max: f64) -> (f64, f64) {
    let previous = if value > max {
        (value - step).max(max)
    } else if value <= min {
        value
    } else {
        (value - step).max(min)
    };
    let next = if value < min {
        min
    } else if value >= max {
        value
    } else {
        (value + step).min(max)
    };
    (
        (previous * 100.0).round() / 100.0,
        (next * 100.0).round() / 100.0,
    )
}

#[cfg(test)]
mod settings_number_tests {
    use super::classic_numeric_neighbors;

    #[test]
    fn numeric_controls_respect_classic_bounds_and_never_invert_legacy_values() {
        assert_eq!(
            classic_numeric_neighbors(13.0, 1.0, 8.0, 32.0),
            (12.0, 14.0)
        );
        assert_eq!(
            classic_numeric_neighbors(32.0, 1.0, 8.0, 32.0),
            (31.0, 32.0)
        );
        assert_eq!(
            classic_numeric_neighbors(40.0, 1.0, 8.0, 32.0),
            (39.0, 40.0)
        );
        assert_eq!(classic_numeric_neighbors(0.0, 1.0, 8.0, 32.0), (0.0, 8.0));
        assert_eq!(classic_numeric_neighbors(8.0, 1.0, 1.0, 8.0), (7.0, 8.0));
        let (previous, next) = classic_numeric_neighbors(0.7, 0.05, 0.0, 1.0);
        assert!((previous - 0.65).abs() < 0.0001);
        assert!((next - 0.75).abs() < 0.0001);
    }
}

impl Workbench {
    /// Native's Alacritty history ring is separate from the Host's byte
    /// retention. Save the actual local cap under CAS, then apply it live to
    /// existing primary/secondary terminal buffers. No Host MCP permission is
    /// widened and no session or PTY is restarted.
    pub(super) fn change_native_scrollback_lines(&mut self, next: u64, cx: &mut Context<Self>) {
        if self.native_scrollback_busy {
            return;
        }
        if !alera_gpui_desktop::native_settings::valid_terminal_scrollback_lines(next) {
            self.native_scrollback_status = "Choose 100–200,000 lines".into();
            cx.notify();
            return;
        }
        let expected = self.native_settings.terminal_scrollback_lines;
        if next == expected {
            return;
        }
        let Some(store) = self.native_settings_store.clone() else {
            self.native_scrollback_status = "Native settings storage unavailable".into();
            cx.notify();
            return;
        };
        self.native_scrollback_busy = true;
        self.native_scrollback_status = "Saving Native scrollback history limit…".into();
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    store.compare_and_set_terminal_scrollback_lines(expected, next)
                })
                .await;
            view.update(cx, |view, cx| {
                view.native_scrollback_busy = false;
                match result {
                    Ok(settings) => {
                        view.install_native_settings(settings, cx);
                        view.native_scrollback_status =
                            "Native terminal scrollback saved and applied to open tabs".into();
                    }
                    Err(error) => {
                        view.native_scrollback_status =
                            format!("Scrollback save refused: {error}; reopen Settings to retry");
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

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
            // Dialog.child() wraps all children in a Kit-owned scrollbar.
            // Use Dialog.content() instead: Settings owns two independently
            // scrolling regions, so the shell must not intercept wheel input.
            let for_content = owner.clone();
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
                .content(move |container, window, cx| {
                    let viewport = window.viewport_size();
                    let body = for_content
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
                    container.child(body)
                })
        });
    }

    /// S10 changes the Project Settings target, not the active Workspace.
    /// A pending draft must be explicitly Saved or Cancelled before switching.
    /// Incrementing the load request rejects late responses for the old target.
    pub(super) fn select_host_project_settings_project(
        &mut self,
        project_id: &str,
        cx: &mut Context<Self>,
    ) {
        if self.project_settings_editing || self.project_settings_save_busy {
            self.project_settings_save_status =
                "Save or Cancel the current Project Settings draft before selecting another project"
                    .into();
            cx.notify();
            return;
        }
        let registered = self.runtime_catalog.as_ref().is_some_and(|catalog| {
            catalog
                .projects
                .iter()
                .any(|project| project.id == project_id)
        });
        if !registered {
            self.project_settings_error =
                Some("Selected project is no longer registered; refresh Projects".into());
            cx.notify();
            return;
        }
        if self.project_settings_selected_project_id.as_deref() == Some(project_id)
            && (self.project_settings_loading
                || self.project_settings_project_id.as_deref() == Some(project_id))
        {
            return;
        }
        self.project_settings_selected_project_id = Some(project_id.to_owned());
        self.project_settings_request = self.project_settings_request.wrapping_add(1);
        self.project_settings_mutation_request =
            self.project_settings_mutation_request.wrapping_add(1);
        self.project_settings_loading = false;
        self.project_settings_project_id = None;
        self.project_settings_config = None;
        self.project_settings_rows.clear();
        self.project_settings_source = None;
        self.project_settings_repo_error = false;
        self.project_settings_use_repo_confirmed = false;
        self.project_settings_error = None;
        self.project_settings_save_status.clear();
        self.project_settings_draft_provider = None;
        self.project_settings_copy_inputs.clear();
        self.project_settings_setup_inputs.clear();
        self.project_settings_prompt_input
            .update(cx, |input, cx| input.clear(cx));
        self.refresh_host_project_settings(cx);
    }

    /// Read the selected project's actual Classic Runtime Host config.
    /// Never open a shadow JSON editor or expose setup shell commands in a
    /// generic Settings search preview.
    pub(super) fn refresh_host_project_settings(&mut self, cx: &mut Context<Self>) {
        if self.project_settings_loading
            || self.project_settings_editing
            || self.project_settings_save_busy
        {
            return;
        }
        let Some(project_id) = self.runtime_catalog.as_ref().and_then(|catalog| {
            alera_gpui_desktop::project_settings_preview::selected_project_for_settings(
                catalog,
                self.model.root(),
                self.project_settings_selected_project_id.as_deref(),
            )
            .map(str::to_owned)
        }) else {
            self.project_settings_project_id = None;
            self.project_settings_config = None;
            self.project_settings_rows.clear();
            self.project_settings_error = Some(
                "No registered projects are available in the Host catalog. Refresh Projects."
                    .to_owned(),
            );
            cx.notify();
            return;
        };
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.project_settings_error = Some("Runtime Host CLI unavailable".to_owned());
            cx.notify();
            return;
        };
        self.project_settings_loading = true;
        self.project_settings_error = None;
        self.project_settings_config = None;
        self.project_settings_save_status.clear();
        self.project_settings_source = None;
        self.project_settings_repo_error = false;
        self.project_settings_use_repo_confirmed = false;

        self.project_settings_request = self.project_settings_request.wrapping_add(1);
        let request = self.project_settings_request;
        let generation = self.workspace_generation;
        cx.spawn(async move |view, cx| {
            let project = project_id.clone();
            let result = cx
                .background_spawn(async move { bridge.read_project_settings_document(&project) })
                .await;
            view.update(cx, |view, cx| {
                if view.project_settings_request != request {
                    return;
                }
                if view.workspace_generation != generation {
                    view.project_settings_loading = false;
                    view.project_settings_project_id = None;
                    view.project_settings_source = None;
                    view.project_settings_repo_error = false;
                    view.project_settings_use_repo_confirmed = false;
                    view.project_settings_rows.clear();
                    view.project_settings_error = Some(
                        "Workspace changed; refresh Project Settings".to_owned(),
                    );
                    cx.notify();
                    return;
                }
                view.project_settings_loading = false;
                match result {
                    Ok(document) => {
                        let value = document.config;
                        match alera_gpui_desktop::project_settings_preview::project_settings_summary(&value) {
                            Ok(rows) => {
                                view.project_settings_rows = rows;
                                view.project_settings_config = Some(value);
                                view.project_settings_project_id = Some(project_id);
                                view.project_settings_source = Some(document.origin);
                                view.project_settings_repo_error = document.repo_config_error;
                                view.project_settings_error = None;
                            }
                            Err(error) => {
                                view.project_settings_rows.clear();
                                view.project_settings_config = None;
                                view.project_settings_project_id = None;
                                view.project_settings_source = None;
                                view.project_settings_repo_error = false;
                                view.project_settings_error = Some(error.to_owned());
                            }
                        }
                    }
                    Err(error) => {
                        view.project_settings_rows.clear();
                        view.project_settings_config = None;
                        view.project_settings_project_id = None;
                        view.project_settings_source = None;
                        view.project_settings_repo_error = false;
                        view.project_settings_error = Some(format!(
                            "Host Project Settings unavailable: {error}"
                        ));
                    }
                }
                cx.notify();
            }).ok();
        }).detach();
        cx.notify();
    }

    fn new_host_project_copy_rule_input(
        rule: &alera_gpui_desktop::project_settings_preview::ProjectCopyRuleDraft,
        cx: &mut Context<Self>,
    ) -> Result<ProjectCopyRowInputs, &'static str> {
        let from = cx.new(|cx| QuickOpenInput::with_placeholder("Relative source, e.g. .env", cx));
        let to = cx.new(|cx| QuickOpenInput::with_placeholder("Optional relative destination", cx));
        if !from.update(cx, |input, cx| input.set_query(&rule.from, cx))
            || !to.update(cx, |input, cx| input.set_query(&rule.to, cx))
        {
            return Err("Worktree copy path is too long to edit in Native");
        }
        Ok(ProjectCopyRowInputs {
            from,
            to,
            overwrite: rule.overwrite,
        })
    }

    fn new_host_project_setup_input(
        command: &str,
        cx: &mut Context<Self>,
    ) -> Result<Entity<QuickOpenInput>, &'static str> {
        let input = cx.new(|cx| QuickOpenInput::profile_multiline(cx));
        input.update(cx, |editor, cx| {
            editor.set_placeholder("Command executed in the new worktree…", cx);
        });
        if !input.update(cx, |editor, cx| editor.set_query(command, cx)) {
            return Err("Worktree setup command is too long to edit in Native");
        }
        Ok(input)
    }

    pub(super) fn add_host_project_copy_rule(&mut self, cx: &mut Context<Self>) {
        if !self.project_settings_editing || self.project_settings_save_busy {
            return;
        }
        if self.project_settings_copy_inputs.len()
            >= alera_gpui_desktop::project_settings_preview::MAX_PROJECT_RULES
        {
            self.project_settings_save_status = "Maximum 64 copy rules".into();
            cx.notify();
            return;
        }
        let draft = alera_gpui_desktop::project_settings_preview::ProjectCopyRuleDraft {
            from: String::new(),
            to: String::new(),
            overwrite: false,
        };
        if let Ok(row) = Self::new_host_project_copy_rule_input(&draft, cx) {
            self.project_settings_copy_inputs.push(row);
            self.project_settings_save_status.clear();
            cx.notify();
        }
    }

    pub(super) fn remove_host_project_copy_rule(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.project_settings_editing
            && !self.project_settings_save_busy
            && index < self.project_settings_copy_inputs.len()
        {
            self.project_settings_copy_inputs.remove(index);
            self.project_settings_save_status.clear();
            cx.notify();
        }
    }

    pub(super) fn toggle_host_project_copy_overwrite(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        if self.project_settings_editing && !self.project_settings_save_busy {
            if let Some(row) = self.project_settings_copy_inputs.get_mut(index) {
                row.overwrite = !row.overwrite;
                self.project_settings_save_status.clear();
                cx.notify();
            }
        }
    }

    pub(super) fn add_host_project_setup_command(&mut self, cx: &mut Context<Self>) {
        if !self.project_settings_editing || self.project_settings_save_busy {
            return;
        }
        if self.project_settings_setup_inputs.len()
            >= alera_gpui_desktop::project_settings_preview::MAX_PROJECT_RULES
        {
            self.project_settings_save_status = "Maximum 64 setup commands".into();
            cx.notify();
            return;
        }
        if let Ok(input) = Self::new_host_project_setup_input("", cx) {
            self.project_settings_setup_inputs.push(input);
            self.project_settings_save_status.clear();
            cx.notify();
        }
    }

    pub(super) fn remove_host_project_setup_command(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        if self.project_settings_editing
            && !self.project_settings_save_busy
            && index < self.project_settings_setup_inputs.len()
        {
            self.project_settings_setup_inputs.remove(index);
            self.project_settings_save_status.clear();
            cx.notify();
        }
    }

    /// The search preview never exposes project prompts. An explicit user
    /// Edit action reveals this project's current prompt in a dedicated
    /// multiline input. Unknown Host schema fields forbid a destructive save.
    pub(super) fn begin_host_project_settings_edit(&mut self, cx: &mut Context<Self>) {
        if self.project_settings_loading
            || self.project_settings_save_busy
            || self.project_settings_editing
        {
            return;
        }
        let Some(config) = self.project_settings_config.as_ref() else {
            self.project_settings_save_status = "Refresh Project Settings before editing".into();
            cx.notify();
            return;
        };
        if self.project_settings_project_id.is_none() {
            self.project_settings_save_status = "No registered Host Project selected".into();
            cx.notify();
            return;
        }
        let provider = config
            .get("gitHostingProvider")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let prompt = config
            .pointer("/newWorkspace/promptAppend")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if let Err(error) = alera_gpui_desktop::project_settings_preview::updated_project_config(
            config,
            provider.as_deref(),
            &prompt,
        ) {
            self.project_settings_save_status = error.to_owned();
            cx.notify();
            return;
        }
        let (copy_seed, setup_seed) =
            match alera_gpui_desktop::project_settings_preview::project_worktree_editor_seed(config)
            {
                Ok(seed) => seed,
                Err(error) => {
                    self.project_settings_save_status = error.to_owned();
                    cx.notify();
                    return;
                }
            };
        let mut copy_inputs = Vec::with_capacity(copy_seed.len());
        for rule in &copy_seed {
            match Self::new_host_project_copy_rule_input(rule, cx) {
                Ok(row) => copy_inputs.push(row),
                Err(error) => {
                    self.project_settings_save_status = error.to_owned();
                    cx.notify();
                    return;
                }
            }
        }
        let mut setup_inputs = Vec::with_capacity(setup_seed.len());
        for command in &setup_seed {
            match Self::new_host_project_setup_input(command, cx) {
                Ok(input) => setup_inputs.push(input),
                Err(error) => {
                    self.project_settings_save_status = error.to_owned();
                    cx.notify();
                    return;
                }
            }
        }
        if !self
            .project_settings_prompt_input
            .update(cx, |input, cx| input.set_query(&prompt, cx))
        {
            self.project_settings_save_status =
                "Project prompt is too long to edit in Native (8 KiB limit)".into();
            cx.notify();
            return;
        }
        self.project_settings_copy_inputs = copy_inputs;
        self.project_settings_setup_inputs = setup_inputs;
        self.project_settings_use_repo_confirmed = false;
        self.project_settings_draft_provider = provider;
        self.project_settings_editing = true;
        self.project_settings_save_status.clear();
        cx.notify();
    }

    pub(super) fn cancel_host_project_settings_edit(&mut self, cx: &mut Context<Self>) {
        if self.project_settings_save_busy {
            return;
        }
        self.project_settings_editing = false;
        self.project_settings_draft_provider = None;
        self.project_settings_prompt_input
            .update(cx, |input, cx| input.clear(cx));
        self.project_settings_copy_inputs.clear();
        self.project_settings_setup_inputs.clear();
        self.project_settings_save_status = "Unsaved Project Settings edits discarded".into();
        cx.notify();
    }

    /// Replace the complete Classic Host ProjectConfig only after comparing
    /// with the snapshot this form displayed. The CLI supports no atomic CAS,
    /// so a preflight comparison reduces conflicts but does not guarantee
    /// atomicity across concurrent Classic writers.
    pub(super) fn save_host_project_settings_edit(&mut self, cx: &mut Context<Self>) {
        if !self.project_settings_editing
            || self.project_settings_save_busy
            || self.project_settings_loading
        {
            return;
        }
        if self.project_settings_prompt_input.read(cx).composing()
            || self
                .project_settings_copy_inputs
                .iter()
                .any(|row| row.from.read(cx).composing() || row.to.read(cx).composing())
            || self
                .project_settings_setup_inputs
                .iter()
                .any(|input| input.read(cx).composing())
        {
            self.project_settings_save_status =
                "Finish the current IME composition before saving".into();
            cx.notify();
            return;
        }
        let Some(project_id) = self.project_settings_project_id.clone() else {
            self.project_settings_save_status = "Select and refresh a Host project first".into();
            cx.notify();
            return;
        };
        if self.runtime_catalog.as_ref().and_then(|catalog| {
            alera_gpui_desktop::project_settings_preview::selected_project_for_settings(
                catalog,
                self.model.root(),
                self.project_settings_selected_project_id.as_deref(),
            )
        }) != Some(project_id.as_str())
        {
            self.project_settings_save_status =
                "Workspace project changed; reload before saving".into();
            cx.notify();
            return;
        }
        let Some(original) = self.project_settings_config.as_ref() else {
            self.project_settings_save_status =
                "Project config not loaded; refresh before saving".into();
            cx.notify();
            return;
        };
        let prompt = self
            .project_settings_prompt_input
            .read(cx)
            .query()
            .to_owned();
        let copy_drafts = self
            .project_settings_copy_inputs
            .iter()
            .map(
                |row| alera_gpui_desktop::project_settings_preview::ProjectCopyRuleDraft {
                    from: row.from.read(cx).query().to_owned(),
                    to: row.to.read(cx).query().to_owned(),
                    overwrite: row.overwrite,
                },
            )
            .collect::<Vec<_>>();
        let setup_drafts = self
            .project_settings_setup_inputs
            .iter()
            .map(|input| input.read(cx).query().to_owned())
            .collect::<Vec<_>>();
        let revised =
            match alera_gpui_desktop::project_settings_preview::updated_project_config_with_worktree(
                original,
                self.project_settings_draft_provider.as_deref(),
                &prompt,
                &copy_drafts,
                &setup_drafts,
            ) {
                Ok(config) => config,
                Err(error) => {
                    self.project_settings_save_status = error.into();
                    cx.notify();
                    return;
                }
            };
        if alera_gpui_desktop::project_settings_preview::host_config_matches(original, &revised) {
            self.project_settings_editing = false;
            self.project_settings_prompt_input
                .update(cx, |input, cx| input.clear(cx));
            self.project_settings_copy_inputs.clear();
            self.project_settings_setup_inputs.clear();
            self.project_settings_save_status = "No Project Settings changes to save".into();
            cx.notify();
            return;
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.project_settings_save_status = "Runtime Host CLI unavailable".into();
            cx.notify();
            return;
        };
        let encoded = match serde_json::to_string(&revised) {
            Ok(encoded) => encoded,
            Err(_) => {
                self.project_settings_save_status = "Could not encode ProjectConfig".into();
                cx.notify();
                return;
            }
        };
        let Some(original_source) = self.project_settings_source else {
            self.project_settings_save_status =
                "Reload effective Project Settings before saving".into();
            cx.notify();
            return;
        };
        let original = original.clone();
        let generation = self.workspace_generation;
        self.project_settings_mutation_request =
            self.project_settings_mutation_request.wrapping_add(1);
        let request = self.project_settings_mutation_request;
        self.project_settings_save_busy = true;
        self.project_settings_save_status =
            "Checking Host version and saving Project Settings…".into();
        cx.spawn(async move |view, cx| {
            let project_for_host = project_id.clone();
            let result = cx.background_spawn(async move {
                let latest = bridge.read_project_settings_document(&project_for_host)
                    .map_err(|error| (error, false))?;
                if latest.config != original || latest.origin != original_source {
                    return Err((
                        std::io::Error::new(
                            std::io::ErrorKind::AlreadyExists,
                            "Host ProjectConfig changed since opening this editor",
                        ),
                        false,
                    ));
                }
                bridge.write_project_settings(&project_for_host, &encoded)
                    .map_err(|error| (error, true))?;
                let readback = bridge.read_project_settings_document(&project_for_host)
                    .map_err(|error| (error, true))?;
                if readback.origin != HostProjectConfigOrigin::UiOverride
                    || !alera_gpui_desktop::project_settings_preview::host_config_matches(
                        &revised, &readback.config,
                    ) {
                    return Err((
                        std::io::Error::other("Host project settings readback differs from saved values"),
                        true,
                    ));
                }
                Ok(readback)
            }).await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation
                    || view.project_settings_mutation_request != request
                {
                    return;
                }
                view.project_settings_save_busy = false;
                match result {
                    Ok(readback) => {
                        match alera_gpui_desktop::project_settings_preview::project_settings_summary(&readback.config) {
                            Ok(rows) => {
                                view.project_settings_source = Some(readback.origin);
                                view.project_settings_repo_error = readback.repo_config_error;
                                view.project_settings_use_repo_confirmed = false;
                                view.project_settings_config = Some(readback.config);
                                view.project_settings_project_id = Some(project_id);
                                view.project_settings_rows = rows;
                                view.project_settings_error = None;
                                view.project_settings_editing = false;
                                view.project_settings_draft_provider = None;
                                view.project_settings_prompt_input.update(cx, |input, cx| input.clear(cx));
                                view.project_settings_copy_inputs.clear();
                                view.project_settings_setup_inputs.clear();
                                view.project_settings_save_status =
                                    "Host Project Settings saved and confirmed by readback".into();
                            }
                            Err(error) => {
                                view.project_settings_save_status = format!(
                                    "Host saved, but the response is invalid: {error}. Refresh before another save."
                                );
                                view.project_settings_editing = false;
                                view.project_settings_copy_inputs.clear();
                                view.project_settings_setup_inputs.clear();
                                view.project_settings_config = None;
                            }
                        }
                    }
                    Err((error, outcome_unknown)) => {
                        if outcome_unknown {
                            // A CLI timeout can occur after the Host committed.
                            // Never issue a second overwrite without a fresh read.
                            view.project_settings_editing = false;
                            view.project_settings_config = None;
                            view.project_settings_project_id = None;
                            view.project_settings_rows.clear();
                            view.project_settings_copy_inputs.clear();
                            view.project_settings_setup_inputs.clear();
                            view.project_settings_source = None;
                            view.project_settings_repo_error = false;
                            view.project_settings_use_repo_confirmed = false;
                            view.project_settings_error = Some(
                                "Project save outcome is uncertain; refresh from Host before editing again".into()
                            );
                        }
                        view.project_settings_save_status = format!(
                            "Project Settings save not confirmed: {error}"
                        );
                    }
                }
                cx.notify();
            }).ok();
        }).detach();
        cx.notify();
    }

    /// "Use Repo File" is Classic's explicit action to remove the Host UI
    /// override. Never delete or modify the repository's alera.toml. Without
    /// a valid repo file the effective source becomes defaults instead.
    pub(super) fn use_repo_file_for_project_settings(&mut self, cx: &mut Context<Self>) {
        if self.project_settings_loading
            || self.project_settings_save_busy
            || self.project_settings_editing
            || self.project_settings_source != Some(HostProjectConfigOrigin::UiOverride)
        {
            return;
        }
        if !self.project_settings_use_repo_confirmed {
            self.project_settings_use_repo_confirmed = true;
            self.project_settings_save_status =
                "Confirm removing this project's UI override. alera.toml will take over if valid; otherwise defaults apply. No repository files will be changed.".into();
            cx.notify();
            return;
        }
        self.project_settings_use_repo_confirmed = false;
        let Some(project_id) = self.project_settings_project_id.clone() else {
            self.project_settings_save_status =
                "Reload Project Settings before switching source".into();
            cx.notify();
            return;
        };
        if self.runtime_catalog.as_ref().and_then(|catalog| {
            alera_gpui_desktop::project_settings_preview::selected_project_for_settings(
                catalog,
                self.model.root(),
                self.project_settings_selected_project_id.as_deref(),
            )
        }) != Some(project_id.as_str())
        {
            self.project_settings_save_status = "Workspace project changed; refresh first".into();
            cx.notify();
            return;
        }
        let Some(original) = self.project_settings_config.clone() else {
            self.project_settings_save_status = "ProjectConfig is not loaded; refresh first".into();
            cx.notify();
            return;
        };
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.project_settings_save_status = "Runtime Host CLI unavailable".into();
            cx.notify();
            return;
        };
        let generation = self.workspace_generation;
        self.project_settings_mutation_request =
            self.project_settings_mutation_request.wrapping_add(1);
        let request = self.project_settings_mutation_request;
        self.project_settings_save_busy = true;
        self.project_settings_save_status =
            "Checking Host override and switching to repository configuration…".into();
        cx.spawn(async move |view, cx| {
            let selected_project_id = project_id.clone();
            let result = cx.background_spawn(async move {
                let latest = bridge.read_project_settings_document(&selected_project_id)
                    .map_err(|error| (error, false))?;
                if latest.origin != HostProjectConfigOrigin::UiOverride || latest.config != original {
                    return Err((
                        std::io::Error::new(
                            std::io::ErrorKind::AlreadyExists,
                            "Host ProjectConfig changed; refresh before removing the override",
                        ),
                        false,
                    ));
                }
                bridge.clear_project_settings_override(&selected_project_id)
                    .map_err(|error| (error, true))?;
                let readback = bridge.read_project_settings_document(&selected_project_id)
                    .map_err(|error| (error, true))?;
                if readback.origin == HostProjectConfigOrigin::UiOverride {
                    return Err((
                        std::io::Error::other("Host still reports an active UI override"),
                        true,
                    ));
                }
                Ok(readback)
            }).await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation
                    || view.project_settings_mutation_request != request
                {
                    return;
                }
                view.project_settings_save_busy = false;
                view.project_settings_use_repo_confirmed = false;
                match result {
                    Ok(readback) => {
                        match alera_gpui_desktop::project_settings_preview::project_settings_summary(
                            &readback.config,
                        ) {
                            Ok(rows) => {
                                let source = readback.origin;
                                let repo_error = readback.repo_config_error;
                                view.project_settings_source = Some(source);
                                view.project_settings_repo_error = repo_error;
                                view.project_settings_config = Some(readback.config);
                                view.project_settings_project_id = Some(project_id);
                                view.project_settings_rows = rows;
                                view.project_settings_error = None;
                                view.project_settings_save_status = if repo_error {
                                    "Host UI override removed; repository alera.toml is invalid. Fix the file before using repo setup.".into()
                                } else {
                                    format!("UI override removed; Host now uses {}", source.label())
                                };
                            }
                            Err(error) => {
                                view.project_settings_config = None;
                                view.project_settings_source = None;
                                view.project_settings_error = Some(format!(
                                    "Override removed, but Host returned invalid settings: {error}; refresh"
                                ));
                            }
                        }
                    }
                    Err((error, outcome_unknown)) => {
                        if outcome_unknown {
                            view.project_settings_config = None;
                            view.project_settings_source = None;
                            view.project_settings_repo_error = false;
                            view.project_settings_project_id = None;
                            view.project_settings_rows.clear();
                            view.project_settings_error = Some(
                                "Override removal outcome is uncertain; refresh Host settings before another action".into()
                            );
                        }
                        view.project_settings_save_status =
                            format!("Use Repo File not confirmed: {error}");
                    }
                }
                cx.notify();
            }).ok();
        }).detach();
        cx.notify();
    }

    pub(super) fn cancel_use_repo_file_confirmation(&mut self, cx: &mut Context<Self>) {
        if self.project_settings_save_busy {
            return;
        }
        self.project_settings_use_repo_confirmed = false;
        self.project_settings_save_status =
            "Kept Host UI override; no configuration changed".into();
        cx.notify();
    }

    pub(super) fn show_native_settings_section(
        &mut self,
        section: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), &'static str> {
        if section != "historySync" && catalog_section(section).is_none() {
            return Err("ui_not_ready");
        }
        if section == "terminal" && self.settings_terminal_fonts.is_none() {
            // GPUI's platform text system is the same source used by the
            // actual terminal renderer, so suggestions reflect installed
            // fonts on Windows, macOS and Linux without a fixed allowlist.
            let mut names = cx.text_system().all_font_names();
            names.retain(|name| {
                !name.trim().is_empty() && name.len() <= 128 && !name.chars().any(char::is_control)
            });
            names.sort_by_key(|name| name.to_lowercase());
            names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
            self.settings_terminal_fonts = Some(names);
        }
        // Existing connected Native controls now read the same Rust Runtime
        // Store as Classic. Never overlay them with stale native-settings.json.
        if has_connected_native_controls(section) {
            self.refresh_shared_desktop_settings(cx);
        }
        if self.settings_section.is_none() {
            let locale = self.settings_menu_locale;
            self.settings_menu_search.update(cx, |input, cx| {
                input.clear(cx);
                input.set_placeholder(tr(locale, "Search settings"), cx);
            });
            self.settings_editor_theme_search.update(cx, |input, cx| {
                input.clear(cx);
                input.set_placeholder(tr(locale, "Search syntax themes"), cx);
            });
            self.settings_terminal_theme_search.update(cx, |input, cx| {
                input.clear(cx);
                input.set_placeholder(tr(locale, "Search terminal themes"), cx);
            });
        }
        self.explorer_context_menu = None;
        self.mixed_tab_context = None;
        self.quick_open = None;
        if self.explorer_action.is_some() {
            self.cancel_explorer_action(cx);
        }
        self.settings_section = Some(section.to_owned());
        let query = self.settings_menu_search.read(cx).query().to_owned();
        let scroll_target = catalog_section(section)
            .and_then(|entry| {
                first_matching_group_index_localized(entry, &query, self.settings_menu_locale)
            })
            .map_or(0, |index| index + 1);
        self.settings_catalog_scroll.scroll_to_item(scroll_target);
        match section {
            "agents" => self.refresh_terminals(cx),
            "historySync" => self.refresh_shared_history_sync(cx),
            "agentProfiles" => self.refresh_agent_profiles(cx),
            "projects" => self.refresh_host_project_settings(cx),
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
        match_count: usize,
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
            .gap_2()
            .px_2()
            .py_2()
            .bg(rgb(if active {
                alera_gpui_desktop::visual_tokens::ELEVATED
            } else {
                PANEL
            }))
            .when(!compact, |row| row.w_full())
            .hover(|style| style.bg(rgb(ACTIVE)))
            .child(div().w(px(2.0)).h(px(16.0)).rounded_md().bg(rgb(if active {
                ACCENT
            } else {
                PANEL
            })))
            .child(icon(
                classic_settings_category_icon(id),
                18.0,
                if active { TEXT } else { MUTED },
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .text_color(rgb(if active { TEXT } else { MUTED }))
                    .child(tr(self.settings_menu_locale, label)),
            )
            .children((match_count > 0).then(|| {
                div()
                    .rounded_md()
                    .bg(rgb(TOOLBAR))
                    .px_2()
                    .py_1()
                    .text_xs()
                    .text_color(rgb(TEXT))
                    .child(match_count.to_string())
            }))
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

    /// User-requested Host mutation only: normal Command Profile creation or
    /// revision-guarded name edit. Neither action mirrors private Host config.
    fn save_host_agent_profile(&mut self, rename: bool, cx: &mut Context<Self>) {
        if self.agent_profile_mutation_busy {
            return;
        }
        self.agent_profile_removal_preview = None;
        if self.agent_profile_name_input.read(cx).composing() {
            self.agent_profile_mutation_status = "Finish IME composition before saving".into();
            cx.notify();
            return;
        }
        let input = self.agent_profile_name_input.read(cx);
        let name = match alera_gpui_desktop::agent_profiles::validate_profile_name(input.query()) {
            Ok(value) => value,
            Err(error) => {
                self.agent_profile_mutation_status = error.to_string();
                cx.notify();
                return;
            }
        };
        let rename_target = if rename {
            let Some(profile) = self.agent_profiles.iter().find(|profile| {
                Some(profile.id.as_str()) == self.agent_selected_profile.as_deref()
            }) else {
                self.agent_profile_mutation_status = "Select a Host Profile to rename".into();
                cx.notify();
                return;
            };
            let Some(revision) = profile.revision else {
                self.agent_profile_mutation_status =
                    "Host did not expose a revision; cannot safely rename this Profile".into();
                cx.notify();
                return;
            };
            Some((profile.id.clone(), revision))
        } else {
            None
        };
        if self.runtime_bridge.is_none() {
            self.runtime_bridge = RuntimeCliBridge::discover().ok();
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.agent_profile_mutation_status =
                "Alera CLI missing; cannot save a Host Profile".into();
            cx.notify();
            return;
        };
        let adapter = self.agent_profile_new_adapter.clone();
        let generation = self.workspace_generation;
        self.agent_profile_mutation_busy = true;
        self.agent_profile_mutation_request = self.agent_profile_mutation_request.wrapping_add(1);
        let request = self.agent_profile_mutation_request;
        self.agent_profile_mutation_status = if rename {
            "Renaming the selected Host Profile with revision validation…"
        } else {
            "Creating a standard Command Profile in the running Host…"
        }
        .into();
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    if let Some((id, revision)) = rename_target {
                        bridge.rename_agent_profile(&id, revision, &name)
                    } else {
                        bridge.create_command_agent_profile(&name, &adapter)
                    }
                })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation
                    || view.agent_profile_mutation_request != request
                {
                    return;
                }
                view.agent_profile_mutation_busy = false;
                match result {
                    Ok(saved) => {
                        view.agent_selected_profile = Some(saved.id.clone());
                        view.agent_profile_name_input.update(cx, |input, cx| {
                            input.set_query(&saved.name, cx);
                        });
                        view.agent_profile_mutation_status = if rename {
                            "Profile renamed in Host; catalog refreshing"
                        } else {
                            "Standard Command Profile created in Host; catalog refreshing"
                        }
                        .into();
                        // Reject an older in-flight listing before readback.
                        view.agent_profiles_loading = false;
                        view.agent_profiles_request = view.agent_profiles_request.wrapping_add(1);
                        view.refresh_agent_profiles(cx);
                    }
                    Err(error) => {
                        view.agent_profile_mutation_status = format!(
                            "Host Profile not confirmed: {error}. Refresh Profiles before retrying."
                        );
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Read a non-secret routing projection from the existing Host. The
    /// selection and observed revision guard against stale UI writebacks.
    pub(super) fn load_selected_agent_profile_routing(&mut self, cx: &mut Context<Self>) {
        if self.agent_profile_mutation_busy || self.agent_profiles_loading {
            return;
        }
        let Some(selected) = self
            .agent_profiles
            .iter()
            .find(|profile| Some(profile.id.as_str()) == self.agent_selected_profile.as_deref())
            .cloned()
        else {
            self.agent_profile_mutation_status = "Select a Host Profile to edit Routing".into();
            cx.notify();
            return;
        };
        self.agent_profile_routing_loaded = None;
        self.agent_profile_description_input
            .update(cx, |input, cx| input.clear(cx));
        self.agent_profile_quota_group_input
            .update(cx, |input, cx| input.clear(cx));
        if self.runtime_bridge.is_none() {
            self.runtime_bridge = RuntimeCliBridge::discover().ok();
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.agent_profile_mutation_status = "Alera CLI missing; cannot load Routing".into();
            cx.notify();
            return;
        };
        self.agent_profile_mutation_busy = true;
        self.agent_profile_mutation_request = self.agent_profile_mutation_request.wrapping_add(1);
        let ticket = self.agent_profile_mutation_request;
        let generation = self.workspace_generation;
        self.agent_profile_mutation_status =
            "Loading Description and Quota Group from Host…".into();
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move { bridge.load_agent_profile_routing(&selected) })
                .await;
            let _ = view.update(cx, |view, cx| {
                if view.workspace_generation != generation
                    || view.agent_profile_mutation_request != ticket
                {
                    return;
                }
                view.agent_profile_mutation_busy = false;
                match result {
                    Ok(fields)
                        if view.agent_selected_profile.as_deref() == Some(fields.id.as_str())
                            && view.agent_profiles.iter().any(|profile| {
                                profile.id == fields.id && profile.revision == Some(fields.revision)
                            }) =>
                    {
                        view.agent_profile_description_input
                            .update(cx, |input, cx| {
                                input.set_query(&fields.description, cx);
                            });
                        view.agent_profile_quota_group_input
                            .update(cx, |input, cx| {
                                input.set_query(&fields.quota_group, cx);
                            });
                        view.agent_profile_routing_loaded = Some((fields.id, fields.revision));
                        view.agent_profile_mutation_status =
                            "Host Routing loaded; edit and save these fields only".into();
                    }
                    Ok(_) => {
                        view.agent_profile_mutation_status =
                            "Profile changed during Routing load; refresh and select it again"
                                .into()
                    }
                    Err(error) => {
                        view.agent_profile_mutation_status =
                            format!("Unable to load Host Routing: {error}")
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// Explicit, read-only discovery from the installed Agent CLI. These
    /// bounded, fixed Classic-compatible commands never run at page startup.
    fn discover_selected_agent_profile_choices(
        &mut self,
        kind: ManagedChoiceKind,
        cx: &mut Context<Self>,
    ) {
        if self.agent_profile_discovery_busy
            || self.agent_profile_mutation_busy
            || self.agent_profiles_loading
        {
            return;
        }
        let Some(selected) = self
            .agent_profiles
            .iter()
            .find(|profile| Some(profile.id.as_str()) == self.agent_selected_profile.as_deref())
            .filter(|profile| profile.managed)
            .cloned()
        else {
            self.agent_profile_discovery_status =
                "Select a Managed Profile before discovering options".into();
            cx.notify();
            return;
        };
        if discovery_arguments(&selected.agent_type, kind).is_none() {
            self.agent_profile_discovery_status =
                "This adapter does not expose a registered read-only option list; enter an exact ID"
                    .into();
            cx.notify();
            return;
        }
        let Some(installed) = self
            .installed_agent_clis
            .iter()
            .find(|agent| agent.id == selected.agent_type)
            .cloned()
        else {
            self.agent_profile_discovery_status =
                "Agent CLI not installed; rescan installed Agents or enter an exact ID".into();
            cx.notify();
            return;
        };
        self.agent_profile_discovery_busy = true;
        self.agent_profile_discovery_request = self.agent_profile_discovery_request.wrapping_add(1);
        let ticket = self.agent_profile_discovery_request;
        let generation = self.workspace_generation;
        self.agent_profile_discovery_status = "Discovering installed Agent options…".into();
        cx.spawn(async move |view, cx| {
            let result = cx.background_spawn(async move {
                discover_choices(&installed, kind)
            }).await;
            let _ = view.update(cx, |view, cx| {
                if view.workspace_generation != generation
                    || view.agent_profile_discovery_request != ticket
                    || view.agent_selected_profile.as_deref() != Some(selected.id.as_str())
                    || !view.agent_profiles.iter().any(|row| {
                        row.id == selected.id
                            && row.managed
                            && row.revision == selected.revision
                    })
                {
                    return;
                }
                view.agent_profile_discovery_busy = false;
                match result {
                    Ok(choices) => {
                        let count = choices.len();
                        match kind {
                            ManagedChoiceKind::Models => view.agent_profile_discovered_models = choices,
                            ManagedChoiceKind::Personas => view.agent_profile_discovered_personas = choices,
                        }
                        view.agent_profile_discovery_status = if count == 0 {
                            "No supported options reported; enter an exact ID if known".into()
                        } else {
                            format!("{count} available option(s) discovered; click one to fill the edit field")
                        };
                    }
                    Err(error) => {
                        view.agent_profile_discovery_status =
                            format!("Read-only discovery unavailable: {error}; enter an exact ID");
                    }
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }

    /// The Host owns the entire Managed configuration. Native only exposes
    /// its display-safe model/persona selection after an explicit Host read.
    pub(super) fn load_selected_agent_profile_managed_selection(&mut self, cx: &mut Context<Self>) {
        if self.agent_profile_mutation_busy || self.agent_profiles_loading {
            return;
        }
        let Some(selected) = self
            .agent_profiles
            .iter()
            .find(|row| Some(row.id.as_str()) == self.agent_selected_profile.as_deref())
            .filter(|row| row.managed)
            .cloned()
        else {
            self.agent_profile_mutation_status =
                "Select a Managed Host Profile to load Model/Persona".into();
            cx.notify();
            return;
        };
        self.agent_profile_managed_loaded = None;
        self.agent_profile_model_input
            .update(cx, |input, cx| input.clear(cx));
        self.agent_profile_persona_input
            .update(cx, |input, cx| input.clear(cx));
        if self.runtime_bridge.is_none() {
            self.runtime_bridge = RuntimeCliBridge::discover().ok();
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.agent_profile_mutation_status =
                "Alera CLI missing; cannot load Managed Model/Persona".into();
            cx.notify();
            return;
        };
        self.agent_profile_mutation_busy = true;
        self.agent_profile_mutation_request = self.agent_profile_mutation_request.wrapping_add(1);
        let ticket = self.agent_profile_mutation_request;
        let generation = self.workspace_generation;
        self.agent_profile_mutation_status = "Loading Managed Model/Persona from Host…".into();
        cx.spawn(async move |view, cx| {
            let result = cx.background_spawn(async move {
                bridge.load_agent_profile_managed_selection(&selected)
            }).await;
            let _ = view.update(cx, |view, cx| {
                if view.workspace_generation != generation
                    || view.agent_profile_mutation_request != ticket {
                    return;
                }
                view.agent_profile_mutation_busy = false;
                match result {
                    Ok(fields) if view.agent_selected_profile.as_deref() == Some(fields.id.as_str())
                        && view.agent_profiles.iter().any(|row| {
                            row.id == fields.id
                                && row.managed
                                && row.revision == Some(fields.revision)
                        }) => {
                        let model_ok = view.agent_profile_model_input.update(cx, |input, cx| {
                            input.set_query(&fields.model, cx)
                        });
                        let persona_ok = view.agent_profile_persona_input.update(cx, |input, cx| {
                            input.set_query(&fields.persona, cx)
                        });
                        if model_ok && persona_ok {
                            view.agent_profile_managed_loaded = Some(fields);
                            view.agent_profile_mutation_status =
                                "Managed Model/Persona loaded; save changes or explicitly clear a field"
                                    .into();
                        } else {
                            view.agent_profile_mutation_status =
                                "Managed Host selection exceeds editor limits".into();
                        }
                    }
                    Ok(_) => {
                        view.agent_profile_mutation_status =
                            "Managed Profile changed during load; refresh and reselect".into();
                    }
                    Err(error) => {
                        view.agent_profile_mutation_status =
                            format!("Unable to load Managed Model/Persona: {error}");
                    }
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }

    /// Save only changed Model/Persona fields. Empty input requires explicit
    /// Clear Model / Clear Persona, so clearing never happens accidentally.
    fn save_selected_agent_profile_managed_selection(
        &mut self,
        clear_model: bool,
        clear_persona: bool,
        cx: &mut Context<Self>,
    ) {
        if self.agent_profile_mutation_busy || self.agent_profiles_loading {
            return;
        }
        let Some(selected) = self
            .agent_profiles
            .iter()
            .find(|row| Some(row.id.as_str()) == self.agent_selected_profile.as_deref())
            .filter(|row| row.managed)
            .cloned()
        else {
            self.agent_profile_mutation_status = "Select a Managed Host Profile first".into();
            cx.notify();
            return;
        };
        let Some(original) = self.agent_profile_managed_loaded.clone() else {
            self.agent_profile_mutation_status =
                "Load Model/Persona from Host before editing".into();
            cx.notify();
            return;
        };
        if original.id != selected.id || Some(original.revision) != selected.revision {
            self.agent_profile_mutation_status =
                "Managed Profile revision changed; reload before saving".into();
            cx.notify();
            return;
        }
        if self.agent_profile_model_input.read(cx).composing()
            || self.agent_profile_persona_input.read(cx).composing()
        {
            self.agent_profile_mutation_status =
                "Finish IME composition before saving Managed Profile".into();
            cx.notify();
            return;
        }
        let model_now = self
            .agent_profile_model_input
            .read(cx)
            .query()
            .trim()
            .to_owned();
        let persona_now = self
            .agent_profile_persona_input
            .read(cx)
            .query()
            .trim()
            .to_owned();
        let descriptor = alera_core::agent_descriptor::agent_descriptor(&selected.agent_type);
        let supports_model = descriptor.is_some_and(|item| {
            item.model_override != alera_core::agent_descriptor::AgentModelOverride::Unsupported
        });
        let supports_persona = descriptor.is_some_and(|item| item.supports_persona);
        if (clear_model && !supports_model) || (clear_persona && !supports_persona) {
            self.agent_profile_mutation_status =
                "Selected adapter does not support that Managed option".into();
            cx.notify();
            return;
        }
        if !clear_model
            && !clear_persona
            && ((supports_model && model_now.is_empty() && !original.model.is_empty())
                || (supports_persona && persona_now.is_empty() && !original.persona.is_empty()))
        {
            self.agent_profile_mutation_status =
                "Use Clear Model / Clear Persona to remove an existing selection".into();
            cx.notify();
            return;
        }
        let model: Option<Option<String>> = if clear_model {
            (!original.model.is_empty()).then_some(None)
        } else if clear_persona {
            None
        } else if supports_model && model_now != original.model {
            Some(Some(model_now))
        } else {
            None
        };
        let persona: Option<Option<String>> = if clear_persona {
            (!original.persona.is_empty()).then_some(None)
        } else if clear_model {
            None
        } else if supports_persona && persona_now != original.persona {
            Some(Some(persona_now))
        } else {
            None
        };
        if model.is_none() && persona.is_none() {
            self.agent_profile_mutation_status = "No Managed Model/Persona changes to save".into();
            cx.notify();
            return;
        }
        if self.runtime_bridge.is_none() {
            self.runtime_bridge = RuntimeCliBridge::discover().ok();
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.agent_profile_mutation_status =
                "Alera CLI missing; cannot save Managed Model/Persona".into();
            cx.notify();
            return;
        };
        self.agent_profile_mutation_busy = true;
        self.agent_profile_mutation_request = self.agent_profile_mutation_request.wrapping_add(1);
        let ticket = self.agent_profile_mutation_request;
        let generation = self.workspace_generation;
        self.agent_profile_mutation_status =
            "Saving Managed Model/Persona to Host with revision check…".into();
        cx.spawn(async move |view, cx| {
            let result = cx.background_spawn(async move {
                bridge.update_agent_profile_managed_selection(
                    &selected,
                    model.as_ref().map(|value| value.as_deref()),
                    persona.as_ref().map(|value| value.as_deref()),
                )
            }).await;
            let _ = view.update(cx, |view, cx| {
                if view.workspace_generation != generation
                    || view.agent_profile_mutation_request != ticket {
                    return;
                }
                view.agent_profile_mutation_busy = false;
                view.agent_profile_managed_loaded = None;
                view.agent_profile_model_input.update(cx, |input, cx| input.clear(cx));
                view.agent_profile_persona_input.update(cx, |input, cx| input.clear(cx));
                match result {
                    Ok(_) => {
                        view.agent_profile_mutation_status =
                            "Host Managed Model/Persona saved and independently verified; refreshing catalog"
                                .into();
                        view.agent_profiles_loading = false;
                        view.agent_profiles_request = view.agent_profiles_request.wrapping_add(1);
                        view.refresh_agent_profiles(cx);
                    }
                    Err(error) => {
                        view.agent_profile_mutation_status =
                            format!("Managed Model/Persona save not confirmed: {error}; reload before retrying");
                    }
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }

    /// Explicitly load Command and Custom Prompt for a temporary edit session.
    /// The full Host response is projected before any UI state is updated.
    pub(super) fn load_selected_agent_profile_content(&mut self, cx: &mut Context<Self>) {
        if self.agent_profile_mutation_busy || self.agent_profiles_loading {
            return;
        }
        let Some(selected) = self
            .agent_profiles
            .iter()
            .find(|profile| Some(profile.id.as_str()) == self.agent_selected_profile.as_deref())
            .cloned()
        else {
            self.agent_profile_mutation_status =
                "Select a Host Profile to edit Command/Prompt".into();
            cx.notify();
            return;
        };
        self.agent_profile_content_loaded = None;
        self.agent_profile_command_input
            .update(cx, |input, cx| input.clear(cx));
        self.agent_profile_custom_prompt_input
            .update(cx, |input, cx| input.clear(cx));
        if self.runtime_bridge.is_none() {
            self.runtime_bridge = RuntimeCliBridge::discover().ok();
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.agent_profile_mutation_status =
                "Alera CLI missing; cannot load Command/Prompt".into();
            cx.notify();
            return;
        };
        self.agent_profile_mutation_busy = true;
        self.agent_profile_mutation_request = self.agent_profile_mutation_request.wrapping_add(1);
        let ticket = self.agent_profile_mutation_request;
        let generation = self.workspace_generation;
        self.agent_profile_mutation_status = "Loading Command/Prompt from Host…".into();
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move { bridge.load_agent_profile_content(&selected) })
                .await;
            let _ = view.update(cx, |view, cx| {
                if view.workspace_generation != generation
                    || view.agent_profile_mutation_request != ticket
                {
                    return;
                }
                view.agent_profile_mutation_busy = false;
                match result {
                    Ok(fields)
                        if view.agent_selected_profile.as_deref() == Some(fields.id.as_str())
                            && view.agent_profiles.iter().any(|profile| {
                                profile.id == fields.id
                                    && profile.revision == Some(fields.revision)
                                    && profile.managed == fields.managed
                            }) =>
                    {
                        let command_loaded = view
                            .agent_profile_command_input
                            .update(cx, |input, cx| input.set_query(&fields.command, cx));
                        let prompt_loaded = view
                            .agent_profile_custom_prompt_input
                            .update(cx, |input, cx| input.set_query(&fields.custom_prompt, cx));
                        if command_loaded && prompt_loaded {
                            view.agent_profile_content_loaded = Some(fields);
                            view.agent_profile_mutation_status =
                                "Command/Prompt loaded from Host; edit then Save or Clear Prompt"
                                    .into();
                        } else {
                            view.agent_profile_mutation_status =
                                "Host content exceeds editor limits; nothing loaded".into();
                        }
                    }
                    Ok(_) => {
                        view.agent_profile_mutation_status =
                            "Host Profile changed during content load; refresh and reselect it"
                                .into();
                    }
                    Err(error) => {
                        view.agent_profile_mutation_status =
                            format!("Unable to load Command/Prompt: {error}");
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// Explicit user action only; edits are transient until Host CAS succeeds.
    /// Clearing a prompt requires the separate Clear Prompt action.
    fn save_selected_agent_profile_content(&mut self, clear_only: bool, cx: &mut Context<Self>) {
        if self.agent_profile_mutation_busy || self.agent_profiles_loading {
            return;
        }
        let Some(selected) = self
            .agent_profiles
            .iter()
            .find(|profile| Some(profile.id.as_str()) == self.agent_selected_profile.as_deref())
            .cloned()
        else {
            self.agent_profile_mutation_status = "Select a Host Profile before saving".into();
            cx.notify();
            return;
        };
        let Some(original) = self.agent_profile_content_loaded.as_ref() else {
            self.agent_profile_mutation_status =
                "Load Command/Prompt from Host before editing".into();
            cx.notify();
            return;
        };
        if original.id != selected.id
            || Some(original.revision) != selected.revision
            || original.managed != selected.managed
        {
            self.agent_profile_mutation_status =
                "Host Profile changed; reload before saving".into();
            cx.notify();
            return;
        }
        if self.agent_profile_command_input.read(cx).composing()
            || self.agent_profile_custom_prompt_input.read(cx).composing()
        {
            self.agent_profile_mutation_status = "Finish IME composition before saving".into();
            cx.notify();
            return;
        }
        let command_now = self
            .agent_profile_command_input
            .read(cx)
            .query()
            .trim()
            .to_owned();
        let prompt_now = self
            .agent_profile_custom_prompt_input
            .read(cx)
            .query()
            .to_owned();
        if !selected.managed && !clear_only && command_now.is_empty() {
            self.agent_profile_mutation_status = "Command must not be empty".into();
            cx.notify();
            return;
        }
        if !clear_only && prompt_now.trim().is_empty() && prompt_now != original.custom_prompt {
            self.agent_profile_mutation_status =
                "Use the explicit Clear Prompt button to remove instructions".into();
            cx.notify();
            return;
        }
        let command = if clear_only || selected.managed || command_now == original.command {
            None
        } else {
            Some(command_now)
        };
        let prompt = if clear_only || prompt_now == original.custom_prompt {
            None
        } else {
            Some(prompt_now)
        };
        let clear_prompt = clear_only && !original.custom_prompt.is_empty();
        if command.is_none() && prompt.is_none() && !clear_prompt {
            self.agent_profile_mutation_status = "No Command/Prompt changes to save".into();
            cx.notify();
            return;
        }
        if self.runtime_bridge.is_none() {
            self.runtime_bridge = RuntimeCliBridge::discover().ok();
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.agent_profile_mutation_status =
                "Alera CLI missing; cannot save Command/Prompt".into();
            cx.notify();
            return;
        };
        self.agent_profile_mutation_busy = true;
        self.agent_profile_mutation_request = self.agent_profile_mutation_request.wrapping_add(1);
        let ticket = self.agent_profile_mutation_request;
        let generation = self.workspace_generation;
        self.agent_profile_mutation_status =
            "Saving Command/Prompt to Host with revision check…".into();
        cx.spawn(async move |view, cx| {
            let result = cx.background_spawn(async move {
                bridge.update_agent_profile_command_prompt(
                    &selected, command.as_deref(), prompt.as_deref(), clear_prompt
                )
            }).await;
            let _ = view.update(cx, |view, cx| {
                if view.workspace_generation != generation
                    || view.agent_profile_mutation_request != ticket {
                    return;
                }
                view.agent_profile_mutation_busy = false;
                view.agent_profile_content_loaded = None;
                view.agent_profile_command_input.update(cx, |input, cx| input.clear(cx));
                view.agent_profile_custom_prompt_input.update(cx, |input, cx| input.clear(cx));
                match result {
                    Ok(_) => {
                        view.agent_profile_mutation_status =
                            "Command/Prompt saved in Host; catalog refreshing for independent readback".into();
                        view.agent_profiles_loading = false;
                        view.agent_profiles_request = view.agent_profiles_request.wrapping_add(1);
                        view.refresh_agent_profiles(cx);
                    }
                    Err(error) => {
                        view.agent_profile_mutation_status =
                            format!("Host Command/Prompt not saved: {error}. Reload before retrying.");
                    }
                }
                cx.notify();
            });
        }).detach();
        cx.notify();
    }

    /// Only Description/Quota Group are writable here; existing Host Command,
    /// Prompt and Managed configurations are never part of Native UI state.
    fn save_selected_agent_profile_routing(&mut self, cx: &mut Context<Self>) {
        if self.agent_profile_mutation_busy || self.agent_profiles_loading {
            return;
        }
        let Some(selected) = self
            .agent_profiles
            .iter()
            .find(|profile| Some(profile.id.as_str()) == self.agent_selected_profile.as_deref())
            .cloned()
        else {
            self.agent_profile_mutation_status =
                "Select a Host Profile before saving Routing".into();
            cx.notify();
            return;
        };
        if self
            .agent_profile_routing_loaded
            .as_ref()
            .is_none_or(|(id, revision)| id != &selected.id || Some(*revision) != selected.revision)
        {
            // Never submit a Profile that the user has not explicitly loaded.
            self.agent_profile_mutation_status =
                "Load the selected Profile Routing before saving".into();
            cx.notify();
            return;
        }
        if self.agent_profile_description_input.read(cx).composing()
            || self.agent_profile_quota_group_input.read(cx).composing()
        {
            self.agent_profile_mutation_status =
                "Finish IME composition before saving Routing".into();
            cx.notify();
            return;
        }
        let (description, quota_group) =
            match alera_gpui_desktop::agent_profiles::validate_profile_routing_fields(
                self.agent_profile_description_input.read(cx).query(),
                self.agent_profile_quota_group_input.read(cx).query(),
            ) {
                Ok(values) => values,
                Err(error) => {
                    self.agent_profile_mutation_status = error.to_string();
                    cx.notify();
                    return;
                }
            };
        if self.runtime_bridge.is_none() {
            self.runtime_bridge = RuntimeCliBridge::discover().ok();
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.agent_profile_mutation_status = "Alera CLI missing; cannot save Routing".into();
            cx.notify();
            return;
        };
        self.agent_profile_mutation_busy = true;
        self.agent_profile_mutation_request = self.agent_profile_mutation_request.wrapping_add(1);
        let ticket = self.agent_profile_mutation_request;
        let generation = self.workspace_generation;
        self.agent_profile_mutation_status = "Saving Description and Quota Group to Host…".into();
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    bridge.save_agent_profile_routing(&selected, &description, &quota_group)
                })
                .await;
            let _ = view.update(cx, |view, cx| {
                if view.workspace_generation != generation
                    || view.agent_profile_mutation_request != ticket
                {
                    return;
                }
                view.agent_profile_mutation_busy = false;
                match result {
                    Ok(saved) => {
                        view.agent_profile_routing_loaded =
                            Some((saved.id.clone(), saved.revision.unwrap_or(-1)));
                        view.agent_profile_mutation_status =
                            "Routing saved in Host; catalog refreshing for independent readback"
                                .into();
                        view.agent_profiles_loading = false;
                        view.agent_profiles_request = view.agent_profiles_request.wrapping_add(1);
                        view.refresh_agent_profiles(cx);
                    }
                    Err(error) => {
                        view.agent_profile_mutation_status =
                            format!("Host Routing not saved: {error}. Reload before retrying.")
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// Clone the complete Host-owned source using only its ID/revision here.
    /// The CLI reads commands, prompts and Managed options from the Host.
    fn clone_selected_host_agent_profile(&mut self, cx: &mut Context<Self>) {
        if self.agent_profile_mutation_busy || self.agent_profiles_loading {
            return;
        }
        let Some(source) = self
            .agent_profiles
            .iter()
            .find(|profile| Some(profile.id.as_str()) == self.agent_selected_profile.as_deref())
            .cloned()
        else {
            self.agent_profile_mutation_status = "Select a Host Profile to clone".into();
            cx.notify();
            return;
        };
        let name = match alera_gpui_desktop::agent_profiles::suggest_clone_name(
            &source,
            &self.agent_profiles,
        ) {
            Ok(value) => value,
            Err(error) => {
                self.agent_profile_mutation_status = format!("Cannot clone Profile: {error}");
                cx.notify();
                return;
            }
        };
        if self.runtime_bridge.is_none() {
            self.runtime_bridge = RuntimeCliBridge::discover().ok();
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.agent_profile_mutation_status =
                "Alera CLI missing; cannot clone Host Profile".into();
            cx.notify();
            return;
        };
        self.agent_profile_removal_preview = None;
        self.agent_profile_mutation_busy = true;
        self.agent_profile_mutation_request = self.agent_profile_mutation_request.wrapping_add(1);
        let ticket = self.agent_profile_mutation_request;
        let generation = self.workspace_generation;
        let source_id = source.id.clone();
        let managed = source.managed;
        self.agent_profile_mutation_status = "Cloning complete Host Profile configuration…".into();
        cx.spawn(async move |view, cx| {
            let result = cx.background_spawn(async move {
                bridge.clone_agent_profile(&source, &name)
            }).await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation
                    || view.agent_profile_mutation_request != ticket {
                    return;
                }
                view.agent_profile_mutation_busy = false;
                match result {
                    Ok(copy) => {
                        if view.agent_selected_profile.as_deref() == Some(source_id.as_str()) {
                            view.agent_selected_profile = Some(copy.id.clone());
                            view.agent_profile_name_input.update(cx, |input, cx| {
                                input.set_query(&copy.name, cx);
                            });
                        }
                        view.agent_profile_mutation_status =
                            "Host Profile cloned; complete configuration retained, catalog refreshing".into();
                        view.agent_profiles_loading = false;
                        view.agent_profiles_request = view.agent_profiles_request.wrapping_add(1);
                        view.refresh_agent_profiles(cx);
                    }
                    Err(error) => {
                        view.agent_profile_mutation_status = if managed {
                            format!("Managed Profile clone not confirmed: {error}. Reduced-protection options require explicit confirmation in Classic or CLI.")
                        } else {
                            format!("Profile clone not confirmed: {error}. Refresh Host Profiles before retrying.")
                        };
                    }
                }
                cx.notify();
            }).ok();
        }).detach();
        cx.notify();
    }

    /// Reorder Host Profiles with the complete observed revision snapshot,
    /// never by mutating Native JSON or reusing stale local indices.
    fn move_host_agent_profile(
        &mut self,
        profile_id: String,
        toward_start: bool,
        cx: &mut Context<Self>,
    ) {
        if self.agent_profile_mutation_busy || self.agent_profiles_loading {
            return;
        }
        self.agent_profile_removal_preview = None;
        if !self
            .agent_profiles
            .iter()
            .any(|profile| profile.id == profile_id)
        {
            self.agent_profile_mutation_status =
                "Profile list changed; refresh before reordering".into();
            cx.notify();
            return;
        }
        if self.runtime_bridge.is_none() {
            self.runtime_bridge = RuntimeCliBridge::discover().ok();
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.agent_profile_mutation_status =
                "Alera CLI missing; cannot reorder Host Profiles".into();
            cx.notify();
            return;
        };
        let observed = self.agent_profiles.clone();
        let generation = self.workspace_generation;
        self.agent_profile_mutation_busy = true;
        self.agent_profile_mutation_request = self.agent_profile_mutation_request.wrapping_add(1);
        let request = self.agent_profile_mutation_request;
        self.agent_profile_mutation_status = "Saving Host Agent Profile order…".into();
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    bridge.move_agent_profile(&observed, &profile_id, toward_start)
                })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation
                    || view.agent_profile_mutation_request != request
                {
                    return;
                }
                view.agent_profile_mutation_busy = false;
                match result {
                    Ok(profiles) => {
                        view.agent_profiles = profiles;
                        view.agent_profile_mutation_status =
                            "Host Agent Profile order saved".into();
                        view.agent_profiles_loading = false;
                        view.agent_profiles_request = view.agent_profiles_request.wrapping_add(1);
                        view.refresh_agent_profiles(cx);
                    }
                    Err(error) => {
                        view.agent_profile_mutation_status = format!(
                            "Could not reorder Host Profiles: {error}. Refresh before retrying."
                        );
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Step 1 of deletion: show the Host's revision-pinned impact. No state
    /// is removed or changed by preview; missing references fail closed.
    fn preview_host_agent_profile_removal(&mut self, cx: &mut Context<Self>) {
        if self.agent_profile_mutation_busy || self.agent_profiles_loading {
            return;
        }
        let Some(profile) = self
            .agent_profiles
            .iter()
            .find(|row| Some(row.id.as_str()) == self.agent_selected_profile.as_deref())
            .cloned()
        else {
            self.agent_profile_mutation_status = "Select a Host Profile to review deletion".into();
            cx.notify();
            return;
        };
        let Some(revision) = profile.revision else {
            self.agent_profile_mutation_status =
                "Host revision missing; refresh before removal".into();
            cx.notify();
            return;
        };
        if self.runtime_bridge.is_none() {
            self.runtime_bridge = RuntimeCliBridge::discover().ok();
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.agent_profile_mutation_status =
                "Alera CLI missing; cannot inspect Profile references".into();
            cx.notify();
            return;
        };
        self.agent_profile_removal_preview = None;
        self.agent_profile_mutation_busy = true;
        self.agent_profile_mutation_request = self.agent_profile_mutation_request.wrapping_add(1);
        let request = self.agent_profile_mutation_request;
        let generation = self.workspace_generation;
        self.agent_profile_mutation_status =
            "Checking Host Profile references before deletion…".into();
        cx.spawn(async move |view, cx| {
            let result = cx.background_spawn(async move {
                bridge.preview_agent_profile_removal(&profile.id, revision)
                    .map(|preview| (preview, profile.name))
            }).await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation || view.agent_profile_mutation_request != request {
                    return;
                }
                view.agent_profile_mutation_busy = false;
                match result {
                    Ok((impact, name)) => {
                        if view.agent_selected_profile.as_deref() == Some(impact.profile_id.as_str()) {
                            view.agent_profile_mutation_status = impact.removal_message(&name);
                            view.agent_profile_removal_preview = Some(impact);
                        } else {
                            view.agent_profile_mutation_status = "Selection changed; preview again".into();
                        }
                    }
                    Err(error) => {
                        view.agent_profile_mutation_status =
                            format!("Could not inspect Profile references: {error}. Refresh before retrying.");
                    }
                }
                cx.notify();
            }).ok();
        }).detach();
        cx.notify();
    }

    /// Step 2: a distinct user click. The bridge rechecks Host impact before
    /// an explicit, revision-guarded --confirm and verifies Host list readback.
    fn confirm_host_agent_profile_removal(&mut self, cx: &mut Context<Self>) {
        if self.agent_profile_mutation_busy || self.agent_profiles_loading {
            return;
        }
        let Some(impact) = self.agent_profile_removal_preview.take() else {
            self.agent_profile_mutation_status = "Review deletion impact before confirming".into();
            cx.notify();
            return;
        };
        if impact.has_blocking_references()
            || self.agent_selected_profile.as_deref() != Some(impact.profile_id.as_str())
            || !self.agent_profiles.iter().any(|profile| {
                profile.id == impact.profile_id && profile.revision == Some(impact.revision)
            })
        {
            self.agent_profile_mutation_status =
                "Profile selection, revision or references changed; review again".into();
            cx.notify();
            return;
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.agent_profile_mutation_status =
                "Alera CLI missing; cannot delete Host Profile".into();
            cx.notify();
            return;
        };
        self.agent_profile_mutation_busy = true;
        self.agent_profile_mutation_request = self.agent_profile_mutation_request.wrapping_add(1);
        let request = self.agent_profile_mutation_request;
        let generation = self.workspace_generation;
        self.agent_profile_mutation_status =
            "Deleting selected Host Profile after reference recheck…".into();
        cx.spawn(async move |view, cx| {
            let result = cx.background_spawn(async move { bridge.remove_agent_profile(&impact) }).await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation || view.agent_profile_mutation_request != request {
                    return;
                }
                view.agent_profile_mutation_busy = false;
                match result {
                    Ok(()) => {
                        view.agent_selected_profile = None;
                        view.agent_profile_name_input.update(cx, |input, cx| input.clear(cx));
                        view.agent_profile_mutation_status = "Host confirmed Profile deletion; catalog refreshing".into();
                        view.agent_profiles_loading = false;
                        view.agent_profiles_request = view.agent_profiles_request.wrapping_add(1);
                        view.refresh_agent_profiles(cx);
                        view.refresh_shared_desktop_settings(cx);
                    }
                    Err(error) => {
                        view.agent_profile_mutation_status = format!(
                            "Host Profile deletion not confirmed: {error}. Refresh and preview again."
                        );
                    }
                }
                cx.notify();
            }).ok();
        }).detach();
        cx.notify();
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
        let draft = self.agent_override_input.read(cx).query().to_owned();
        let proposed =
            match alera_gpui_desktop::native_settings::agent_executable_form_value(clear, &draft) {
                Ok(path) => path,
                Err(message) => {
                    self.agent_override_status = message.into();
                    cx.notify();
                    return;
                }
            };
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
                        view.agent_override_status = if clear {
                            "Agent executable override cleared; using Auto PATH"
                        } else {
                            "Agent executable preference saved"
                        }
                        .into();
                        view.model.set_status("Local Agent executable setting updated");
                        // Cancel any older PATH scan started before the setting
                        // was saved; it must not overwrite the new detection.
                        view.installed_agent_clis_loading = false;
                        view.installed_agent_clis_request =
                            view.installed_agent_clis_request.wrapping_add(1);
                        view.refresh_installed_agent_clis(cx);
                    }
                    Err(error) => {
                        view.agent_override_status = match error.kind() {
                            std::io::ErrorKind::InvalidInput if cfg!(windows) =>
                                "Invalid Agent executable: choose an existing absolute .exe, .cmd, .bat, or .com file.".into(),
                            std::io::ErrorKind::InvalidInput =>
                                "Invalid Agent executable: choose an existing executable file with an absolute path.".into(),
                            std::io::ErrorKind::AlreadyExists =>
                                "Agent executable path changed in another window; reopen Settings before saving.".into(),
                            _ => format!("Could not save Agent executable: {error}"),
                        };
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
        let (previous, next) = classic_numeric_neighbors(value, step, min, max);
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
                            if previous == value {
                                return;
                            }
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
                            if next == value {
                                return;
                            }
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
                    32.0,
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
                    8.0,
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
                    let name_for_edit = profile.name.clone();
                    let move_up_id = id.clone();
                    let move_down_id = id.clone();
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
                                    if view.agent_profile_mutation_busy {
                                        view.agent_profile_mutation_status =
                                            "Finish the current Host Profile operation first".into();
                                        cx.notify();
                                        return;
                                    }
                                    view.agent_selected_profile = Some(selection_id.clone());
                                    view.agent_profile_removal_preview = None;
                                    view.agent_profile_content_loaded = None;
                                    view.agent_profile_managed_loaded = None;
                                    view.agent_profile_discovery_request =
                                        view.agent_profile_discovery_request.wrapping_add(1);
                                    view.agent_profile_discovery_busy = false;
                                    view.agent_profile_discovered_models.clear();
                                    view.agent_profile_discovered_personas.clear();
                                    view.agent_profile_discovery_status.clear();
                                    view.agent_profile_model_input.update(cx, |input, cx| input.clear(cx));
                                    view.agent_profile_persona_input.update(cx, |input, cx| input.clear(cx));
                                    view.agent_profile_command_input.update(cx, |input, cx| input.clear(cx));
                                    view.agent_profile_custom_prompt_input.update(cx, |input, cx| input.clear(cx));
                                    view.agent_profile_name_input.update(cx, |input, cx| {
                                        input.set_query(&name_for_edit, cx);
                                    });
                                    view.load_selected_agent_profile_routing(cx);
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
                        .child(
                            div()
                                .id(("native-profile-move-up", index))
                                .cursor_pointer()
                                .text_xs()
                                .text_color(rgb(TEXT))
                                .child("↑")
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    view.move_host_agent_profile(move_up_id.clone(), true, cx);
                                })),
                        )
                        .child(
                            div()
                                .id(("native-profile-move-down", index))
                                .cursor_pointer()
                                .text_xs()
                                .text_color(rgb(TEXT))
                                .child("↓")
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    view.move_host_agent_profile(move_down_id.clone(), false, cx);
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
                .child(div().mt_3().text_sm().child("Manage Host Agent Profiles"))
                .child(div().text_xs().text_color(rgb(MUTED)).child(
                    "Create, rename, clone, or edit Routing, Command, Prompt, and Managed Model/Persona. Reduced-protection settings remain Host/Classic owned.",
                ))
                .child(self.agent_profile_name_input.clone())
                .child(div().text_xs().text_color(rgb(MUTED)).child(
                    "Adapter for new Command Profile:"
                ))
                .child(
                    div().flex().flex_wrap().gap_2().children(
                        alera_core::agent_descriptor::AGENT_DESCRIPTORS
                            .iter()
                            .enumerate()
                            .map(|(index, descriptor)| {
                                let id = descriptor.id.to_owned();
                                let selected = self.agent_profile_new_adapter == descriptor.id;
                                div()
                                    .id(("native-create-profile-adapter", index))
                                    .cursor_pointer()
                                    .p_2()
                                    .rounded_md()
                                    .text_xs()
                                    .bg(rgb(if selected { ACTIVE } else { PANEL }))
                                    .text_color(rgb(if selected { TEXT } else { MUTED }))
                                    .child(descriptor.display_name)
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        view.agent_profile_new_adapter = id.clone();
                                        cx.notify();
                                    }))
                            }),
                    ),
                )
                .child(
                    div().flex().flex_wrap().gap_3()
                        .child(
                            div().id("native-create-agent-profile")
                                .cursor_pointer().text_sm()
                                .text_color(rgb(if self.agent_profile_mutation_busy { MUTED } else { TEXT }))
                                .child("Create command profile")
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.save_host_agent_profile(false, cx);
                                }))
                        )
                        .child(
                            div().id("native-rename-agent-profile")
                                .cursor_pointer().text_sm()
                                .text_color(rgb(if self.agent_profile_mutation_busy { MUTED } else { TEXT }))
                                .child("Rename selected")
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.save_host_agent_profile(true, cx);
                                }))
                        )
                        .child(
                            div().id("native-clone-agent-profile")
                                .cursor_pointer().text_sm()
                                .text_color(rgb(if self.agent_profile_mutation_busy { MUTED } else { TEXT }))
                                .child("Clone selected")
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.clone_selected_host_agent_profile(cx);
                                }))
                        )
                )
                .child(div().mt_3().text_sm().child("Command / Custom Prompt · Host Profile"))
                .child(div().text_xs().text_color(rgb(MUTED)).child(
                    "Explicitly load these Host-owned fields to edit. Command mode uses a single-line command; Custom Prompt preserves multiple lines. Changes are not saved until confirmed.",
                ))
                .child(div().flex().flex_wrap().gap_3()
                    .child(div().id("native-load-profile-content")
                        .cursor_pointer().text_sm()
                        .text_color(rgb(if self.agent_profile_mutation_busy { MUTED } else { TEXT }))
                        .child("Load Command / Prompt")
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.load_selected_agent_profile_content(cx);
                        })))
                    .child(div().id("native-save-profile-content")
                        .cursor_pointer().text_sm()
                        .text_color(rgb(if self.agent_profile_mutation_busy { MUTED } else { TEXT }))
                        .child("Save Command / Prompt")
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.save_selected_agent_profile_content(false, cx);
                        })))
                    .child(div().id("native-clear-profile-prompt")
                        .cursor_pointer().text_sm()
                        .text_color(rgb(if self.agent_profile_mutation_busy { MUTED } else { TEXT }))
                        .child("Clear Prompt")
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.save_selected_agent_profile_content(true, cx);
                        })))
                )
                .when(
                    self.agent_profile_content_loaded.as_ref().is_some_and(|fields| {
                        selected_profile.as_deref() == Some(fields.id.as_str())
                            && self.agent_profiles.iter().any(|profile| {
                                profile.id == fields.id
                                    && profile.revision == Some(fields.revision)
                                    && profile.managed == fields.managed
                            })
                    }),
                    |panel| {
                        panel
                            .when(
                                self.agent_profile_content_loaded.as_ref().is_some_and(|fields| !fields.managed),
                                |panel| panel
                                    .child(div().text_xs().text_color(rgb(MUTED)).child("Command (single line, up to 4 KiB)"))
                                    .child(self.agent_profile_command_input.clone()),
                            )
                            .child(div().text_xs().text_color(rgb(MUTED)).child("Custom Prompt (multiline, up to 8 KiB)"))
                            .child(self.agent_profile_custom_prompt_input.clone())
                    }
                )
                .when(
                    self.agent_profiles.iter().any(|row| {
                        row.managed
                            && selected_profile.as_deref() == Some(row.id.as_str())
                    }),
                    |panel| panel
                        .child(div().mt_3().text_sm().child("Managed Model / Persona · Host Profile"))
                        .child(div().text_xs().text_color(rgb(MUTED)).child(
                            "Uses Classic's registered read-only option discovery when supported; exact Model IDs and Persona names can always be entered manually. Only these two fields change; Sandbox and Permissions remain Host-owned.",
                        ))
                        .child(div().flex().flex_wrap().gap_3()
                            .child(div().id("native-load-profile-managed-selection")
                                .cursor_pointer().text_sm()
                                .text_color(rgb(if self.agent_profile_mutation_busy { MUTED } else { TEXT }))
                                .child("Load Model / Persona")
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.load_selected_agent_profile_managed_selection(cx);
                                })))
                            .child(div().id("native-save-profile-managed-selection")
                                .cursor_pointer().text_sm()
                                .text_color(rgb(if self.agent_profile_mutation_busy { MUTED } else { TEXT }))
                                .child("Save Model / Persona")
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.save_selected_agent_profile_managed_selection(false, false, cx);
                                })))
                            .child(div().id("native-clear-profile-model")
                                .cursor_pointer().text_sm()
                                .text_color(rgb(if self.agent_profile_mutation_busy { MUTED } else { TEXT }))
                                .child("Clear Model")
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.save_selected_agent_profile_managed_selection(true, false, cx);
                                })))
                            .child(div().id("native-clear-profile-persona")
                                .cursor_pointer().text_sm()
                                .text_color(rgb(if self.agent_profile_mutation_busy { MUTED } else { TEXT }))
                                .child("Clear Persona")
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.save_selected_agent_profile_managed_selection(false, true, cx);
                                })))
                        )
                        .when(
                            self.agent_profiles.iter().find(|row| {
                                selected_profile.as_deref() == Some(row.id.as_str())
                            }).is_some_and(|row| discovery_arguments(
                                &row.agent_type, ManagedChoiceKind::Models,
                            ).is_some()),
                            |panel| panel.child(
                                div().id("native-discover-managed-models")
                                    .cursor_pointer().text_sm()
                                    .text_color(rgb(if self.agent_profile_discovery_busy { MUTED } else { TEXT }))
                                    .child("Refresh Model choices (read-only)")
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        view.discover_selected_agent_profile_choices(
                                            ManagedChoiceKind::Models, cx,
                                        );
                                    }))
                            ),
                        )
                        .when(
                            self.agent_profiles.iter().find(|row| {
                                selected_profile.as_deref() == Some(row.id.as_str())
                            }).is_some_and(|row| discovery_arguments(
                                &row.agent_type, ManagedChoiceKind::Personas,
                            ).is_some()),
                            |panel| panel.child(
                                div().id("native-discover-managed-personas")
                                    .cursor_pointer().text_sm()
                                    .text_color(rgb(if self.agent_profile_discovery_busy { MUTED } else { TEXT }))
                                    .child("Refresh Persona choices (read-only)")
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        view.discover_selected_agent_profile_choices(
                                            ManagedChoiceKind::Personas, cx,
                                        );
                                    }))
                            ),
                        )
                        .when(!self.agent_profile_discovery_status.is_empty(), |panel| panel
                            .child(div().text_xs().text_color(rgb(MUTED))
                                .child(self.agent_profile_discovery_status.clone()))
                        )
                        .when(
                            self.agent_profile_managed_loaded.as_ref().is_some_and(|fields| {
                                selected_profile.as_deref() == Some(fields.id.as_str())
                                    && self.agent_profiles.iter().any(|row| {
                                        row.id == fields.id
                                            && row.managed
                                            && row.revision == Some(fields.revision)
                                    })
                            }),
                            |panel| {
                                let descriptor = self.agent_profiles.iter()
                                    .find(|row| selected_profile.as_deref() == Some(row.id.as_str()))
                                    .and_then(|row| alera_core::agent_descriptor::agent_descriptor(&row.agent_type));
                                let supports_model = descriptor.is_some_and(|item| {
                                    item.model_override != alera_core::agent_descriptor::AgentModelOverride::Unsupported
                                });
                                let supports_persona = descriptor.is_some_and(|item| item.supports_persona);
                                panel
                                    .when(supports_model, |panel| panel
                                        .child(div().text_xs().text_color(rgb(MUTED)).child("Exact Model ID (leave unchanged for current setting)"))
                                        .child(self.agent_profile_model_input.clone())
                                        .child(div().flex().flex_wrap().gap_2().children(
                                            self.agent_profile_discovered_models.iter().cloned().enumerate()
                                                .map(|(index, model)| {
                                                    div().id(("native-managed-model-option", index))
                                                        .cursor_pointer().p_1().text_xs()
                                                        .bg(rgb(PANEL)).text_color(rgb(TEXT))
                                                        .child(model.clone())
                                                        .on_click(cx.listener(move |view, _, _, cx| {
                                                            if view.agent_profile_mutation_busy
                                                                || view.agent_profile_managed_loaded.is_none() {
                                                                return;
                                                            }
                                                            view.agent_profile_model_input.update(cx, |input, cx| {
                                                                input.set_query(&model, cx);
                                                            });
                                                            cx.notify();
                                                        }))
                                                }),
                                        ))
                                    )
                                    .when(!supports_model, |panel| panel
                                        .child(div().text_xs().text_color(rgb(MUTED)).child("This adapter has no Model override"))
                                    )
                                    .when(supports_persona, |panel| panel
                                        .child(div().text_xs().text_color(rgb(MUTED)).child("Exact Persona / Agent name"))
                                        .child(self.agent_profile_persona_input.clone())
                                        .child(div().flex().flex_wrap().gap_2().children(
                                            self.agent_profile_discovered_personas.iter().cloned().enumerate()
                                                .map(|(index, persona)| {
                                                    div().id(("native-managed-persona-option", index))
                                                        .cursor_pointer().p_1().text_xs()
                                                        .bg(rgb(PANEL)).text_color(rgb(TEXT))
                                                        .child(persona.clone())
                                                        .on_click(cx.listener(move |view, _, _, cx| {
                                                            if view.agent_profile_mutation_busy
                                                                || view.agent_profile_managed_loaded.is_none() {
                                                                return;
                                                            }
                                                            view.agent_profile_persona_input.update(cx, |input, cx| {
                                                                input.set_query(&persona, cx);
                                                            });
                                                            cx.notify();
                                                        }))
                                                }),
                                        ))
                                    )
                                    .when(!supports_persona, |panel| panel
                                        .child(div().text_xs().text_color(rgb(MUTED)).child("This adapter has no Persona selection"))
                                    )
                            },
                        )
                )
                .child(div().mt_3().text_sm().child("Routing · selected Host Profile"))
                .child(div().text_xs().text_color(rgb(MUTED)).child(
                    "Edit only Description and Quota Group. Existing Command, Prompt and Managed options remain Host-owned.",
                ))
                .child(div().flex().flex_wrap().gap_3()
                    .child(div().id("native-load-profile-routing")
                        .cursor_pointer().text_sm()
                        .text_color(rgb(if self.agent_profile_mutation_busy { MUTED } else { TEXT }))
                        .child("Load routing")
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.load_selected_agent_profile_routing(cx);
                        })))
                    .child(div().id("native-save-profile-routing")
                        .cursor_pointer().text_sm()
                        .text_color(rgb(if self.agent_profile_mutation_busy { MUTED } else { TEXT }))
                        .child("Save routing")
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.save_selected_agent_profile_routing(cx);
                        })))
                )
                .when(
                    self.agent_profile_routing_loaded.as_ref().is_some_and(|(id, revision)| {
                        selected_profile.as_deref() == Some(id.as_str())
                        && self.agent_profiles.iter().any(|profile| {
                            profile.id == *id && profile.revision == Some(*revision)
                        })
                    }),
                    |panel| panel
                        .child(div().text_xs().text_color(rgb(MUTED)).child("Description"))
                        .child(self.agent_profile_description_input.clone())
                        .child(div().text_xs().text_color(rgb(MUTED)).child("Quota Group"))
                        .child(self.agent_profile_quota_group_input.clone())
                )
                .child(
                    div().mt_2().flex().gap_3()
                        .child(
                            div().id("native-preview-delete-agent-profile")
                                .cursor_pointer().text_sm()
                                .text_color(rgb(if self.agent_profile_mutation_busy { MUTED } else { TEXT }))
                                .child("Review deletion impact")
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.preview_host_agent_profile_removal(cx);
                                }))
                        )
                        .children(
                            self.agent_profile_removal_preview.as_ref()
                                .filter(|impact| !impact.has_blocking_references()
                                    && self.agent_selected_profile.as_deref() == Some(impact.profile_id.as_str())
                                    && self.agent_profiles.iter().any(|profile| {
                                        profile.id == impact.profile_id
                                            && profile.revision == Some(impact.revision)
                                    }))
                                .map(|_| {
                                    div().id("native-confirm-delete-agent-profile")
                                        .cursor_pointer().text_sm()
                                        .text_color(rgb(TEXT))
                                        .child("Confirm delete selected")
                                        .on_click(cx.listener(|view, _, _, cx| {
                                            view.confirm_host_agent_profile_removal(cx);
                                        }))
                                })
                        )
                )
                .child(div().text_xs().text_color(rgb(MUTED)).child(
                    if self.agent_profile_mutation_busy {
                        "Checking or saving Host Agent Profile…".to_owned()
                    } else {
                        self.agent_profile_mutation_status.clone()
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
                    .h(px(175.0))
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
                    .h(px(44.0))
                    .px_4()
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(tr(locale, "Settings")),
            )
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
                            .filter_map(|(group_index, group)| {
                                let items = visible
                                    .iter()
                                    .copied()
                                    .filter(|item| item.nav_group == group)
                                    .collect::<Vec<_>>();
                                if items.is_empty() {
                                    return None;
                                }
                                Some(
                                    div()
                                        .id(("native-settings-nav-group", group_index))
                                        .flex()
                                        .gap_1()
                                        .when(compact, |panel| panel.flex_row().items_center())
                                        .when(!compact, |panel| panel.flex_col())
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
                                                matching_row_count_localized(item, &query, locale),
                                                section == item.id,
                                                compact,
                                                cx,
                                            )
                                        })),
                                )
                            }),
                    ),
            );
        let heading = selected
            .map(|selected| tr(locale, selected.title))
            .unwrap_or_else(|| tr(locale, "Settings"));
        let preview = selected
            .map(|selected| self.render_settings_catalog_preview(selected, locale, cx))
            .unwrap_or_else(|| {
                vec![div()
                    .text_sm()
                    .text_color(rgb(MUTED))
                    .child(tr(locale, "No matching settings."))
                    .into_any_element()]
            });
        // Give the catalog its own bounded viewport. Otherwise GPUI Kit's
        // outer dialog scrolls the entire sidebar/header and the group's
        // ScrollHandle never receives a scrollable viewport.
        // The Kit title and margins use the remaining 72 px. Its content
        // builder does not add the extra scrollbar used by Dialog.child().
        let modal_body_height = (metrics.height - 72.0).max(0.0);
        let rows_height = (modal_body_height - if compact { 315.0 } else { 225.0 })
            .max(64.0)
            .min(modal_body_height);
        let content = div()
            .id("native-settings-content")
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
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
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .children(selected.map(|entry| {
                                        icon(classic_settings_category_icon(entry.id), 18.0, ACCENT)
                                    }))
                                    .child(
                                        div()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_lg()
                                            .child(heading),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .when(self.settings_shared_error.is_some(), |actions| {
                                        actions.child(
                                            native_components::action_button(
                                                "native-settings-host-retry-header",
                                                tr(locale, "Refresh"),
                                                false,
                                                false,
                                            )
                                            .on_click(cx.listener(|view, _, _, cx| {
                                                view.refresh_shared_desktop_settings(cx);
                                            })),
                                        )
                                    })
                                    .child(
                                        icon_button(
                                            "native-settings-close",
                                            UiIcon::Close,
                                            "Close settings",
                                            false,
                                        )
                                        .on_click(cx.listener(|view, _, window, cx| {
                                            view.dismiss_native_settings(window, cx);
                                        })),
                                    ),
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
                                        .cursor_pointer()
                                        .rounded_md()
                                        .border_1()
                                        .border_color(rgb(BORDER))
                                        .bg(rgb(PANEL))
                                        .px_2()
                                        .py_1()
                                        .text_xs()
                                        .text_color(rgb(MUTED))
                                        .hover(|style| style.bg(rgb(ACTIVE)))
                                        .child(tr(locale, group.title))
                                        .on_click(cx.listener(move |view, _, _, cx| {
                                            view.settings_catalog_scroll.scroll_to_item(index + 1);
                                            cx.notify();
                                        }))
                                }),
                            )
                        },
                    )),
            )
            .child(
                div()
                    .id("native-settings-rows")
                    .h(px(rows_height))
                    .max_h(px(rows_height))
                    .overflow_y_scroll()
                    .track_scroll(&self.settings_catalog_scroll)
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .children(preview)
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
                            tr(locale, "Refresh"),
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
            // The Kit shell owns the remaining height (header and margins).
            // Only the bounded rows viewport may scroll within these bounds.
            .h(px(modal_body_height))
            .min_h_0()
            .overflow_hidden()
            .key_context("AleraNativeModal")
            .track_focus(&self.settings_modal_focus)
            .when(compact, |body| body.flex_col())
            .when(!compact, |body| body.flex_row())
            .child(navigation)
            .child(content);
        Some(body.into_any_element())
    }
}
