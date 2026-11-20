//! GPUI Kit owns all Native context/dropdown menu presentation and focus.
//! Only target validation and binding to Alera actions live here.

use super::native_agent_quota_ui::{classic_agent_svg_bytes, quota_provider_icon};
use super::*;
use alera_gpui_desktop::native_i18n::tr;
use alera_gpui_desktop::visual_tokens::new_tab_menu_max_height;

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;

/// SVG identities use GPUI Kit's native Item.icon slot, maintaining Classic's
/// visual alignment and the standard accessible menu label/key navigation.
/// Raster identities use ElementItem (with the same activation/dismissal
/// semantics), because GPUI Kit's icon slot only accepts SVG-backed Icons.
fn branded_agent_popup_item(agent_id: String, label: String) -> PopupMenuItem {
    if let Some(svg) = classic_agent_svg_bytes(&agent_id) {
        return PopupMenuItem::new(label).icon(Icon::default().data(svg));
    }
    if matches!(
        agent_id.as_str(),
        "cursor" | "agy" | "opencode" | "opencode2" | "amp"
    ) {
        // The standard PopupMenuItem reserves an empty xsmall icon (12px)
        // plus a 4px gap for ElementItem. Offset one icon slot so PNG logos
        // align with SVG/logo items, not a second indented column.
        return PopupMenuItem::element(move |_, _| {
            div()
                .flex()
                .min_w_0()
                .items_center()
                .gap_1()
                .ml(px(-16.0))
                .child(quota_provider_icon(&agent_id, 16.0))
                .child(label.clone())
        });
    }
    PopupMenuItem::new(label).icon(IconName::Terminal)
}

impl Workbench {
    fn kit_menu_host_is_active(&self, target: &TerminalTabContextTarget) -> bool {
        self.terminals.context_target_is_live(target)
            && self
                .terminals
                .active_tab
                .and_then(|index| self.terminals.context_target_at(index))
                .as_ref()
                == Some(target)
    }

    /// Classic's single + button creates a File/Terminal, while the optional
    /// Host session list remains an action rather than pretending to be a tab.
    pub(super) fn kit_new_tab_menu(
        owner: gpui::WeakEntity<Self>,
        target: Option<PaneAttachGuard>,
        running: Vec<(TerminalSession, String)>,
        profiles: Vec<AgentProfileRow>,
        installed: Vec<InstalledAgentCli>,
        installed_loading: bool,
        terminal_create_busy: bool,
        terminal_create_timed_out: bool,
        terminal_create_unknown: bool,
    ) -> impl Fn(
        gpui_kit::component::menu::PopupMenu,
        &mut Window,
        &mut Context<gpui_kit::component::menu::PopupMenu>,
    ) -> gpui_kit::component::menu::PopupMenu
           + 'static {
        move |mut menu, window, cx| {
            // Pick up changes to the shared App Language at popup-open time,
            // just like Tab right-click and pane action menus.
            let locale = owner
                .upgrade()
                .map(|entity| entity.read(cx).settings_menu_locale)
                .unwrap_or_else(|| UiLocale::from_preference("system"));
            let viewport_height = f32::from(window.viewport_size().height);
            let Some(target) = target.clone() else {
                return menu.label(tr(locale, "Pane no longer available; reopen New Tab"));
            };
            let new_file_owner = owner.clone();
            let new_file_target = target.clone();
            menu = menu.item(
                PopupMenuItem::new(tr(locale, "New File"))
                    .icon(IconName::File)
                    .on_click(move |_, _, cx| {
                        let _ = new_file_owner.update(cx, |view, cx| {
                            if new_file_target
                                .still_valid(view.workspace_generation, &view.mixed_panes)
                            {
                                view.new_editor_in_pane(new_file_target.pane(), cx);
                            } else {
                                view.model.set_status(tr(
                                    view.settings_menu_locale,
                                    "Pane changed; reopen New Tab",
                                ));
                                cx.notify();
                            }
                        });
                    }),
            );
            let new_terminal_owner = owner.clone();
            let new_terminal_target = target.clone();
            menu = menu.item(
                PopupMenuItem::new(tr(locale, "New Terminal"))
                    .icon(IconName::Terminal)
                    .on_click(move |_, _, cx| {
                        let _ = new_terminal_owner.update(cx, |view, cx| {
                            view.create_host_terminal_in_pane(new_terminal_target.clone(), cx);
                        });
                    }),
            );
            // Classic lists installed local Agent CLIs, not just Host Agent
            // Profiles. Inventory is read-only and works without any Host.
            menu = menu.separator().label(tr(locale, "Installed Agent CLIs"));
            if installed.is_empty() {
                menu = menu.label(tr(
                    locale,
                    if installed_loading {
                        "Detecting installed Agent CLIs…"
                    } else {
                        "No installed Agent CLIs detected"
                    },
                ));
            }
            for agent in installed.iter().cloned() {
                let agent_owner = owner.clone();
                let agent_target = target.clone();
                let agent_id = agent.id.clone();
                let label = agent.new_tab_menu_label().to_owned();
                menu = menu.item(branded_agent_popup_item(agent_id.clone(), label).on_click(
                    move |_, window, cx| {
                        let _ = agent_owner.update(cx, |view, cx| {
                            view.prepare_detected_agent_in_pane(
                                &agent_id,
                                &agent_target,
                                window,
                                cx,
                            );
                        });
                    },
                ));
            }
            let refresh_owner = owner.clone();
            menu = menu.item(
                PopupMenuItem::new(tr(locale, "Rescan installed agents"))
                    .icon(IconName::RefreshCw)
                    .on_click(move |_, _, cx| {
                        let _ = refresh_owner.update(cx, |view, cx| {
                            view.refresh_installed_agent_clis(cx);
                        });
                    }),
            );
            if !profiles.is_empty() {
                menu = menu.separator().label(tr(locale, "Host Agent Profiles"));
            }
            for profile in profiles.iter().cloned() {
                let owner = owner.clone();
                let agent_target = target.clone();
                let id = profile.id;
                let label = format!(
                    "{} · {} ({})",
                    tr(locale, "New Agent"),
                    profile.name,
                    profile.agent_type
                );
                let brand_id = profile.agent_type.clone();
                menu = menu.item(branded_agent_popup_item(brand_id, label).on_click(
                    move |_, window, cx| {
                        let _ = owner.update(cx, |view, cx| {
                            view.prepare_agent_in_pane(&id, &agent_target, window, cx);
                        });
                    },
                ));
            }
            if terminal_create_timed_out || terminal_create_unknown || terminal_create_busy {
                // Only add recovery chrome when an operation is actually
                // pending; keep normal New Tab usage close to Classic.
                menu = menu.separator().label(tr(locale, "Pending Host requests"));
            }
            if terminal_create_timed_out {
                let retry_owner = owner.clone();
                let retry_target = target.clone();
                // Retrying is global, but the saved creation intent still
                // determines which pane may receive the Host session.
                menu = menu.item(
                    PopupMenuItem::new(tr(locale, "Recheck pending Host session (no new PTY)"))
                        .on_click(move |_, _, cx| {
                            let _ = retry_owner.update(cx, |view, cx| {
                                if retry_target
                                    .still_valid(view.workspace_generation, &view.mixed_panes)
                                    && view.terminal_create_pane.is_some()
                                {
                                    view.retry_host_terminal_confirmation(cx);
                                }
                            });
                        }),
                );
            }
            if terminal_create_unknown {
                menu = menu.label(tr(
                    locale,
                    "Host create result unknown · inspect running sessions before dismissing",
                ));
            }
            if terminal_create_busy {
                let cancel_owner = owner.clone();
                let cancel_target = target.clone();
                // Local cancellation never terminates a Host PTY. Allow it
                // from either current pane if the original pane was closed.
                menu = menu.item(
                    PopupMenuItem::new(tr(locale, "Cancel pending request (keep Host PTY)"))
                        .on_click(move |_, _, cx| {
                            let _ = cancel_owner.update(cx, |view, cx| {
                                if cancel_target
                                    .still_valid(view.workspace_generation, &view.mixed_panes)
                                    && view.terminal_create_pane.is_some()
                                {
                                    view.cancel_host_terminal_confirmation(cx);
                                }
                            });
                        }),
                );
            }
            menu = menu
                .separator()
                .label(tr(locale, "Attach running Host session"));
            if running.is_empty() {
                menu = menu.label(tr(locale, "No running Host sessions"));
            }
            for (expected, title) in running.iter().cloned() {
                let owner = owner.clone();
                let attach_target = target.clone();
                menu = menu.item(PopupMenuItem::new(title).icon(IconName::Terminal).on_click(
                    move |_, window, cx| {
                        let _ = owner.update(cx, |view, cx| {
                            // Recheck handle + Host workspace + tab ID and pane
                            // generation at click time. Never follow reused IDs.
                            if let Some(identity) =
                                view.attach_live_terminal_to_pane(&expected, &attach_target, cx)
                            {
                                view.focus_mixed_tab(&identity, window, cx);
                            }
                        });
                    },
                ));
            }
            menu.scrollable(true)
                .max_h(px(new_tab_menu_max_height(viewport_height)))
        }
    }

    pub(super) fn kit_pane_actions_menu(
        owner: gpui::WeakEntity<Self>,
        active: Option<WorkbenchTabIdentity>,
        active_in_secondary: bool,
        has_secondary: bool,
    ) -> impl Fn(
        gpui_kit::component::menu::PopupMenu,
        &mut Window,
        &mut Context<gpui_kit::component::menu::PopupMenu>,
    ) -> gpui_kit::component::menu::PopupMenu
           + 'static {
        move |mut menu, _, cx| {
            // Use the authoritative shared Desktop locale when this popup
            // opens, not the locale when the Tab was first created.
            let locale = owner
                .upgrade()
                .map(|entity| entity.read(cx).settings_menu_locale)
                .unwrap_or_else(|| UiLocale::from_preference("system"));
            let file_owner = owner.clone();
            let pane = if active_in_secondary {
                PaneSlot::Secondary
            } else {
                PaneSlot::Primary
            };
            menu = menu.item(
                PopupMenuItem::new(tr(locale, "New File in this pane")).on_click(
                    move |_, _, cx| {
                        let _ = file_owner.update(cx, |view, cx| view.new_editor_in_pane(pane, cx));
                    },
                ),
            );
            menu = menu.separator();
            if let Some(WorkbenchTabIdentity::Host(target)) = active.clone() {
                menu = menu.label(tr(locale, "Terminal"));
                let refresh_owner = owner.clone();
                let refresh_target = target.clone();
                menu = menu.item(PopupMenuItem::new(tr(locale, "Refresh output")).on_click(
                    move |_, _, cx| {
                        let _ = refresh_owner.update(cx, |view, cx| {
                            if view.kit_menu_host_is_active(&refresh_target) {
                                view.refresh_terminal_output(cx);
                            }
                        });
                    },
                ));
                let copy_owner = owner.clone();
                let copy_target = target.clone();
                menu = menu.item(
                    PopupMenuItem::new(tr(locale, "Copy visible terminal")).on_click(
                        move |_, _, cx| {
                            let _ = copy_owner.update(cx, |view, cx| {
                                if view.kit_menu_host_is_active(&copy_target) {
                                    view.copy_terminal_text_for_pane(pane, false, cx);
                                }
                            });
                        },
                    ),
                );
                let selected_owner = owner.clone();
                let selected_target = target.clone();
                menu = menu.item(
                    PopupMenuItem::new(tr(locale, "Copy selected rows")).on_click(
                        move |_, _, cx| {
                            let _ = selected_owner.update(cx, |view, cx| {
                                if view.kit_menu_host_is_active(&selected_target) {
                                    view.copy_terminal_text_for_pane(pane, true, cx);
                                }
                            });
                        },
                    ),
                );
                let mode_owner = owner.clone();
                let mode_target = target.clone();
                menu = menu.item(
                    PopupMenuItem::new(tr(locale, "Toggle VT screen / text log")).on_click(
                        move |_, _, cx| {
                            let _ = mode_owner.update(cx, |view, cx| {
                                if view.kit_menu_host_is_active(&mode_target) {
                                    view.terminals.toggle_view_mode();
                                    view.save_terminal_layout();
                                    cx.notify();
                                }
                            });
                        },
                    ),
                );
                for (label, cols, rows) in
                    [("Resize: 80 × 24", 80, 24), ("Resize: 120 × 36", 120, 36)]
                {
                    let size_owner = owner.clone();
                    let size_target = target.clone();
                    menu = menu.item(PopupMenuItem::new(tr(locale, label)).on_click(
                        move |_, _, cx| {
                            let _ = size_owner.update(cx, |view, cx| {
                                if view.kit_menu_host_is_active(&size_target) {
                                    view.resize_terminal(cols, rows, cx);
                                }
                            });
                        },
                    ));
                }
                let split_owner = owner.clone();
                let split_target = target.clone();
                menu = menu.item(
                    PopupMenuItem::new(tr(locale, "Toggle Terminal split")).on_click(
                        move |_, _, cx| {
                            let _ = split_owner.update(cx, |view, cx| {
                                if view.kit_menu_host_is_active(&split_target) {
                                    view.toggle_terminal_split(cx);
                                }
                            });
                        },
                    ),
                );
                let direction_owner = owner.clone();
                let direction_target = target.clone();
                menu = menu.item(
                    PopupMenuItem::new(tr(locale, "Switch Terminal split direction")).on_click(
                        move |_, _, cx| {
                            let _ = direction_owner.update(cx, |view, cx| {
                                if view.kit_menu_host_is_active(&direction_target)
                                    && view.terminal_panes.secondary.is_some()
                                {
                                    view.terminal_panes.toggle_direction();
                                    cx.notify();
                                }
                            });
                        },
                    ),
                );
                menu = menu.separator();
            }
            if let Some(active) = active.clone() {
                let split_owner = owner.clone();
                let label = if active_in_secondary {
                    "Move tab to first pane"
                } else {
                    "Move tab to second pane"
                };
                let action = if active_in_secondary {
                    MixedContextAction::FirstPane
                } else {
                    MixedContextAction::SecondPane
                };
                menu = menu.item(PopupMenuItem::new(tr(locale, label)).on_click(
                    move |_, window, cx| {
                        let _ = split_owner.update(cx, |view, cx| {
                            if view.mixed_tab_is_current(&active, cx) {
                                view.mixed_tab_context = Some(active.clone());
                                view.apply_mixed_tab_context(action, window, cx);
                            }
                        });
                    },
                ));
            }
            if has_secondary {
                let axis_owner = owner.clone();
                menu = menu.item(
                    PopupMenuItem::new(tr(locale, "Switch split direction")).on_click(
                        move |_, _, cx| {
                            let _ = axis_owner.update(cx, |view, cx| {
                                view.mixed_panes.toggle_axis();
                                view.save_mixed_pane_layout(cx);
                                cx.notify();
                            });
                        },
                    ),
                );
                let owner = owner.clone();
                menu = menu.item(PopupMenuItem::new(tr(locale, "Close split pane")).on_click(
                    move |_, _, cx| {
                        let _ = owner.update(cx, |view, cx| view.unsplit_mixed_panes(cx));
                    },
                ));
            }
            if active.is_none() && !has_secondary {
                menu = menu.label(tr(locale, "Open a tab to split"));
            }
            menu
        }
    }

    /// GPUI Kit positions, focuses, and dismisses this menu. The callback
    /// revalidates the exact Explorer target at action time to avoid sending
    /// a stale or off-workspace path into file-system operations.
    pub(super) fn kit_explorer_context_menu(
        owner: gpui::WeakEntity<Self>,
        path: Option<PathBuf>,
    ) -> impl Fn(
        gpui_kit::component::menu::PopupMenu,
        &mut Window,
        &mut Context<gpui_kit::component::menu::PopupMenu>,
    ) -> gpui_kit::component::menu::PopupMenu
           + 'static {
        move |mut menu, _, cx| {
            let locale = owner
                .upgrade()
                .map(|entity| entity.read(cx).settings_menu_locale)
                .unwrap_or_else(|| UiLocale::from_preference("system"));
            for action in Self::explorer_actions_for_path(path.as_deref()) {
                let owner = owner.clone();
                let path = path.clone();
                menu = menu.item(PopupMenuItem::new(tr(locale, action.label())).on_click(
                    move |_, window, cx| {
                        let _ = owner.update(cx, |view, cx| {
                            view.run_kit_explorer_menu_action(path.clone(), action, window, cx);
                        });
                    },
                ));
            }
            menu
        }
    }

    fn run_kit_explorer_menu_action(
        &mut self,
        path: Option<PathBuf>,
        action: ExplorerMenuAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings_section.is_some() {
            return;
        }
        if let Some(ref path) = path {
            if !path.starts_with(self.model.root()) || !path.exists() {
                self.model.set_status(tr(
                    self.settings_menu_locale,
                    "Explorer item changed; refresh the folder",
                ));
                cx.notify();
                return;
            }
            self.explorer_multi_selection.context_target(path);
        } else {
            self.explorer_multi_selection.clear();
        }
        self.explorer_selection = path.clone();
        self.explorer_delete_confirmed = false;
        self.explorer_delete_pending = None;
        self.explorer_context_menu = Some(ExplorerContextMenu { path });
        self.run_explorer_menu_action(action, window, cx);
    }

    pub(super) fn kit_mixed_tab_context_menu(
        owner: gpui::WeakEntity<Self>,
        identity: WorkbenchTabIdentity,
        secondary: bool,
        pinned: bool,
    ) -> impl Fn(
        gpui_kit::component::menu::PopupMenu,
        &mut Window,
        &mut Context<gpui_kit::component::menu::PopupMenu>,
    ) -> gpui_kit::component::menu::PopupMenu
           + 'static {
        move |mut menu, _, cx| {
            let locale = owner
                .upgrade()
                .map(|entity| entity.read(cx).settings_menu_locale)
                .unwrap_or_else(|| UiLocale::from_preference("system"));
            let actions = [
                (MixedContextAction::Focus, "Focus"),
                (
                    MixedContextAction::TogglePin,
                    if pinned { "Unpin Tab" } else { "Pin Tab" },
                ),
                (MixedContextAction::Close, "Close"),
                (MixedContextAction::CloseOthers, "Close others"),
                (MixedContextAction::CloseToRight, "Close to right"),
                (MixedContextAction::MoveLeft, "Move left"),
                (MixedContextAction::MoveRight, "Move right"),
                (
                    if secondary {
                        MixedContextAction::FirstPane
                    } else {
                        MixedContextAction::SecondPane
                    },
                    if secondary {
                        "Move to first pane"
                    } else {
                        "Move to second pane"
                    },
                ),
            ];
            for (action, label) in actions {
                let owner = owner.clone();
                let identity = identity.clone();
                menu = menu.item(PopupMenuItem::new(tr(locale, label)).on_click(
                    move |_, window, cx| {
                        let _ = owner.update(cx, |view, cx| {
                            let expected = if secondary {
                                PaneSlot::Secondary
                            } else {
                                PaneSlot::Primary
                            };
                            if view.mixed_tab_is_current(&identity, cx)
                                && view.mixed_tabs.contains(&identity)
                                && view.mixed_panes.tabs_in(expected).contains(&identity)
                            {
                                view.mixed_tab_context = Some(identity.clone());
                                view.apply_mixed_tab_context(action, window, cx);
                            } else {
                                view.model.set_status(tr(
                                    view.settings_menu_locale,
                                    "Tab changed; action cancelled",
                                ));
                                cx.notify();
                            }
                        });
                    },
                ));
            }
            menu
        }
    }

    /// Kit-owned dropdown retains the keyboard/focus overlay; Alera only
    /// validates the selected branch against a fresh snapshot before filling
    /// the input. A selection never performs checkout or merge.
    pub(super) fn render_kit_branch_dropdown(
        &self,
        id: &'static str,
        label: String,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let choices = self.native_branch_choices();
        let owner = cx.entity().downgrade();
        native_components::select_trigger(id, label, false)
            .dropdown_menu(move |mut menu, _, _| {
                if choices.is_empty() {
                    menu = menu.label("No local branches yet");
                }
                for name in choices.iter().cloned() {
                    let owner = owner.clone();
                    menu = menu.item(PopupMenuItem::new(name.clone()).on_click(move |_, _, cx| {
                        let _ = owner.update(cx, |view, cx| {
                            view.select_kit_branch(name.clone(), cx);
                        });
                    }));
                }
                menu.scrollable(true).max_h(px(320.0))
            })
            .into_any_element()
    }

    fn select_kit_branch(&mut self, branch: String, cx: &mut Context<Self>) {
        if !self
            .native_branch_choices()
            .iter()
            .any(|name| name == &branch)
        {
            self.model
                .set_status("Branch list changed; refresh before selecting");
            cx.notify();
            return;
        }
        self.git_merge_armed = None;
        self.git_branch_input
            .update(cx, |input, cx| input.set_query(&branch, cx));
        self.right_sidebar_view = RightSidebarView::Git;
        self.model.set_status(format!(
            "Selected {branch}. Use Compare or Switch to proceed."
        ));
        cx.notify();
    }

    fn explorer_actions_for_path(path: Option<&Path>) -> Vec<ExplorerMenuAction> {
        if let Some(path) = path {
            let mut actions = vec![
                ExplorerMenuAction::File(ExplorerAction::NewFile),
                ExplorerMenuAction::File(ExplorerAction::NewFolder),
                ExplorerMenuAction::Clipboard(ExplorerClipboardMode::Copy),
                ExplorerMenuAction::Clipboard(ExplorerClipboardMode::Cut),
                ExplorerMenuAction::Paste,
                ExplorerMenuAction::CopyAbsolutePath,
                ExplorerMenuAction::CopyRelativePath,
                ExplorerMenuAction::RevealInFileManager,
                ExplorerMenuAction::File(ExplorerAction::Duplicate),
                ExplorerMenuAction::File(ExplorerAction::Move),
                ExplorerMenuAction::File(ExplorerAction::Copy),
            ];
            if path.is_file() {
                actions.extend([
                    ExplorerMenuAction::OpenInAlera,
                    ExplorerMenuAction::OpenDefaultApplication,
                    ExplorerMenuAction::ViewCurrentGitDiff,
                    ExplorerMenuAction::GitHistory,
                ]);
            }
            actions.push(ExplorerMenuAction::File(ExplorerAction::Rename));
            if path.is_dir() {
                actions.push(ExplorerMenuAction::CollapseFolder);
            }
            actions.extend([
                ExplorerMenuAction::Refresh,
                ExplorerMenuAction::File(ExplorerAction::Delete),
            ]);
            actions
        } else {
            vec![
                ExplorerMenuAction::File(ExplorerAction::NewFile),
                ExplorerMenuAction::File(ExplorerAction::NewFolder),
                ExplorerMenuAction::Paste,
            ]
        }
    }

    /// File-system operations on Git tree folders use the same guarded
    /// clipboard as regular Git files. Expand/collapse is a separate action.
    pub(super) fn kit_git_folder_menu(
        owner: gpui::WeakEntity<Self>,
        folder: FolderKey,
    ) -> impl Fn(
        gpui_kit::component::menu::PopupMenu,
        &mut Window,
        &mut Context<gpui_kit::component::menu::PopupMenu>,
    ) -> gpui_kit::component::menu::PopupMenu + 'static {
        move |mut menu, _, cx| {
            let locale = owner
                .upgrade()
                .map(|entity| entity.read(cx).settings_menu_locale)
                .unwrap_or_else(|| UiLocale::from_preference("system"));
            for (label, action) in [
                ("Copy worktree folder / selected items", GitFolderMenuAction::Copy),
                ("Cut worktree folder / selected items", GitFolderMenuAction::Cut),
                ("Paste into this folder", GitFolderMenuAction::Paste),
                ("Reveal folder in file manager", GitFolderMenuAction::Reveal),
                ("Copy relative folder path", GitFolderMenuAction::CopyPath),
            ] {
                let owner = owner.clone();
                let folder = folder.clone();
                menu = menu.item(PopupMenuItem::new(tr(locale, label)).on_click(move |_, _, cx| {
                    let _ = owner.update(cx, |view, cx| {
                        view.run_git_folder_menu_action(folder.clone(), action, cx);
                    });
                }));
            }
            menu
        }
    }

    /// Classic-style Git Graph branch context menu. Keep presentation in
    /// GPUI Kit and route every action through Workbench's existing handlers.
    /// Git file actions live in GPUI Kit's contextual menu rather than a
    /// permanent row of icons. Stage/Unstage work on selected files in the
    /// target area; destructive discard always requires a second click.
    pub(super) fn kit_git_change_menu(
        owner: gpui::WeakEntity<Self>,
        row: alera_gpui_desktop::git_inspect::GitChangeRow,
        discard_armed: bool,
    ) -> impl Fn(
        gpui_kit::component::menu::PopupMenu,
        &mut Window,
        &mut Context<gpui_kit::component::menu::PopupMenu>,
    ) -> gpui_kit::component::menu::PopupMenu
           + 'static {
        move |mut menu, _, cx| {
            let locale = owner
                .upgrade()
                .map(|entity| entity.read(cx).settings_menu_locale)
                .unwrap_or_else(|| UiLocale::from_preference("system"));
            let mut actions = vec![
                ("Preview changes", GitChangeMenuAction::Preview),
                (
                    "Open worktree file in Alera",
                    GitChangeMenuAction::OpenWorkingFile,
                ),
                (
                    "Open worktree file externally",
                    GitChangeMenuAction::OpenExternal,
                ),
                ("Copy worktree file(s)", GitChangeMenuAction::CopyFiles),
                ("Cut worktree file(s)", GitChangeMenuAction::CutFiles),
                ("Paste into containing folder", GitChangeMenuAction::PasteFiles),
                ("Reveal worktree file in file manager", GitChangeMenuAction::RevealFile),
                ("Copy relative path", GitChangeMenuAction::CopyPath),
            ];
            if matches!(
                row.area,
                alera_gpui_desktop::git_inspect::GitChangeArea::Staged
                    | alera_gpui_desktop::git_inspect::GitChangeArea::Changes
            ) {
                actions.push((
                    "Open Git original in read-only tab (HEAD / Index)",
                    GitChangeMenuAction::Original,
                ));
            }
            // Git paths can be tracked even after deletion; unlike Explorer,
            // the History action must not require an existing worktree file.
            if row.area != alera_gpui_desktop::git_inspect::GitChangeArea::Untracked {
                actions.push(("View File History", GitChangeMenuAction::History));
            } else {
                actions.push(("Add to .gitignore", GitChangeMenuAction::AddToGitIgnore));
            }
            if row.area.can_stage() {
                actions.push(("Stage selected", GitChangeMenuAction::Stage));
            }
            if row.area.can_unstage() {
                actions.push(("Unstage selected", GitChangeMenuAction::Unstage));
            }
            if row.area == alera_gpui_desktop::git_inspect::GitChangeArea::Untracked {
                actions.push((
                    "Move untracked file(s) to Recycle Bin (confirm)",
                    GitChangeMenuAction::TrashUntracked,
                ));
            }
            if row.area == alera_gpui_desktop::git_inspect::GitChangeArea::Changes {
                actions.push((
                    if discard_armed {
                        "Confirm Discard unstaged changes"
                    } else {
                        "Discard unstaged changes (confirm)"
                    },
                    GitChangeMenuAction::Discard,
                ));
            }
            for (label, action) in actions {
                let owner = owner.clone();
                let row = row.clone();
                menu = menu.item(PopupMenuItem::new(tr(locale, label)).on_click(move |_, _, cx| {
                    let _ = owner.update(cx, |view, cx| {
                        view.run_git_change_menu_action(row.clone(), action, cx);
                    });
                }));
            }
            menu
        }
    }

    /// A true History Tab perspective popup: presets and exact snapshot refs,
    /// with no mutable Git actions and no ad-hoc text revspec execution.
    pub(super) fn kit_git_center_perspective_menu(
        owner: gpui::WeakEntity<Self>,
        branches: Vec<GitBranchRow>,
        active: GitGraphCenterPerspective,
        generation: u64,
    ) -> impl Fn(
        gpui_kit::component::menu::PopupMenu,
        &mut Window,
        &mut Context<gpui_kit::component::menu::PopupMenu>,
    ) -> gpui_kit::component::menu::PopupMenu
           + 'static {
        move |mut menu, window, _| {
            menu = menu.label("Git History perspective");
            for (label, choice) in [
                ("Current branch", GitGraphCenterPerspective::Current),
                ("All branches", GitGraphCenterPerspective::AllBranches),
                ("Favorite branches", GitGraphCenterPerspective::Favorites),
            ] {
                let owner = owner.clone();
                let selected = choice == active;
                menu = menu.item(
                    PopupMenuItem::new(format!("{}{}", if selected { "✓ " } else { "" }, label))
                        .on_click(move |_, _, cx| {
                            let _ = owner.update(cx, |view, cx| {
                                view.select_git_graph_center_perspective(
                                    choice.clone(),
                                    generation,
                                    cx,
                                );
                            });
                        }),
                );
            }
            menu = menu.separator();
            menu = menu.label("Choose a branch (read-only)");
            let owner_search = owner.clone();
            menu = menu.item(PopupMenuItem::new("Search all loaded branches…").on_click(
                move |_, window, cx| {
                    let _ = owner_search.update(cx, |view, cx| {
                        view.open_git_center_branch_picker(
                            GitGraphBranchPickerMode::Perspective,
                            generation,
                            cx,
                        );
                        window.focus(&view.git_branch_picker_input.read(cx).focus_handle(cx), cx);
                    });
                },
            ));
            menu = menu.label(format!(
                "{} refs available · first 24 shown",
                branches.len()
            ));
            for branch in branches.iter().take(24) {
                let choice = GitGraphCenterPerspective::Branch {
                    name: branch.name.clone(),
                    remote: branch.remote,
                };
                let selected = choice == active;
                let title = format!(
                    "{}{}{}",
                    if selected {
                        "✓ "
                    } else if branch.favorite {
                        "★ "
                    } else {
                        ""
                    },
                    if branch.remote { "Remote: " } else { "Local: " },
                    branch.name
                );
                let owner = owner.clone();
                menu = menu.item(PopupMenuItem::new(title).on_click(move |_, _, cx| {
                    let _ = owner.update(cx, |view, cx| {
                        view.select_git_graph_center_perspective(choice.clone(), generation, cx);
                    });
                }));
            }
            // Keep all bounded refs accessible without extending beyond the
            // window height, even for repositories with 80 branches.
            let height = (f32::from(window.viewport_size().height) - 120.0)
                .max(160.0)
                .min(480.0);
            menu.scrollable(true).max_h(px(height))
        }
    }

    /// Three mutually exclusive Classic-compatible display-only projections.
    pub(super) fn kit_git_center_merged_visibility_menu(
        owner: gpui::WeakEntity<Self>,
        active: GitMergedBranchVisibility,
        generation: u64,
    ) -> impl Fn(
        gpui_kit::component::menu::PopupMenu,
        &mut Window,
        &mut Context<gpui_kit::component::menu::PopupMenu>,
    ) -> gpui_kit::component::menu::PopupMenu
           + 'static {
        move |mut menu, _, _| {
            menu = menu.label("Merged Branch Visibility");
            for choice in [
                GitMergedBranchVisibility::ShowAll,
                GitMergedBranchVisibility::HideMergedNames,
                GitMergedBranchVisibility::HideMergedGraph,
            ] {
                let owner = owner.clone();
                let label = format!(
                    "{}{}",
                    if choice == active { "✓ " } else { "" },
                    choice.label()
                );
                menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                    let _ = owner.update(cx, |view, cx| {
                        view.set_git_graph_merged_visibility(choice, generation, cx);
                    });
                }));
            }
            menu
        }
    }

    /// List exact local and remote merge targets from the same bounded Git
    /// snapshot. The popup is viewport-scrolled, not a large off-screen menu.
    pub(super) fn kit_git_center_merged_target_menu(
        owner: gpui::WeakEntity<Self>,
        branches: Vec<GitBranchRow>,
        current: String,
        active: Option<String>,
        generation: u64,
    ) -> impl Fn(
        gpui_kit::component::menu::PopupMenu,
        &mut Window,
        &mut Context<gpui_kit::component::menu::PopupMenu>,
    ) -> gpui_kit::component::menu::PopupMenu
           + 'static {
        move |mut menu, window, _| {
            menu = menu.label("Merged Into (read-only)");
            let owner_current = owner.clone();
            menu = menu.item(
                PopupMenuItem::new(format!(
                    "{}Current: {}",
                    if active.is_none() { "✓ " } else { "" },
                    current
                ))
                .on_click(move |_, _, cx| {
                    let _ = owner_current.update(cx, |view, cx| {
                        view.set_git_graph_merged_target(None, generation, cx);
                    });
                }),
            );
            menu = menu.separator();
            let owner_search = owner.clone();
            menu = menu.item(
                PopupMenuItem::new("Search all merged-into targets…").on_click(
                    move |_, window, cx| {
                        let _ = owner_search.update(cx, |view, cx| {
                            view.open_git_center_branch_picker(
                                GitGraphBranchPickerMode::MergeTarget,
                                generation,
                                cx,
                            );
                            window
                                .focus(&view.git_branch_picker_input.read(cx).focus_handle(cx), cx);
                        });
                    },
                ),
            );
            menu = menu.label(format!(
                "{} refs available · first 24 shown",
                branches.len()
            ));
            for branch in branches.iter().filter(|branch| !branch.current).take(24) {
                let reference = if branch.remote {
                    format!("refs/remotes/{}", branch.name)
                } else {
                    branch.name.clone()
                };
                let title = format!(
                    "{}{}{}",
                    if active.as_deref() == Some(reference.as_str()) {
                        "✓ "
                    } else {
                        ""
                    },
                    if branch.remote { "Remote: " } else { "Local: " },
                    branch.name
                );
                let owner = owner.clone();
                menu = menu.item(PopupMenuItem::new(title).on_click(move |_, _, cx| {
                    let _ = owner.update(cx, |view, cx| {
                        view.set_git_graph_merged_target(Some(reference.clone()), generation, cx);
                    });
                }));
            }
            let height = (f32::from(window.viewport_size().height) - 120.0)
                .max(160.0)
                .min(480.0);
            menu.scrollable(true).max_h(px(height))
        }
    }

    /// Compare both complete committed tips without checking out any branch.
    /// The center Graph supports both local and remote refs, in either direction.
    pub(super) fn kit_git_center_compare_menu(
        owner: gpui::WeakEntity<Self>,
        branches: Vec<GitBranchRow>,
        selected_ref: Option<String>,
        generation: u64,
    ) -> impl Fn(
        gpui_kit::component::menu::PopupMenu,
        &mut Window,
        &mut Context<gpui_kit::component::menu::PopupMenu>,
    ) -> gpui_kit::component::menu::PopupMenu
           + 'static {
        move |mut menu, window, _| {
            let selected_branch = selected_ref.as_deref().and_then(|reference| {
                branches.iter()
                    .find(|branch| {
                        if branch.current { return false; }
                        let key = if branch.remote {
                            format!("refs/remotes/{}", branch.name)
                        } else {
                            branch.name.clone()
                        };
                        key == reference || (!branch.remote && format!("refs/heads/{}", key) == reference)
                    })
                    .map(|branch| if branch.remote {
                        format!("refs/remotes/{}", branch.name)
                    } else {
                        branch.name.clone()
                    })
            });
            menu = menu.label("Compare branches (read-only)");
            if let Some(selected) = selected_branch {
                for (label, direction) in [
                    (
                        "HEAD -> selected (committed tips)",
                        CompareDirection::CurrentToSelected,
                    ),
                    (
                        "Selected -> HEAD (committed tips)",
                        CompareDirection::SelectedToCurrent,
                    ),
                ] {
                    let owner = owner.clone();
                    let branch = selected.clone();
                    menu = menu.item(PopupMenuItem::new(format!("{label} · {branch}")).on_click(
                        move |_, _, cx| {
                            let _ = owner.update(cx, |view, cx| {
                                view.compare_git_graph_center_branch(
                                    branch.clone(),
                                    direction,
                                    generation,
                                    cx,
                                );
                            });
                        },
                    ));
                }
                menu = menu.separator();
            }
            menu = menu.label("Compare with HEAD");
            let owner_search = owner.clone();
            menu = menu.item(
                PopupMenuItem::new("Search local or remote branches to compare...").on_click(
                    move |_, window, cx| {
                        let _ = owner_search.update(cx, |view, cx| {
                            view.open_git_center_branch_picker(
                                GitGraphBranchPickerMode::Compare,
                                generation,
                                cx,
                            );
                            window
                                .focus(&view.git_branch_picker_input.read(cx).focus_handle(cx), cx);
                        });
                    },
                ),
            );
            let mut added = 0usize;
            for branch in branches
                .iter()
                .filter(|branch| !branch.current)
                .take(24)
            {
                added += 1;
                let owner = owner.clone();
                let branch_name = if branch.remote {
                    format!("refs/remotes/{}", branch.name)
                } else {
                    branch.name.clone()
                };
                let display_name = format!(
                    "{}{}{}",
                    if branch.favorite { "★ " } else { "" },
                    if branch.remote { "Remote: " } else { "Local: " },
                    branch.name
                );
                menu = menu.item(
                    PopupMenuItem::new(display_name)
                    .on_click(move |_, _, cx| {
                        let _ = owner.update(cx, |view, cx| {
                            view.compare_git_graph_center_branch(
                                branch_name.clone(),
                                CompareDirection::CurrentToSelected,
                                generation,
                                cx,
                            );
                        });
                    }),
                );
            }
            if added == 0 {
                menu = menu.label("No other branches in this perspective");
            }
            let height = (f32::from(window.viewport_size().height) - 120.0)
                .max(160.0)
                .min(480.0);
            menu.scrollable(true).max_h(px(height))
        }
    }

    pub(super) fn kit_git_graph_branch_menu(
        owner: gpui::WeakEntity<Self>,
        branch: String,
        remote: bool,
        current: bool,
        has_worktree: bool,
        armed_remote: Option<(String, bool)>,
        armed_delete: Option<String>,
    ) -> impl Fn(
        gpui_kit::component::menu::PopupMenu,
        &mut Window,
        &mut Context<gpui_kit::component::menu::PopupMenu>,
    ) -> gpui_kit::component::menu::PopupMenu
           + 'static {
        move |mut menu, _, _| {
            menu = menu.label(if remote {
                "Remote branch"
            } else if current {
                "Current branch"
            } else {
                "Local branch"
            });
            let mut actions = vec![("Show branch history", GitGraphBranchMenuAction::History)];
            if current && !remote {
                actions.push((
                    "Rename current worktree branch…",
                    GitGraphBranchMenuAction::PrepareRename,
                ));
            }
            if !remote && !current {
                actions.push((
                    "Compare: changes on selected branch",
                    GitGraphBranchMenuAction::Compare,
                ));
                actions.push((
                    "Compare: changes on current branch",
                    GitGraphBranchMenuAction::CompareReverse,
                ));
                actions.push((
                    "Rebase current branch onto this branch (preview)...",
                    GitGraphBranchMenuAction::PreviewRebase,
                ));
                if has_worktree {
                    actions.push((
                        "Open existing worktree",
                        GitGraphBranchMenuAction::OpenWorktree,
                    ));
                } else {
                    actions.push(("Switch to branch", GitGraphBranchMenuAction::Switch));
                    actions.push((
                        "Rename inactive branch…",
                        GitGraphBranchMenuAction::PrepareRename,
                    ));
                }
                actions.push((
                    if armed_remote.as_ref() == Some(&(branch.clone(), true)) {
                        "Confirm Pull branch without checkout"
                    } else {
                        "Pull branch without checkout (confirm)"
                    },
                    GitGraphBranchMenuAction::PullBranch,
                ));
                actions.push((
                    if armed_remote.as_ref() == Some(&(branch.clone(), false)) {
                        "Confirm Push branch without checkout"
                    } else {
                        "Push branch without checkout (confirm)"
                    },
                    GitGraphBranchMenuAction::PushBranch,
                ));
                if !has_worktree {
                    actions.push((
                        if armed_delete.as_deref() == Some(branch.as_str()) {
                            "Confirm Delete merged local branch"
                        } else {
                            "Delete merged local branch (confirm)"
                        },
                        GitGraphBranchMenuAction::DeleteMerged,
                    ));
                }
                actions.push((
                    "Merge into current branch (confirm)",
                    GitGraphBranchMenuAction::Merge,
                ));
            }
            actions.push(("Copy branch name", GitGraphBranchMenuAction::CopyName));
            for (title, action) in actions {
                let owner = owner.clone();
                let branch = branch.clone();
                menu = menu.item(PopupMenuItem::new(title).on_click(move |_, _, cx| {
                    let _ = owner.update(cx, |view, cx| {
                        view.run_git_graph_branch_menu(branch.clone(), remote, action, cx);
                    });
                }));
            }
            menu
        }
    }

    /// Commit menu only prepares explicit, validated Git operations. The
    /// actual soft reset is only reachable after a separate double-confirm.
    pub(super) fn kit_git_graph_commit_menu(
        owner: gpui::WeakEntity<Self>,
        oid: String,
        subject: String,
        parents: usize,
    ) -> impl Fn(
        gpui_kit::component::menu::PopupMenu,
        &mut Window,
        &mut Context<gpui_kit::component::menu::PopupMenu>,
    ) -> gpui_kit::component::menu::PopupMenu
           + 'static {
        move |mut menu, _, _| {
            let preview = owner.clone();
            let preview_oid = oid.clone();
            menu = menu.item(PopupMenuItem::new("Preview commit changes").on_click(
                move |_, _, cx| {
                    let _ = preview.update(cx, |view, cx| {
                        let exists = view.git_history.as_ref().is_some_and(|graph| {
                            graph.commits.iter().any(|commit| commit.oid == preview_oid)
                        });
                        if exists {
                            view.preview_git_history(
                                GitReadRequest::Commit(preview_oid.clone()),
                                cx,
                            );
                        } else {
                            view.model
                                .set_status("Commit list changed; refresh Git Graph");
                            cx.notify();
                        }
                    });
                },
            ));
            for (title, kind) in [
                (
                    "Create local branch at commit…",
                    alera_gpui_desktop::git_actions::GitCommitRefKind::LocalBranch,
                ),
                (
                    "Create lightweight tag at commit…",
                    alera_gpui_desktop::git_actions::GitCommitRefKind::LightweightTag,
                ),
            ] {
                let owner = owner.clone();
                let commit_id = oid.clone();
                menu = menu.item(PopupMenuItem::new(title).on_click(move |_, _, cx| {
                    let _ = owner.update(cx, |view, cx| {
                        view.prepare_git_commit_ref(commit_id.clone(), kind, cx);
                    });
                }));
            }
            let apply_choices = if parents == 2 {
                vec![
                    (
                        "Cherry-pick merge (mainline parent 1)…",
                        alera_gpui_desktop::git_actions::GitCommitApplyKind::CherryPick,
                        1,
                    ),
                    (
                        "Cherry-pick merge (mainline parent 2)…",
                        alera_gpui_desktop::git_actions::GitCommitApplyKind::CherryPick,
                        2,
                    ),
                    (
                        "Revert merge (mainline parent 1)…",
                        alera_gpui_desktop::git_actions::GitCommitApplyKind::Revert,
                        1,
                    ),
                    (
                        "Revert merge (mainline parent 2)…",
                        alera_gpui_desktop::git_actions::GitCommitApplyKind::Revert,
                        2,
                    ),
                ]
            } else if parents == 1 {
                vec![
                    (
                        "Cherry-pick commit (preview and confirm)…",
                        alera_gpui_desktop::git_actions::GitCommitApplyKind::CherryPick,
                        0,
                    ),
                    (
                        "Revert commit (preview and confirm)…",
                        alera_gpui_desktop::git_actions::GitCommitApplyKind::Revert,
                        0,
                    ),
                ]
            } else {
                Vec::new()
            };
            for (title, kind, mainline) in apply_choices {
                let owner = owner.clone();
                let commit_id = oid.clone();
                menu = menu.item(PopupMenuItem::new(title).on_click(move |_, _, cx| {
                    let _ = owner.update(cx, |view, cx| {
                        view.preview_git_commit_apply(commit_id.clone(), kind, mainline, cx);
                    });
                }));
            }
            for (label, mode) in [
                (
                    "Soft Reset current branch here (preview)...",
                    NativeResetMode::Soft,
                ),
                (
                    "Mixed Reset current branch here (preview)...",
                    NativeResetMode::Mixed,
                ),
            ] {
                let reset_owner = owner.clone();
                let reset_oid = oid.clone();
                menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                    let _ = reset_owner.update(cx, |view, cx| {
                        view.preview_git_soft_reset(reset_oid.clone(), mode, cx);
                    });
                }));
            }
            let create_worktree = owner.clone();
            let worktree_sha = oid.clone();
            menu = menu.item(PopupMenuItem::new("Create Worktree at commit…").on_click(
                move |_, _, cx| {
                    let _ = create_worktree.update(cx, |view, cx| {
                        view.prepare_git_worktree_at_commit(worktree_sha.clone(), cx);
                    });
                },
            ));
            let hash = oid.clone();
            menu = menu.item(
                PopupMenuItem::new("Copy commit hash").on_click(move |_, _, cx| {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(hash.clone()));
                }),
            );
            let copy_subject = subject.clone();
            menu = menu.item(PopupMenuItem::new("Copy commit subject").on_click(
                move |_, _, cx| {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(copy_subject.clone()));
                },
            ));
            menu
        }
    }

    pub(super) fn native_branch_choices(&self) -> Vec<String> {
        let Some(graph) = &self.git_history else {
            return Vec::new();
        };
        let mut branches = graph
            .branches
            .iter()
            .filter(|branch| !branch.remote)
            .collect::<Vec<_>>();
        branches.sort_by(|left, right| {
            right
                .current
                .cmp(&left.current)
                .then(right.favorite.cmp(&left.favorite))
                .then_with(|| left.name.cmp(&right.name))
        });
        branches
            .into_iter()
            .map(|branch| branch.name.clone())
            .take(80)
            .collect()
    }
}
