//! Classic Settings catalog presentation on the GPUI Kit workbench.
//! This layer is deliberately inert: real Native controls remain in
//! native_settings_ui.rs, and catalog previews never touch preferences/Host.
use super::*;
use alera_gpui_desktop::native_i18n::{tr, UiLocale};
use alera_gpui_desktop::settings_menu_catalog::{
    shared_setting_row_path, PreviewRow, SettingsSection,
};

/// Fail-closed Host state for catalog rows backed by the shared Rust
/// snapshot. Until the first snapshot installs, a row may only report that
/// the Host is still loading or unreachable — never imply a saved value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CatalogHostState {
    /// First snapshot fetch is in flight or has not reported yet.
    Loading,
    /// The last Host contact failed; controls stay disabled until retry.
    Unavailable,
    /// Snapshot installed; real controls may render.
    Ready,
}

fn catalog_host_state(migrated: bool, has_error: bool) -> CatalogHostState {
    if migrated {
        CatalogHostState::Ready
    } else if has_error {
        CatalogHostState::Unavailable
    } else {
        CatalogHostState::Loading
    }
}

/// An unsupported preview must never impersonate an interactive setting.
fn render_unavailable_row(
    row: &PreviewRow,
    row_index: usize,
    locale: UiLocale,
) -> gpui::AnyElement {
    div()
        .id(("native-setting-preview-row", row_index))
        .px_4()
        .py_3()
        .flex()
        .items_center()
        .gap_4()
        .border_b_1()
        .border_color(rgb(BORDER))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(TEXT))
                        .child(tr(locale, row.title)),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(tr(locale, row.detail)),
                ),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(MUTED))
                .child(tr(locale, "Not available in GPUI yet")),
        )
        .into_any_element()
}

impl Workbench {
    /// Pending label for Host-backed rows before the shared snapshot
    /// installs: loading vs unreachable Host, in the same row chrome as
    /// unsupported catalog previews.
    fn render_host_state_row(&self, row: &PreviewRow) -> gpui::AnyElement {
        let locale = self.settings_menu_locale;
        let status = match catalog_host_state(
            self.settings_native_profile_migrated,
            self.settings_shared_error.is_some(),
        ) {
            CatalogHostState::Unavailable => tr(locale, "Rust Host settings unavailable"),
            _ => tr(locale, "Loading Rust Host settings"),
        };
        div()
            .p_3()
            .bg(rgb(PANEL))
            .flex()
            .items_center()
            .gap_4()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(TEXT))
                            .child(tr(locale, row.title)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child(tr(locale, row.detail)),
                    ),
            )
            .child(div().text_xs().text_color(rgb(MUTED)).child(status))
            .into_any_element()
    }

    fn render_project_management_setting(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        div()
            .p_3()
            .bg(rgb(PANEL))
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .child(tr(self.settings_menu_locale, "Projects")),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child("Open the existing native project and worktree manager."),
                    ),
            )
            .child(
                native_components::action_button(
                    "native-settings-open-projects",
                    "Manage Projects",
                    false,
                    false,
                )
                .on_click(cx.listener(|view, _, window, cx| {
                    view.show_project_management = true;
                    view.dismiss_native_settings(window, cx);
                })),
            )
            .into_any_element()
    }

    fn render_settings_excluded_directories(
        &self,
        items: &[serde_json::Value],
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        const PATH: &str = "editor.quickOpenExcludedDirectories";
        let names = items
            .iter()
            .filter_map(|value| value.as_str().map(str::to_owned))
            .collect::<Vec<_>>();
        let count = names.len();
        div()
            .rounded_lg()
            .bg(rgb(PANEL))
            .p_3()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_sm().child(tr(self.settings_menu_locale, "Excluded Directories")))
            .child(div().text_xs().text_color(rgb(MUTED)).child(
                "Ctrl+P excludes matching directory names regardless of Git ignored visibility. Explorer search is unchanged.",
            ))
            .child(
                div().flex().flex_wrap().gap_2()
                    .children(names.iter().cloned().enumerate().map(|(index, name)| {
                        let next = names.iter()
                            .filter(|item| item.as_str() != name)
                            .cloned().collect::<Vec<_>>();
                        div()
                            .id(("quick-open-exclusion-remove", index))
                            .cursor_pointer()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(BORDER))
                            .bg(rgb(TOOLBAR))
                            .px_2()
                            .py_1()
                            .text_xs()
                            .child(format!("{name}  ×"))
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.change_shared_desktop_setting(
                                    PATH, serde_json::json!(next), cx,
                                );
                            }))
                    })),
            )
            .child(
                div().flex().items_center().gap_2()
                    .child(div().flex_1().min_w_0().rounded_md().border_1()
                        .border_color(rgb(BORDER)).bg(rgb(TOOLBAR)).p_2()
                        .child(self.settings_exclusion_input.clone()))
                    .child(native_components::action_button(
                        "quick-open-exclusion-add", "Add", false, false
                    ).on_click(cx.listener(|view, _, _, cx| {
                        let name = view.settings_exclusion_input.read(cx).query().trim().to_owned();
                        if !alera_core::desktop_settings_contract::valid_excluded_directory_name(&name) {
                            view.model.set_status("Use one directory name without slashes, spaces at the ends or control characters");
                            cx.notify();
                            return;
                        }
                        let Some(existing) = view.settings_extra_values.get(PATH)
                            .and_then(serde_json::Value::as_array) else {
                            view.model.set_status("Rust Host Quick Open exclusion list unavailable");
                            cx.notify();
                            return;
                        };
                        let mut next = existing.iter()
                            .filter_map(|value| value.as_str().map(str::to_owned))
                            .collect::<Vec<_>>();
                        if next.iter().any(|entry| entry.eq_ignore_ascii_case(&name)) {
                            view.model.set_status("Directory is already excluded");
                            cx.notify();
                            return;
                        }
                        next.push(name);
                        view.change_shared_desktop_setting(PATH, serde_json::json!(next), cx);
                    })))
                    .child(native_components::action_button(
                        "quick-open-exclusion-reset", "Restore Defaults", false, false,
                    ).on_click(cx.listener(|view, _, _, cx| {
                        view.change_shared_desktop_setting(
                            PATH,
                            alera_core::desktop_settings_contract::quick_open_default_excluded_directories_json(),
                            cx,
                        );
                    }))),
            )
            .child(div().text_xs().text_color(rgb(MUTED))
                .child(format!("{count} excluded directory names · .git, .hg and .svn remain protected")))
            .into_any_element()
    }

    fn render_settings_readonly_row(
        &self,
        section: &'static str,
        row: &PreviewRow,
    ) -> Option<gpui::AnyElement> {
        if !self.settings_native_profile_migrated {
            return match (section, row.title) {
                (
                    "application",
                    "Confirm Project Removal"
                    | "Confirm Workspace Removal"
                    | "Auto-Archive After Inactivity"
                    | "App Language",
                ) => Some(self.render_host_state_row(row)),
                _ => None,
            };
        }
        let source = match (section, row.title) {
            ("application", "Confirm Project Removal") => {
                self.native_settings.get("general.confirmProjectRemoval")
            }
            ("application", "Confirm Workspace Removal") => {
                self.native_settings.get("general.confirmWorkspaceRemoval")
            }
            ("application", "Auto-Archive After Inactivity") => self
                .native_settings
                .get("general.autoArchiveWorkspacesAfterDays"),
            ("application", "App Language") => Some(serde_json::json!(self.settings_menu_language)),
            _ => None,
        }?;
        let status = match source {
            serde_json::Value::Bool(enabled) => {
                if enabled {
                    "Enabled".to_owned()
                } else {
                    "Disabled".to_owned()
                }
            }
            serde_json::Value::Number(days) if row.title == "Auto-Archive After Inactivity" => {
                format!("{days} days (0 disables)")
            }
            serde_json::Value::String(language) => language,
            value => value.to_string(),
        };
        Some(
            div()
                .p_3()
                .bg(rgb(PANEL))
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_sm()
                        .child(tr(self.settings_menu_locale, row.title)),
                )
                .child(div().text_xs().text_color(rgb(MUTED)).child(format!(
                    "Rust Host: {status} · {}",
                    if row.title == "App Language" {
                        "change via language selector"
                    } else {
                        "edit in Classic"
                    }
                )))
                .into_any_element(),
        )
    }

    fn render_shared_selection_row(
        &self,
        label: &'static str,
        path: &'static str,
        selected: &str,
        row_index: usize,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let options: &'static [(&'static str, &'static str)] = match path {
            "terminal.cursorShape" => &[
                ("block", "Block"),
                ("bar", "Bar"),
                ("underline", "Underline"),
            ],
            "terminal.fontFamily" => &[
                ("JetBrains Mono", "JetBrains Mono"),
                ("Cascadia Code", "Cascadia Code"),
                ("Consolas", "Consolas"),
                ("Menlo", "Menlo"),
            ],
            "terminal.themeName" => &[
                ("Alera Dark", "Alera Dark"),
                ("Ghostty Default Style Dark", "Ghostty Dark"),
                ("Builtin Tango Light", "Tango Light"),
                ("Dracula", "Dracula"),
                ("Catppuccin Mocha", "Catppuccin"),
                ("Ayu Dark", "Ayu Dark"),
            ],
            "editor.themeName" => &[
                ("Alera", "Alera"),
                ("GitHub Dark", "GitHub Dark"),
                ("GitHub Dark Dimmed", "GitHub Dimmed"),
                ("Tokyo Night Dark", "Tokyo Night"),
                ("VS2015", "VS2015"),
                ("Dracula", "Dracula"),
                ("Monokai", "Monokai"),
                ("Nord", "Nord"),
                ("Night Owl", "Night Owl"),
                ("Atom One Dark", "Atom One Dark"),
            ],
            _ => &[],
        };
        div()
            .rounded_lg()
            .bg(rgb(PANEL))
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_sm().child(tr(self.settings_menu_locale, label)))
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(format!("Rust Host: {selected}")),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .children(options.iter().copied().enumerate().map(
                        |(index, (value, caption))| {
                            let active = selected == value;
                            div()
                                .id(("native-shared-choice", row_index * 32 + index))
                                .cursor_pointer()
                                .rounded_md()
                                .border_1()
                                .border_color(rgb(BORDER))
                                .bg(rgb(if active { ACTIVE } else { TOOLBAR }))
                                .px_2()
                                .py_1()
                                .text_xs()
                                .text_color(rgb(if active { TEXT } else { MUTED }))
                                .child(caption)
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    view.change_shared_desktop_setting(
                                        path,
                                        serde_json::json!(value),
                                        cx,
                                    );
                                }))
                        },
                    )),
            )
            .into_any_element()
    }

    fn render_connected_settings_row(
        &self,
        section: &'static str,
        row: &PreviewRow,
        row_index: usize,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        if section == "projects" && row.title == "Projects" {
            return Some(self.render_project_management_setting(cx));
        }
        let path = shared_setting_row_path(section, row.title)?;
        if !self.settings_native_profile_migrated {
            return Some(self.render_host_state_row(row));
        }
        let value = self
            .native_settings
            .get(path)
            .or_else(|| self.settings_extra_values.get(path).cloned())?;
        if path == "editor.quickOpenExcludedDirectories" {
            return value
                .as_array()
                .map(|items| self.render_settings_excluded_directories(items, cx));
        }
        if let Some(checked) = value.as_bool() {
            return Some(self.native_settings_bool_row(row.title, path, checked, cx));
        }
        let numeric = match path {
            "general.autoGitFetchIntervalMinutes" => Some((1.0, 1.0, 60.0)),
            "terminal.fontSize" => Some((1.0, 8.0, 48.0)),
            "terminal.lineHeight" => Some((0.1, 0.8, 2.4)),
            "terminal.backgroundOpacity" => Some((0.05, 0.0, 1.0)),
            "terminal.tuiScrollSensitivity" => Some((1.0, 1.0, 10.0)),
            "editor.tabSize" => Some((1.0, 1.0, 16.0)),
            "editor.autosaveDelaySeconds" => Some((1.0, 1.0, 60.0)),
            _ => None,
        };
        if let Some((step, min, max)) = numeric {
            return value.as_f64().map(|number| {
                self.native_settings_number_row(row.title, path, number, step, min, max, cx)
            });
        }
        value.as_str().map(|selected| {
            self.render_shared_selection_row(row.title, path, selected, row_index, cx)
        })
    }

    /// All Classic Settings categories remain visible. Rows backed by the
    /// reviewed Rust Core contract get real controls; all others are explicit
    /// unavailable labels rather than fake toggles or buttons.
    pub(super) fn render_settings_catalog_preview(
        &self,
        selected: &'static SettingsSection,
        locale: UiLocale,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        div()
            .id("native-settings-catalog-groups")
            .flex()
            .flex_col()
            .gap_3()
            .children(
                selected
                    .groups
                    .iter()
                    .enumerate()
                    .map(|(group_idx, group)| {
                        div()
                            .id(("native-settings-catalog-group", group_idx))
                            .rounded_lg()
                            .border_1()
                            .border_color(rgb(BORDER))
                            .bg(rgb(PANEL))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .px_4()
                                    .py_3()
                                    .text_sm()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(tr(locale, group.title)),
                            )
                            .children(group.rows.iter().enumerate().map(|(row_idx, row)| {
                                self.render_connected_settings_row(
                                    selected.id,
                                    row,
                                    group_idx * 100 + row_idx,
                                    cx,
                                )
                                .or_else(|| self.render_settings_readonly_row(selected.id, row))
                                .unwrap_or_else(|| {
                                    render_unavailable_row(row, group_idx * 100 + row_idx, locale)
                                })
                            }))
                    }),
            )
            .into_any_element()
    }

    /// Exactly the five Classic AppLanguage values. Selection is persisted
    /// through Rust Host CAS and updates Classic on its next change event.
    pub(super) fn render_settings_language_preview(
        &self,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let selected = self.settings_menu_language.as_str();
        div()
            .id("native-settings-language-preview")
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .py_2()
            .border_b_1()
            .border_color(rgb(BORDER))
            .children(
                [
                    ("system", "System"),
                    ("english", "EN"),
                    ("traditionalChinese", "繁中"),
                    ("simplifiedChinese", "简中"),
                    ("japanese", "日本語"),
                ]
                .into_iter()
                .enumerate()
                .map(|(index, (preference, label))| {
                    div()
                        .id(("native-settings-locale-option", index))
                        .cursor_pointer()
                        .rounded_md()
                        .px_2()
                        .py_1()
                        .text_xs()
                        .text_color(rgb(if selected == preference { TEXT } else { MUTED }))
                        .bg(rgb(if selected == preference {
                            ACTIVE
                        } else {
                            PANEL
                        }))
                        .hover(|style| style.bg(rgb(ACTIVE)))
                        .child(label)
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.change_shared_desktop_setting(
                                "general.language",
                                serde_json::json!(preference),
                                cx,
                            );
                        }))
                }),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{catalog_host_state, CatalogHostState};

    #[test]
    fn installed_snapshot_stays_ready_even_after_a_later_refresh_error() {
        assert_eq!(catalog_host_state(true, false), CatalogHostState::Ready);
        assert_eq!(catalog_host_state(true, true), CatalogHostState::Ready);
    }

    #[test]
    fn missing_snapshot_reports_loading_until_the_host_reports_failure() {
        assert_eq!(catalog_host_state(false, false), CatalogHostState::Loading);
        assert_eq!(
            catalog_host_state(false, true),
            CatalogHostState::Unavailable
        );
    }
}
