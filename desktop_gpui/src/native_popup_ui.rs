//! GPUI Kit owns all Native context/dropdown menu presentation and focus.
//! Only target validation and binding to Alera actions live here.

use super::*;

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
        terminal_create_busy: bool,
        terminal_create_timed_out: bool,
    ) -> impl Fn(
        gpui_kit::component::menu::PopupMenu,
        &mut Window,
        &mut Context<gpui_kit::component::menu::PopupMenu>,
    ) -> gpui_kit::component::menu::PopupMenu
           + 'static {
        move |mut menu, _, _| {
            let Some(target) = target.clone() else {
                return menu.label("Pane no longer available; reopen New Tab");
            };
            let new_file_owner = owner.clone();
            let new_file_target = target.clone();
            menu = menu.item(PopupMenuItem::new("New File").on_click(move |_, _, cx| {
                let _ = new_file_owner.update(cx, |view, cx| {
                    if new_file_target.still_valid(view.workspace_generation, &view.mixed_panes) {
                        view.new_editor_in_pane(new_file_target.pane(), cx);
                    } else {
                        view.model.set_status("Pane changed; reopen New Tab");
                        cx.notify();
                    }
                });
            }));
            let new_terminal_owner = owner.clone();
            let new_terminal_target = target.clone();
            menu = menu.item(
                PopupMenuItem::new("New Terminal").on_click(move |_, _, cx| {
                    let _ = new_terminal_owner.update(cx, |view, cx| {
                        view.create_host_terminal_in_pane(new_terminal_target.clone(), cx);
                    });
                }),
            );
            if !profiles.is_empty() {
                menu = menu.separator();
            }
            for profile in profiles.iter().cloned() {
                let owner = owner.clone();
                let agent_target = target.clone();
                let id = profile.id;
                let label = format!("New Agent · {} ({})", profile.name, profile.agent_type);
                menu = menu.item(PopupMenuItem::new(label).on_click(move |_, window, cx| {
                    let _ = owner.update(cx, |view, cx| {
                        view.prepare_agent_in_pane(&id, &agent_target, window, cx);
                    });
                }));
            }
            if terminal_create_timed_out {
                let retry_owner = owner.clone();
                let retry_target = target.clone();
                // Retrying is global, but the saved creation intent still
                // determines which pane may receive the Host session.
                menu = menu.item(
                    PopupMenuItem::new("Recheck pending Host session (no new PTY)").on_click(
                        move |_, _, cx| {
                            let _ = retry_owner.update(cx, |view, cx| {
                                if retry_target
                                    .still_valid(view.workspace_generation, &view.mixed_panes)
                                    && view.terminal_create_pane.is_some()
                                {
                                    view.retry_host_terminal_confirmation(cx);
                                }
                            });
                        },
                    ),
                );
            }
            if terminal_create_busy {
                let cancel_owner = owner.clone();
                let cancel_target = target.clone();
                // Local cancellation never terminates a Host PTY. Allow it
                // from either current pane if the original pane was closed.
                menu = menu.item(
                    PopupMenuItem::new("Cancel pending request (keep Host PTY)").on_click(
                        move |_, _, cx| {
                            let _ = cancel_owner.update(cx, |view, cx| {
                                if cancel_target
                                    .still_valid(view.workspace_generation, &view.mixed_panes)
                                    && view.terminal_create_pane.is_some()
                                {
                                    view.cancel_host_terminal_confirmation(cx);
                                }
                            });
                        },
                    ),
                );
            }
            menu = menu.label("Attach running Host session");
            if running.is_empty() {
                menu = menu.label("No running Host sessions");
            }
            for (expected, title) in running.iter().cloned() {
                let owner = owner.clone();
                let attach_target = target.clone();
                menu = menu.item(PopupMenuItem::new(title).on_click(move |_, window, cx| {
                    let _ = owner.update(cx, |view, cx| {
                        // Recheck handle + Host workspace + tab ID and pane
                        // generation at click time. Never follow reused IDs.
                        if let Some(identity) =
                            view.attach_live_terminal_to_pane(&expected, &attach_target, cx)
                        {
                            view.focus_mixed_tab(&identity, window, cx);
                        }
                    });
                }));
            }
            menu.scrollable(true).max_h(px(320.0))
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
        move |mut menu, _, _| {
            let file_owner = owner.clone();
            let pane = if active_in_secondary {
                PaneSlot::Secondary
            } else {
                PaneSlot::Primary
            };
            menu = menu.item(PopupMenuItem::new("New File in this pane").on_click(
                move |_, _, cx| {
                    let _ = file_owner.update(cx, |view, cx| view.new_editor_in_pane(pane, cx));
                },
            ));
            menu = menu.separator();
            if let Some(WorkbenchTabIdentity::Host(target)) = active.clone() {
                menu = menu.label("Terminal");
                let refresh_owner = owner.clone();
                let refresh_target = target.clone();
                menu = menu.item(
                    PopupMenuItem::new("Refresh output").on_click(move |_, _, cx| {
                        let _ = refresh_owner.update(cx, |view, cx| {
                            if view.kit_menu_host_is_active(&refresh_target) {
                                view.refresh_terminal_output(cx);
                            }
                        });
                    }),
                );
                let copy_owner = owner.clone();
                let copy_target = target.clone();
                menu = menu.item(PopupMenuItem::new("Copy visible terminal").on_click(
                    move |_, _, cx| {
                        let _ = copy_owner.update(cx, |view, cx| {
                            if view.kit_menu_host_is_active(&copy_target) {
                                view.copy_primary_terminal_text(false, cx);
                            }
                        });
                    },
                ));
                let selected_owner = owner.clone();
                let selected_target = target.clone();
                menu = menu.item(PopupMenuItem::new("Copy selected rows").on_click(
                    move |_, _, cx| {
                        let _ = selected_owner.update(cx, |view, cx| {
                            if view.kit_menu_host_is_active(&selected_target) {
                                view.copy_primary_terminal_text(true, cx);
                            }
                        });
                    },
                ));
                let mode_owner = owner.clone();
                let mode_target = target.clone();
                menu = menu.item(PopupMenuItem::new("Toggle VT screen / text log").on_click(
                    move |_, _, cx| {
                        let _ = mode_owner.update(cx, |view, cx| {
                            if view.kit_menu_host_is_active(&mode_target) {
                                view.terminals.toggle_view_mode();
                                view.save_terminal_layout();
                                cx.notify();
                            }
                        });
                    },
                ));
                for (label, cols, rows) in
                    [("Resize: 80 × 24", 80, 24), ("Resize: 120 × 36", 120, 36)]
                {
                    let size_owner = owner.clone();
                    let size_target = target.clone();
                    menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                        let _ = size_owner.update(cx, |view, cx| {
                            if view.kit_menu_host_is_active(&size_target) {
                                view.resize_terminal(cols, rows, cx);
                            }
                        });
                    }));
                }
                let split_owner = owner.clone();
                let split_target = target.clone();
                menu = menu.item(PopupMenuItem::new("Toggle Terminal split").on_click(
                    move |_, _, cx| {
                        let _ = split_owner.update(cx, |view, cx| {
                            if view.kit_menu_host_is_active(&split_target) {
                                view.toggle_terminal_split(cx);
                            }
                        });
                    },
                ));
                let direction_owner = owner.clone();
                let direction_target = target.clone();
                menu = menu.item(
                    PopupMenuItem::new("Switch Terminal split direction").on_click(
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
                menu = menu.item(PopupMenuItem::new(label).on_click(move |_, window, cx| {
                    let _ = split_owner.update(cx, |view, cx| {
                        if view.mixed_tab_is_current(&active, cx) {
                            view.mixed_tab_context = Some(active.clone());
                            view.apply_mixed_tab_context(action, window, cx);
                        }
                    });
                }));
            }
            if has_secondary {
                let axis_owner = owner.clone();
                menu = menu.item(PopupMenuItem::new("Switch split direction").on_click(
                    move |_, _, cx| {
                        let _ = axis_owner.update(cx, |view, cx| {
                            view.mixed_panes.toggle_axis();
                            view.save_mixed_pane_layout(cx);
                            cx.notify();
                        });
                    },
                ));
                let owner = owner.clone();
                menu = menu.item(PopupMenuItem::new("Close split pane").on_click(
                    move |_, _, cx| {
                        let _ = owner.update(cx, |view, cx| view.unsplit_mixed_panes(cx));
                    },
                ));
            }
            if active.is_none() && !has_secondary {
                menu = menu.label("Open a tab to split");
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
        move |mut menu, _, _| {
            for action in Self::explorer_actions_for_path(path.as_deref()) {
                let owner = owner.clone();
                let path = path.clone();
                menu = menu.item(PopupMenuItem::new(action.label()).on_click(
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
                self.model
                    .set_status("Explorer item changed; refresh the folder");
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
        move |mut menu, _, _| {
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
                menu = menu.item(PopupMenuItem::new(label).on_click(move |_, window, cx| {
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
                            view.model.set_status("Tab changed; action cancelled");
                            cx.notify();
                        }
                    });
                }));
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

    fn native_branch_choices(&self) -> Vec<String> {
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
