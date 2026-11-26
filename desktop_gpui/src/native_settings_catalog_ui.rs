//! Classic Settings catalog presentation on the GPUI Kit workbench.
//! Only reviewed, Host-backed rows are interactive. Most rows remain
//! read-only; operational Agent Hook toggles use the versioned Native CLI
//! bridge, never direct RuntimeStore access or speculative local writes.
use super::*;
use alera_gpui_desktop::native_i18n::{tr, UiLocale};
use alera_gpui_desktop::settings_menu_catalog::{
    matching_font_suggestions, matching_theme_choices, settings_operational_action,
    settings_support_counts, shared_setting_row_path, PreviewRow, SettingsOperationalAction,
    SettingsSection, SettingsSupportCounts, APP_LANGUAGE_CHOICES, EDITOR_SYNTAX_THEME_CHOICES,
    TERMINAL_THEME_CHOICES,
};
use gpui_kit::component::switch::Switch;

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

/// The preview must use the same Classic colors and weight/italic flags as
/// the actual NativeEditor styled-text renderer.
fn editor_preview_highlight(
    palette: &alera_gpui_desktop::editor_syntax_theme::EditorSyntaxPalette,
    categories: &[&str],
) -> gpui::HighlightStyle {
    let color = palette
        .first_color(categories)
        .unwrap_or(palette.foreground);
    let modifier = palette.first_modifier(categories).unwrap_or_default();
    gpui::HighlightStyle {
        color: Some(rgb(color).into()),
        font_weight: (modifier & 1 != 0).then_some(gpui::FontWeight::BOLD),
        font_style: (modifier & 2 != 0).then_some(gpui::FontStyle::Italic),
        ..Default::default()
    }
}

fn catalog_host_state(migrated: bool, has_error: bool, refreshing: bool) -> CatalogHostState {
    if refreshing {
        CatalogHostState::Loading
    } else if has_error {
        CatalogHostState::Unavailable
    } else if migrated {
        CatalogHostState::Ready
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

/// All counts remain numeric; labels reflect the persisted Host language.
fn settings_support_summary(locale: UiLocale, counts: SettingsSupportCounts) -> String {
    format!(
        "{}: {} · {}: {} · {}: {} · {}: {}",
        tr(locale, "Editable"),
        counts.editable,
        tr(locale, "Actions"),
        counts.actions,
        tr(locale, "Read-only"),
        counts.read_only,
        tr(locale, "Not available"),
        counts.unavailable,
    )
}

impl Workbench {
    fn shared_catalog_state(&self) -> CatalogHostState {
        catalog_host_state(
            self.settings_native_profile_migrated,
            self.settings_shared_error.is_some(),
            self.settings_shared_busy,
        )
    }

    /// Pending label for Host-backed rows before the shared snapshot
    /// installs: loading vs unreachable Host, in the same row chrome as
    /// unsupported catalog previews.
    fn render_host_state_row(&self, row: &PreviewRow) -> gpui::AnyElement {
        let locale = self.settings_menu_locale;
        let status = match self.shared_catalog_state() {
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

    /// Worktree rules are shown only after the user explicitly chooses Edit.
    /// The generic Settings search/summary never shows copied paths or setup
    /// shell commands, which may contain project-local secrets.
    fn render_host_project_worktree_editor(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let copies = self.project_settings_copy_inputs.len();
        let setups = self.project_settings_setup_inputs.len();
        div()
            .id("host-project-worktree-editor")
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(TEXT))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(format!("Worktree Copy Rules ({copies})")),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child("Relative paths only. From is required; empty To uses From. Overwrite controls replacing an existing destination."),
            )
            .children(self.project_settings_copy_inputs.iter().enumerate().map(|(index, rule)| {
                div()
                    .id(("project-copy-rule-editor", index))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(TEXT))
                            .child(format!("Copy rule {}", index + 1)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(110.0))
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(div().text_xs().text_color(rgb(MUTED)).child("From"))
                                    .child(
                                        div()
                                            .p_2()
                                            .border_1()
                                            .rounded_md()
                                            .border_color(rgb(BORDER))
                                            .bg(rgb(TOOLBAR))
                                            .child(rule.from.clone()),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(110.0))
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(div().text_xs().text_color(rgb(MUTED)).child("To (optional)"))
                                    .child(
                                        div()
                                            .p_2()
                                            .border_1()
                                            .rounded_md()
                                            .border_color(rgb(BORDER))
                                            .bg(rgb(TOOLBAR))
                                            .child(rule.to.clone()),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                native_components::action_button(
                                    format!("project-copy-overwrite-{index}"),
                                    if rule.overwrite { "Overwrite: ON" } else { "Overwrite: OFF" },
                                    rule.overwrite,
                                    false,
                                ).on_click(cx.listener(move |view, _, _, cx| {
                                    view.toggle_host_project_copy_overwrite(index, cx);
                                })),
                            )
                            .child(
                                native_components::action_button(
                                    format!("project-copy-remove-{index}"),
                                    "Remove rule",
                                    false,
                                    true,
                                ).on_click(cx.listener(move |view, _, _, cx| {
                                    view.remove_host_project_copy_rule(index, cx);
                                })),
                            ),
                    )
            }))
            .child(
                native_components::action_button(
                    "project-copy-rule-add", "+ Add Copy Rule", false, false,
                ).on_click(cx.listener(|view, _, _, cx| {
                    view.add_host_project_copy_rule(cx);
                })),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(TEXT))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(format!("Worktree Setup Commands ({setups})")),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child("Commands run only during later worktree setup; editing these does not execute anything now. Empty commands are omitted when saving."),
            )
            .children(self.project_settings_setup_inputs.iter().enumerate().map(|(index, command)| {
                div()
                    .id(("project-setup-command-editor", index))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(TEXT))
                            .child(format!("Setup command {}", index + 1)),
                    )
                    .child(
                        div()
                            .min_h(px(55.0))
                            .p_2()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(BORDER))
                            .bg(rgb(TOOLBAR))
                            .child(command.clone()),
                    )
                    .child(
                        native_components::action_button(
                            format!("project-setup-remove-{index}"),
                            "Remove command",
                            false,
                            true,
                        ).on_click(cx.listener(move |view, _, _, cx| {
                            view.remove_host_project_setup_command(index, cx);
                        })),
                    )
            }))
            .child(
                native_components::action_button(
                    "project-setup-command-add", "+ Add Setup Command", false, false,
                ).on_click(cx.listener(|view, _, _, cx| {
                    view.add_host_project_setup_command(cx);
                })),
            )
            .into_any_element()
    }

    /// Only the effective, registered Host ProjectConfig is summarized here.
    /// Worktree setup commands and prompt bodies are counted, not displayed,
    /// because a search preview must not reveal potential secret values.
    fn render_host_project_settings(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let project_id = self.runtime_catalog.as_ref().and_then(|catalog| {
            alera_gpui_desktop::project_settings_preview::selected_project_for_settings(
                catalog,
                self.model.root(),
                self.project_settings_selected_project_id.as_deref(),
            )
        });
        let project = self.runtime_catalog.as_ref().and_then(|catalog| {
            catalog
                .projects
                .iter()
                .find(|project| Some(project.id.as_str()) == project_id)
        });
        let project_name = project
            .map(|project| project.name.clone())
            .unwrap_or_else(|| "No registered Host projects".to_owned());
        let project_choices = self
            .runtime_catalog
            .as_ref()
            .map(|catalog| {
                catalog
                    .projects
                    .iter()
                    .map(|p| (p.id.clone(), p.name.clone(), p.root.display().to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let loaded = project_id.is_some()
            && self.project_settings_project_id.as_deref() == project_id
            && !self.project_settings_loading
            && self.project_settings_error.is_none();
        let rows = if loaded {
            self.project_settings_rows.clone()
        } else {
            Vec::new()
        };
        let editing = self.project_settings_editing;
        let busy = self.project_settings_save_busy;
        let config_source = self.project_settings_source;
        let repo_config_error = self.project_settings_repo_error;
        let confirm_repo_fallback = self.project_settings_use_repo_confirmed;
        let save_status = self.project_settings_save_status.clone();
        let draft_provider = self.project_settings_draft_provider.clone();
        let status = if self.project_settings_loading {
            "Loading Host ProjectConfig…".to_owned()
        } else if let Some(error) = self.project_settings_error.as_deref() {
            error.to_owned()
        } else if loaded {
            "Runtime Host effective configuration · paths, setup commands and prompt text remain hidden until you choose Edit."
                .to_owned()
        } else {
            "Refresh to inspect this registered project's actual configuration.".to_owned()
        };
        div()
            .p_3()
            .bg(rgb(PANEL))
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(TEXT))
                                    .child("Project Settings"),
                            )
                            .child(div().text_xs().text_color(rgb(MUTED)).child(project_name)),
                    )
                    .child(
                        native_components::action_button(
                            "native-settings-host-project-config-refresh",
                            if self.project_settings_loading { "Loading…" } else if editing || busy {
                                "Finish editing first"
                            } else if project.is_none() {
                                "Refresh Projects"
                            } else {
                                "Refresh Config"
                            },
                            false,
                            false,
                        )
                        .on_click(cx.listener(|view, _, _, cx| {
                            if view.project_settings_loading || view.project_settings_save_busy || view.project_settings_editing {
                                view.project_settings_save_status = "Save or Cancel the current draft before refreshing".into();
                                cx.notify();
                                return;
                            }
                            if view.runtime_catalog.as_ref().and_then(|catalog| {
                                alera_gpui_desktop::project_settings_preview::selected_project_for_settings(
                                    catalog,
                                    view.model.root(),
                                    view.project_settings_selected_project_id.as_deref(),
                                )
                            }).is_none() {
                                view.refresh_project_tree(cx);
                            } else {
                                view.refresh_host_project_settings(cx);
                            }
                        })),
                    ),
            )
            .child(div().text_xs().text_color(rgb(MUTED)).child(status))
            .when(!project_choices.is_empty(), |panel| {
                panel.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(rgb(TEXT))
                                .child("Registered Projects"),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child("Choose a project's settings without switching your active Workspace. Save or Cancel an open draft first."),
                        )
                        .child(
                            div()
                                .id("host-project-settings-project-list")
                                .max_h(px(190.0))
                                .overflow_y_scroll()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .children(
                                    project_choices.into_iter().enumerate().map(|(index, (id, name, root))| {
                                        let selected = Some(id.as_str()) == project_id;
                                        div()
                                            .id(("host-project-settings-choice", index))
                                            .cursor_pointer()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .p_2()
                                            .rounded_md()
                                            .border_1()
                                            .border_color(rgb(if selected { BORDER_ACTIVE } else { BORDER }))
                                            .bg(rgb(if selected { ACTIVE } else { TOOLBAR }))
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .text_color(rgb(TEXT))
                                                    .child(if selected { format!("✓ {name}") } else { name }),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(MUTED))
                                                    .child(root),
                                            )
                                            .on_click(cx.listener(move |view, _, _, cx| {
                                                view.select_host_project_settings_project(&id, cx);
                                            }))
                                    })
                                ),
                        ),
                )
            })
            .when(loaded, |panel| {
                panel.child(
                    div().flex().items_center().justify_between().gap_2()
                        .p_2().rounded_md().border_1().border_color(rgb(BORDER))
                        .child(div().text_xs().text_color(rgb(TEXT)).child("Effective config source"))
                        .child(div().text_xs().text_color(rgb(MUTED))
                            .child(config_source.map(HostProjectConfigOrigin::label).unwrap_or("Unknown"))),
                )
            })
            .when(loaded && repo_config_error, |panel| {
                panel.child(div().text_xs().text_color(rgb(MUTED))
                    .child("Repository alera.toml is invalid; Host reported defaults. Fix that file before relying on repository setup."))
            })
            .children(rows.into_iter().enumerate().map(|(i, (name, value))| {
                div()
                    .id(("native-host-project-config-row", i))
                    .flex()
                    .justify_between()
                    .gap_3()
                    .p_2()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(div().text_xs().text_color(rgb(TEXT)).child(name))
                    .child(div().text_xs().text_color(rgb(MUTED)).child(value))
            }))
            .when(loaded
                && config_source == Some(HostProjectConfigOrigin::UiOverride)
                && !editing && !busy, |panel| {
                panel.child(
                    div().flex().flex_col().gap_2()
                        .child(div().text_xs().text_color(rgb(MUTED))
                            .child("Use Repo File removes only the Host UI override. A valid alera.toml takes effect; without one, defaults apply."))
                        .child(
                            div().flex().flex_wrap().gap_2()
                                .child(
                                    native_components::action_button(
                                        "host-project-config-use-repo",
                                        if confirm_repo_fallback {
                                            "Confirm Remove UI Override"
                                        } else {
                                            "Use Repo File"
                                        },
                                        false,
                                        confirm_repo_fallback,
                                    ).on_click(cx.listener(|view, _, _, cx| {
                                        view.use_repo_file_for_project_settings(cx);
                                    })),
                                )
                                .when(confirm_repo_fallback, |row| {
                                    row.child(
                                        native_components::action_button(
                                            "host-project-config-use-repo-cancel",
                                            "Keep UI Override",
                                            false, false,
                                        ).on_click(cx.listener(|view, _, _, cx| {
                                            view.cancel_use_repo_file_confirmation(cx);
                                        })),
                                    )
                                }),
                        ),
                )
            })
            .when(loaded && !editing && !busy, |panel| {
                panel.child(
                    native_components::action_button(
                        "host-project-config-edit",
                        "Edit Project Settings",
                        false,
                        false,
                    ).on_click(cx.listener(|view, _, _, cx| {
                        view.begin_host_project_settings_edit(cx);
                    })),
                )
            })
            .when(loaded && editing && !busy, |panel| {
                let choices: [(&'static str, Option<&'static str>); 4] = [
                    ("Auto-detect", None),
                    ("GitHub", Some("github")),
                    ("GitLab", Some("gitlab")),
                    ("Azure DevOps", Some("azureDevops")),
                ];
                panel.child(
                    div()
                        .id("host-project-config-edit-form")
                        .flex()
                        .flex_col()
                        .gap_3()
                        .p_2()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(TEXT))
                                .child("Git Hosting Provider"),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap_2()
                                .children(choices.into_iter().enumerate().map(|(index, (label, choice))| {
                                    let selected = draft_provider.as_deref() == choice;
                                    div()
                                        .id(("host-project-config-provider", index))
                                        .cursor_pointer()
                                        .px_2()
                                        .py_1()
                                        .rounded_md()
                                        .border_1()
                                        .border_color(rgb(BORDER))
                                        .bg(rgb(if selected { ACTIVE } else { TOOLBAR }))
                                        .text_xs()
                                        .text_color(rgb(if selected { TEXT } else { MUTED }))
                                        .child(label)
                                        .on_click(cx.listener(move |view, _, _, cx| {
                                            if !view.project_settings_save_busy && view.project_settings_editing {
                                                view.project_settings_draft_provider = choice.map(str::to_owned);
                                                cx.notify();
                                            }
                                        }))
                                })),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child(format!(
                                    "Selected: {}",
                                    draft_provider.as_deref().unwrap_or("Auto-detect"),
                                )),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(TEXT))
                                .child("New Workspace · Prompt Append"),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child("Project instructions are revealed only in Edit mode. Worktree Copy Rules and Setup Commands can be edited below; Save updates the Host config only after a version check."),
                        )
                        .child(
                            div()
                                .id("host-project-config-prompt-input")
                                .rounded_md()
                                .min_h(px(110.0))
                                .p_2()
                                .border_1()
                                .border_color(rgb(BORDER))
                                .bg(rgb(TOOLBAR))
                                .child(self.project_settings_prompt_input.clone()),
                        )
                        .child(self.render_host_project_worktree_editor(cx))
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(
                                    native_components::action_button(
                                        "host-project-config-save",
                                        if busy { "Saving…" } else { "Save to Host" },
                                        busy,
                                        false,
                                    )
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        view.save_host_project_settings_edit(cx);
                                    })),
                                )
                                .child(
                                    native_components::action_button(
                                        "host-project-config-cancel",
                                        "Cancel",
                                        busy,
                                        false,
                                    )
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        view.cancel_host_project_settings_edit(cx);
                                    })),
                                ),
                        ),
                )
            })
            .when(busy, |panel| {
                panel.child(div().p_2().text_xs().text_color(rgb(MUTED))
                    .child("Saving to Host; editing is locked until readback completes…"))
            })
            .when(!save_status.is_empty(), |panel| {
                panel.child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(save_status),
                )
            })
            .into_any_element()
    }

    fn render_native_scrollback_setting(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let current = self.native_settings.terminal_scrollback_lines;
        let busy = self.native_scrollback_busy;
        const OPTIONS: [u64; 9] = [
            100, 500, 1_000, 5_000, 10_000, 20_000, 50_000, 100_000, 200_000,
        ];
        let status = if self.native_scrollback_status.is_empty() {
            "Shift+PageUp / Shift+PageDown scrolls the Native VT history. Full-screen TUIs keep their own keys."
        } else {
            &self.native_scrollback_status
        };
        div()
            .rounded_lg()
            .bg(rgb(PANEL))
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(TEXT))
                    .child(format!("Scrollback Lines · {current}")),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child("Native Alacritty history retained per terminal. Classic's setting is independent; the Host byte buffer can impose a shorter restore window."),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .children(OPTIONS.into_iter().enumerate().map(|(index, next)| {
                        div()
                            .id(("native-scrollback-choice", index))
                            .cursor_pointer()
                            .px_3()
                            .py_2()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(BORDER))
                            .bg(rgb(if current == next { ACTIVE } else { TOOLBAR }))
                            .text_xs()
                            .text_color(rgb(if current == next { TEXT } else { MUTED }))
                            .child(format!("{next}"))
                            .on_click(cx.listener(move |view, _, _, cx| {
                                if !view.native_scrollback_busy {
                                    view.change_native_scrollback_lines(next, cx);
                                }
                            }))
                    })),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        native_components::action_button(
                            "native-scrollback-minus-100", "-100", busy || current <= 100, false
                        ).on_click(cx.listener(|view, _, _, cx| {
                            let next = view.native_settings.terminal_scrollback_lines.saturating_sub(100).max(100);
                            view.change_native_scrollback_lines(next, cx);
                        })),
                    )
                    .child(
                        native_components::action_button(
                            "native-scrollback-plus-100", "+100", busy || current >= 200_000, false
                        ).on_click(cx.listener(|view, _, _, cx| {
                            let next = (view.native_settings.terminal_scrollback_lines + 100).min(200_000);
                            view.change_native_scrollback_lines(next, cx);
                        })),
                    )
                    .child(div().text_xs().text_color(rgb(MUTED))
                        .child("Fine adjustment · 100 lines per step")),
            )
            .child(div().text_xs().text_color(rgb(MUTED)).child(if busy {
                "Saving setting and updating open terminal screens…".to_owned()
            } else {
                status.to_owned()
            }))
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
                    .child(div().text_xs().text_color(rgb(MUTED)).child(tr(
                        self.settings_menu_locale,
                        "Open the existing native project and worktree manager.",
                    ))),
            )
            .child(
                native_components::action_button(
                    "native-settings-open-projects",
                    tr(self.settings_menu_locale, "Manage Projects"),
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

    /// The Quotas section already has a real Host-backed provider overview
    /// in the status bar. Do not leave its Settings entry as a fake action.
    fn render_provider_quotas_setting(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        div()
            .p_3()
            .bg(rgb(PANEL))
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_sm().child(tr(self.settings_menu_locale, "Provider Quotas")))
                    .child(div().text_xs().text_color(rgb(MUTED)).child(
                        tr(self.settings_menu_locale, "Open the existing provider/account quota overview and refresh from Runtime Host."),
                    ))
                    .children(self.render_quota_settings_pin_rows(cx)),
            )
            .child(
                native_components::action_button(
                    "native-settings-provider-quotas",
                    tr(self.settings_menu_locale, "Open Quota Overview"),
                    false,
                    false,
                )
                .on_click(cx.listener(|view, _, window, cx| {
                    view.dismiss_native_settings(window, cx);
                    view.quota_popover_account = None;
                    view.quota_usage_open = false;
                    view.quota_popover_open = true;
                    view.refresh_agent_quotas(true, None, cx);
                    cx.notify();
                })),
            )
            .into_any_element()
    }

    /// Search registered GPUI shortcuts by command, context, or physical key.
    /// This is a discovery view; Classic's editable override/reset store is
    /// not implemented in Native and is deliberately not exposed here.
    fn render_keyboard_shortcut_search(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let query = self.settings_shortcut_search.read(cx).query().to_owned();
        let matches = alera_gpui_desktop::native_shortcut_catalog::search_native_shortcuts(&query);
        let count = matches.len();
        div()
            .p_3()
            .bg(rgb(PANEL))
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(TEXT))
                    .child("Search GPUI shortcuts"),
            )
            .child(
                div()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(TOOLBAR))
                    .p_2()
                    .child(self.settings_shortcut_search.clone()),
            )
            .child(div().text_xs().text_color(rgb(MUTED)).child(format!(
                "{count} common registered shortcuts · {}",
                if cfg!(target_os = "macos") {
                    "macOS"
                } else {
                    "Windows / Linux"
                }
            )))
            .child(div().text_xs().text_color(rgb(MUTED)).child(
                "Search is read-only. Native shortcut overrides and reset are not available yet.",
            ))
            .when(count == 0, |list| {
                list.child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child("No shortcuts match"),
                )
            })
            .children(matches.into_iter().enumerate().map(|(i, shortcut)| {
                let binding = if cfg!(target_os = "macos") {
                    shortcut.mac
                } else {
                    shortcut.windows
                };
                div()
                    .id(("native-shortcut-row", i))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .p_2()
                    .rounded_sm()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_sm().text_color(rgb(TEXT)).child(shortcut.action))
                            .child(div().text_xs().text_color(rgb(MUTED)).child(shortcut.scope)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT))
                            .child(binding),
                    )
            }))
            .into_any_element()
    }

    /// The Host Agent Profiles manager already renders below the catalog.
    /// Replace the misleading 'not available' preview with an operational
    /// refresh action; never duplicate profile editing or credential handling.
    fn render_host_agent_profiles_setting(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        div()
            .p_3()
            .bg(rgb(PANEL))
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .child(
                div().flex().flex_col().gap_1()
                    .child(div().text_sm().text_color(rgb(TEXT)).child("Host Agent Profiles"))
                    .child(
                        div().text_xs().text_color(rgb(MUTED))
                            .child("Create, edit, select, and remove Host profiles using the connected manager below."),
                    ),
            )
            .child(
                native_components::action_button(
                    "native-settings-profiles-refresh",
                    if self.agent_profiles_loading { "Loading…" } else { "Refresh Profiles" },
                    false,
                    false,
                )
                .on_click(cx.listener(|view, _, _, cx| {
                    if !view.agent_profiles_loading {
                        view.refresh_agent_profiles(cx);
                    }
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
                tr(self.settings_menu_locale, "Ctrl+P excludes matching directory names regardless of Git ignored visibility. Explorer search is unchanged."),
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
                        "quick-open-exclusion-add", tr(self.settings_menu_locale, "Add"), false, false
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
                        "quick-open-exclusion-reset", tr(self.settings_menu_locale, "Restore Defaults"), false, false,
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
        if self.shared_catalog_state() != CatalogHostState::Ready {
            return match (section, row.title) {
                (
                    "application",
                    "Confirm Project Removal"
                    | "Confirm Workspace Removal"
                    | "Auto-Archive After Inactivity",
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
            _ => None,
        }?;
        let locale = self.settings_menu_locale;
        let status = match source {
            serde_json::Value::Bool(enabled) => {
                tr(locale, if enabled { "Enabled" } else { "Disabled" })
            }
            serde_json::Value::Number(days) if row.title == "Auto-Archive After Inactivity" => {
                format!("{days} {}", tr(locale, "days (0 disables)"))
            }
            value => value.to_string(),
        };
        Some(
            div()
                .p_3()
                .bg(rgb(PANEL))
                .flex()
                .items_center()
                .gap_3()
                .justify_between()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(div().text_sm().child(tr(locale, row.title)))
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(MUTED))
                                .child(tr(locale, row.detail)),
                        ),
                )
                .child(div().text_xs().text_color(rgb(MUTED)).child(format!(
                    "{}: {status} · {}",
                    tr(locale, "Rust Host"),
                    tr(locale, "edit in Classic"),
                )))
                .into_any_element(),
        )
    }
    /// Classic setting rows keep their help text visible beside the working
    /// control, not only in the search catalog.
    fn render_described_shared_control(
        &self,
        row: &PreviewRow,
        control: gpui::AnyElement,
    ) -> gpui::AnyElement {
        div()
            .rounded_lg()
            .bg(rgb(PANEL))
            .flex()
            .flex_col()
            .child(control)
            .child(
                div()
                    .px_3()
                    .pb_3()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(tr(self.settings_menu_locale, row.detail)),
            )
            .into_any_element()
    }

    fn render_shared_selection_row(
        &self,
        row: &PreviewRow,
        path: &'static str,
        selected: &str,
        row_index: usize,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let locale = self.settings_menu_locale;
        // Both theme pickers now expose the complete Classic catalog, while
        // preserving arbitrary Host values that older versions may have saved.
        let theme_choices = match path {
            "terminal.themeName" => Some(TERMINAL_THEME_CHOICES),
            "editor.themeName" => Some(EDITOR_SYNTAX_THEME_CHOICES),
            _ => None,
        };
        if let Some(choices) = theme_choices {
            let theme_search_input = if path == "editor.themeName" {
                self.settings_editor_theme_search.clone()
            } else {
                self.settings_terminal_theme_search.clone()
            };
            let theme_query = theme_search_input.read(cx).query().to_owned();
            let matched_choices = matching_theme_choices(choices, &theme_query);
            let matching_count = matched_choices.len();
            let terminal_palette = (path == "terminal.themeName")
                .then(|| alera_gpui_desktop::terminal_theme::theme_for_name(selected));
            let editor_palette = (path == "editor.themeName")
                .then(|| alera_gpui_desktop::editor_syntax_theme::palette_for_name(selected));
            let owner = cx.entity().downgrade();
            let selected_label = selected.to_owned();
            return div()
                .rounded_lg()
                .bg(rgb(PANEL))
                .p_3()
                .flex()
                .flex_col()
                .gap_2()
                .child(div().text_sm().child(tr(locale, row.title)))
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(tr(locale, row.detail)),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(if path == "editor.themeName" {
                            tr(locale, "Search syntax themes")
                        } else {
                            tr(locale, "Search terminal themes")
                        }),
                )
                .child(
                    div()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(TOOLBAR))
                        .p_2()
                        .child(theme_search_input),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(format!(
                            "{matching_count}/{} · {}",
                            choices.len(),
                            tr(locale, "Theme Preset"),
                        )),
                )
                .when(matching_count == 0, |panel| {
                    panel.child(
                        div()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child(tr(locale, "No themes found.")),
                    )
                })
                .child(
                    native_components::select_trigger(
                        ("native-shared-theme-select", row_index),
                        selected_label,
                        matching_count == 0,
                    )
                    .dropdown_menu(move |mut menu, _, _| {
                        for choice in matched_choices.iter().copied() {
                            let owner = owner.clone();
                            menu = menu.item(PopupMenuItem::new(choice).on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |view, cx| {
                                    if view.shared_catalog_state() != CatalogHostState::Ready {
                                        view.model.set_status(
                                            "Rust Host settings are unavailable; refresh before editing",
                                        );
                                        cx.notify();
                                    } else if view
                                        .settings_extra_values
                                        .get(path)
                                        .and_then(serde_json::Value::as_str)
                                        != Some(choice)
                                    {
                                        view.change_shared_desktop_setting(
                                            path,
                                            serde_json::json!(choice),
                                            cx,
                                        );
                                    }
                                });
                            }));
                        }
                        menu.scrollable(true).max_h(px(320.0))
                    }),
                )
                .children(terminal_palette.map(|palette| {
                    div()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(palette.background))
                        .p_3()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(palette.foreground))
                                .child(format!("{} · {}", tr(locale, "Terminal"), palette.name)),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(palette.ansi[2]))
                                .child("✓ Alera"),
                        )
                        .child(
                            div()
                                .flex()
                                .gap_1()
                                .children(palette.ansi.iter().copied().enumerate().map(
                                    |(index, color)| {
                                        div()
                                            .id(("native-terminal-theme-color", index))
                                            .flex_1()
                                            .h(px(11.0))
                                            .rounded_sm()
                                            .bg(rgb(color))
                                    },
                                )),
                        )
                }))
                .children(editor_palette.map(|palette| {
                    let keyword = palette.first_color(&["keyword"]).unwrap_or(palette.foreground);
                    let function = palette
                        .first_color(&["title.function_", "function", "title"])
                        .unwrap_or(palette.foreground);
                    let string = palette.first_color(&["string"]).unwrap_or(palette.foreground);
                    let comment = palette.first_color(&["comment"]).unwrap_or(palette.foreground);
                    div()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(palette.background))
                        .p_3()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(palette.foreground))
                                .child(format!("{} · {}", tr(locale, "Editor"), palette.name)),
                        )
                        // The same multi-line example as Classic's
                        // Editor theme preview, using identical token names.
                        .child(
                            div()
                                .font_family("JetBrains Mono")
                                .text_sm()
                                .text_color(rgb(palette.foreground))
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    gpui::StyledText::new("class ThemePreview {")
                                        .with_highlights([
                                            (0..5, editor_preview_highlight(palette, &["keyword"])),
                                            (6..18, editor_preview_highlight(palette, &["title.class_", "class-title"])),
                                        ]),
                                )
                                .child(
                                    gpui::StyledText::new("  final name = 'Alera';")
                                        .with_highlights([
                                            (2..7, editor_preview_highlight(palette, &["keyword"])),
                                            (8..12, editor_preview_highlight(palette, &["variable"])),
                                            (15..22, editor_preview_highlight(palette, &["string"])),
                                        ]),
                                )
                                .child(
                                    gpui::StyledText::new("  // Syntax colors")
                                        .with_highlights([
                                            (2..18, editor_preview_highlight(palette, &["comment"])),
                                        ]),
                                )
                                .child("}"),
                        )
                        .child(
                            div()
                                .flex()
                                .gap_1()
                                .children([keyword, function, string, comment].into_iter().enumerate().map(
                                    |(index, color)| {
                                        div()
                                            .id(("native-editor-theme-color", index))
                                            .flex_1()
                                            .h(px(11.0))
                                            .rounded_sm()
                                            .bg(rgb(color))
                                    },
                                )),
                        )
                }))
                .into_any_element();
        }

        // Classic cursor-shape control is segmented; common font presets are
        // offered without inventing a fixed font catalog for this device.
        let options: &'static [(&'static str, &'static str)] = match path {
            "terminal.cursorShape" => &[
                ("block", "Block"),
                ("bar", "Bar"),
                ("underline", "Underline"),
            ],
            "editor.externalEditor" => &[("zed", "Zed"), ("vscode", "Visual Studio Code")],
            "terminal.fontFamily" => &[
                ("JetBrains Mono", "JetBrains Mono"),
                ("Cascadia Code", "Cascadia Code"),
                ("Consolas", "Consolas"),
                ("Menlo", "Menlo"),
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
            .child(div().text_sm().child(tr(locale, row.title)))
            .child(div().text_xs().text_color(rgb(MUTED)).child(tr(locale, row.detail)))
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(format!("Rust Host: {selected}")),
            )
            .children((path == "terminal.fontFamily").then(|| {
                let suggestions = self
                    .settings_terminal_fonts
                    .as_ref()
                    .map(|installed| {
                        matching_font_suggestions(
                            installed,
                            self.settings_font_family_input.read(cx).query(),
                            8,
                        )
                        .into_iter()
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child(tr(locale, "Enter a font family name or choose a suggestion.")),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(rgb(BORDER))
                                    .bg(rgb(TOOLBAR))
                                    .p_2()
                                    .child(self.settings_font_family_input.clone()),
                            )
                            .child(
                                native_components::action_button(
                                    "native-terminal-font-family-apply",
                                    tr(locale, "Save"),
                                    false,
                                    false,
                                )
                                .on_click(cx.listener(|view, _, _, cx| {
                                    let font = view
                                        .settings_font_family_input
                                        .read(cx)
                                        .query()
                                        .trim()
                                        .to_owned();
                                    let candidate = serde_json::json!(font);
                                    if !alera_core::desktop_settings_contract::valid_extended_portable(
                                        "terminal.fontFamily",
                                        &candidate,
                                    ) {
                                        view.model.set_status(
                                            "Enter a font family name (maximum 128 bytes, no control characters)",
                                        );
                                        cx.notify();
                                        return;
                                    }
                                    if view
                                        .settings_extra_values
                                        .get("terminal.fontFamily")
                                        .and_then(serde_json::Value::as_str)
                                        == Some(font.as_str())
                                    {
                                        return;
                                    }
                                    view.change_shared_desktop_setting(
                                        "terminal.fontFamily",
                                        candidate,
                                        cx,
                                    );
                                })),
                            )
                            .child(
                                native_components::action_button(
                                    "native-terminal-font-family-revert",
                                    tr(locale, "Reset"),
                                    false,
                                    false,
                                )
                                .on_click(cx.listener(|view, _, _, cx| {
                                    if let Some(font) = view
                                        .settings_extra_values
                                        .get("terminal.fontFamily")
                                        .and_then(serde_json::Value::as_str)
                                        .map(str::to_owned)
                                    {
                                        view.settings_font_family_input
                                            .update(cx, |input, cx| input.set_query(&font, cx));
                                    }
                                })),
                            ),
                    )
                    .children((!suggestions.is_empty()).then(|| {
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_1()
                            .children(suggestions.into_iter().enumerate().map(|(index, name)| {
                                let label = name.clone();
                                div()
                                    .id(("installed-terminal-font-suggestion", index))
                                    .cursor_pointer()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(rgb(BORDER))
                                    .bg(rgb(TOOLBAR))
                                    .hover(|style| style.bg(rgb(ACTIVE)))
                                    .px_2()
                                    .py_1()
                                    .text_xs()
                                    .text_color(rgb(TEXT))
                                    .child(label)
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        view.settings_font_family_input.update(cx, |input, cx| {
                                            input.set_query(&name, cx);
                                        });
                                    }))
                            }))
                    }))
            }))
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
                                .child(tr(locale, caption))
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    if view
                                        .settings_extra_values
                                        .get(path)
                                        .and_then(serde_json::Value::as_str)
                                        != Some(value)
                                    {
                                        view.change_shared_desktop_setting(
                                            path,
                                            serde_json::json!(value),
                                            cx,
                                        );
                                    }
                                }))
                        },
                    )),
            )
            .into_any_element()
    }
    /// Operational Hook toggles are not portable DesktopSettings scalars:
    /// Classic stores them in Host runtimeSettings with a transaction
    /// revision. They stay inert until a versioned Host snapshot is present.
    fn render_agent_hook_toggle(
        &self,
        row: &PreviewRow,
        agent: &'static str,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let Some(snapshot) = self.agent_hook_switches.as_ref() else {
            return self.render_host_state_row(row);
        };
        if self.agent_hook_write_loading {
            return div()
                .p_3()
                .bg(rgb(PANEL))
                .text_sm()
                .text_color(rgb(MUTED))
                .child(format!(
                    "{} — saving through Runtime Host…",
                    tr(self.settings_menu_locale, row.title)
                ))
                .into_any_element();
        }
        let enabled = snapshot.switches.get(agent).copied().unwrap_or(false);
        div()
            .p_3()
            .bg(rgb(PANEL))
            .flex()
            .flex_col()
            .gap_2()
            .child(
                Switch::new(format!("agent-hook-{agent}"))
                    .label(tr(self.settings_menu_locale, row.title))
                    .checked(enabled)
                    .on_change(cx.listener(move |view, next, _, cx| {
                        view.change_agent_hook_setting(agent, *next, cx);
                    })),
            )
            .child(div().text_xs().text_color(rgb(MUTED)).child(
                if self.agent_hook_ui_status.is_empty() {
                    format!(
                        "Runtime Host revision {} · changes use compare-and-set",
                        snapshot.revision
                    )
                } else {
                    self.agent_hook_ui_status.clone()
                },
            ))
            .into_any_element()
    }

    fn render_connected_settings_row(
        &self,
        section: &'static str,
        row: &PreviewRow,
        row_index: usize,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        if section == "application" && row.title == "App Language" {
            return Some(self.render_settings_language_row(row, cx));
        }
        match settings_operational_action(section, row.title) {
            Some(SettingsOperationalAction::ManageProjects) => {
                return Some(self.render_project_management_setting(cx));
            }
            Some(SettingsOperationalAction::ViewHostProjectSettings) => {
                return Some(self.render_host_project_settings(cx));
            }
            Some(SettingsOperationalAction::OpenQuotaOverview) => {
                return Some(self.render_provider_quotas_setting(cx));
            }
            Some(SettingsOperationalAction::SearchNativeKeyboardShortcuts) => {
                return Some(self.render_keyboard_shortcut_search(cx));
            }
            Some(SettingsOperationalAction::RefreshHostAgentProfiles) => {
                return Some(self.render_host_agent_profiles_setting(cx));
            }
            Some(SettingsOperationalAction::ConfigureNativeScrollback) => {
                return Some(self.render_native_scrollback_setting(cx));
            }
            None => {}
        }
        if section == "agents" {
            match row.title {
                "Codex Hooks" => return Some(self.render_agent_hook_toggle(row, "codex", cx)),
                "Claude Code Hooks" => {
                    return Some(self.render_agent_hook_toggle(row, "claude", cx));
                }
                _ => {}
            }
        }
        let path = shared_setting_row_path(section, row.title)?;
        if self.shared_catalog_state() != CatalogHostState::Ready {
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
        if path == "agents.builtinMcpEnabled" {
            let checked = value.as_bool()?;
            let locale = self.settings_menu_locale;
            let mut content = div()
                .flex()
                .flex_col()
                .child(self.native_settings_bool_row(row.title, path, checked, cx));
            if let Some(diagnostics) = self.settings_extra_values.get("builtinMcpDiagnostics") {
                let sessions = diagnostics["sessions"].as_array();
                let tools = diagnostics["tools"].as_array();
                content =
                    content.child(div().px_3().py_1().text_xs().text_color(rgb(MUTED)).child(
                        format!(
                            "{} · {} {}",
                            tr(locale, "Last Host snapshot"),
                            diagnostics["sessionCount"].as_u64().unwrap_or(0),
                            tr(locale, "Agent sessions")
                        ),
                    ));
                if let Some(sessions) = sessions {
                    for session in sessions {
                        let provider = session["provider"].as_str().unwrap_or("Agent");
                        let status = session["status"].as_str().unwrap_or("unknown");
                        let handle = session["handle"].as_str().unwrap_or("");
                        let short: String = handle.chars().take(12).collect();
                        content = content.child(
                            div()
                                .px_3()
                                .text_xs()
                                .child(format!("{provider} · {short} · {status}")),
                        );
                    }
                }
                if let Some(tools) = tools {
                    content =
                        content.child(div().px_3().py_1().text_xs().text_color(rgb(MUTED)).child(
                            format!(
                                "alera-workbench · {} {}",
                                tools.len(),
                                tr(locale, "Available MCP tools")
                            ),
                        ));
                    for name in tools.iter().filter_map(|tool| tool.as_str()) {
                        content = content.child(div().px_3().text_xs().child(name.to_owned()));
                    }
                }
            } else {
                content = content.child(
                    div()
                        .px_3()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(tr(locale, "MCP diagnostics unavailable")),
                );
            }
            return Some(self.render_described_shared_control(row, content.into_any_element()));
        }
        if let Some(checked) = value.as_bool() {
            return Some(self.render_described_shared_control(
                row,
                self.native_settings_bool_row(row.title, path, checked, cx),
            ));
        }
        let numeric = match path {
            "general.autoGitFetchIntervalMinutes" => Some((1.0, 1.0, 60.0)),
            "terminal.fontSize" => Some((1.0, 8.0, 32.0)),
            "terminal.lineHeight" => Some((0.1, 0.8, 2.4)),
            "terminal.backgroundOpacity" => Some((0.05, 0.0, 1.0)),
            "terminal.tuiScrollSensitivity" => Some((1.0, 1.0, 10.0)),
            "editor.tabSize" => Some((1.0, 1.0, 8.0)),
            "editor.autosaveDelaySeconds" => Some((1.0, 1.0, 60.0)),
            _ => None,
        };
        if let Some((step, min, max)) = numeric {
            return value.as_f64().map(|number| {
                self.render_described_shared_control(
                    row,
                    self.native_settings_number_row(row.title, path, number, step, min, max, cx),
                )
            });
        }
        value
            .as_str()
            .map(|selected| self.render_shared_selection_row(row, path, selected, row_index, cx))
    }

    /// All Classic Settings categories remain visible. Rows backed by the
    /// reviewed Rust Core contract get real controls; all others are explicit
    /// unavailable labels rather than fake toggles or buttons.
    pub(super) fn render_settings_catalog_preview(
        &self,
        selected: &'static SettingsSection,
        locale: UiLocale,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let support = settings_support_counts(selected);
        let mut children = vec![div()
            .id("native-settings-catalog-summary")
            .px_4()
            .py_2()
            .text_xs()
            .text_color(rgb(MUTED))
            .child(settings_support_summary(locale, support))
            .into_any_element()];
        // GPUI scroll_to_item targets painted direct children. Each group
        // must be a sibling of the summary for Classic's jump chips to work.
        children.extend(
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
                        .into_any_element()
                }),
        );
        children
    }
    /// Classic keeps the App Language control inside Application > Language.
    /// Keep the option labels in their own language and write via Host CAS.
    fn render_settings_language_row(
        &self,
        row: &PreviewRow,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        if self.shared_catalog_state() != CatalogHostState::Ready {
            return self.render_host_state_row(row);
        }
        let locale = self.settings_menu_locale;
        let selected = self.settings_menu_language.as_str();
        let selected_label = APP_LANGUAGE_CHOICES
            .iter()
            .find(|(preference, _)| *preference == selected)
            .map(|(_, label)| *label)
            .unwrap_or("Follow System");
        let owner = cx.entity().downgrade();
        div()
            .id("native-settings-app-language")
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .bg(rgb(PANEL))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_sm().text_color(rgb(TEXT)).child(tr(locale, row.title)))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child(tr(locale, row.detail)),
                    ),
            )
            .child(
                native_components::select_trigger(
                    "native-settings-locale-select",
                    selected_label,
                    false,
                )
                .dropdown_menu(move |mut menu, _, _| {
                    for (preference, label) in APP_LANGUAGE_CHOICES {
                        let owner = owner.clone();
                        menu = menu.item(
                            PopupMenuItem::new(label).on_click(move |_, _, cx| {
                                let _ = owner.update(cx, |view, cx| {
                                    if view.shared_catalog_state() != CatalogHostState::Ready {
                                        view.model.set_status(
                                            "Rust Host settings are unavailable; refresh before editing",
                                        );
                                    } else if view.settings_menu_language.as_str() != preference {
                                        view.change_shared_desktop_setting(
                                            "general.language",
                                            serde_json::json!(preference),
                                            cx,
                                        );
                                    }
                                });
                            }),
                        );
                    }
                    menu.scrollable(true).max_h(px(280.0))
                }),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{catalog_host_state, settings_support_summary, CatalogHostState};
    use alera_gpui_desktop::native_i18n::UiLocale;
    use alera_gpui_desktop::settings_menu_catalog::SettingsSupportCounts;

    #[test]
    fn s04_settings_capability_summary_follows_host_locale() {
        let counts = SettingsSupportCounts {
            editable: 6,
            actions: 2,
            read_only: 3,
            unavailable: 9,
        };
        assert_eq!(
            settings_support_summary(UiLocale::English, counts),
            "Editable: 6 · Actions: 2 · Read-only: 3 · Not available: 9"
        );
        assert_eq!(
            settings_support_summary(UiLocale::TraditionalChinese, counts),
            "可編輯設定: 6 · 操作: 2 · 唯讀: 3 · 尚未提供: 9"
        );
    }

    #[test]
    fn installed_snapshot_is_not_writable_when_runtime_host_disconnects() {
        assert_eq!(
            catalog_host_state(true, false, false),
            CatalogHostState::Ready
        );
        assert_eq!(
            catalog_host_state(true, true, false),
            CatalogHostState::Unavailable
        );
        assert_eq!(
            catalog_host_state(true, true, true),
            CatalogHostState::Loading
        );
    }

    #[test]
    fn missing_snapshot_reports_loading_until_the_host_reports_failure() {
        assert_eq!(
            catalog_host_state(false, false, false),
            CatalogHostState::Loading
        );
        assert_eq!(
            catalog_host_state(false, true, false),
            CatalogHostState::Unavailable
        );
        assert_eq!(
            catalog_host_state(false, true, true),
            CatalogHostState::Loading
        );
    }
}
