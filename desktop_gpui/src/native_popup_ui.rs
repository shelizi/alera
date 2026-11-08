//! Shared anchored native popup menus. GPUI provides anchored/deferred and
//! mouse/key event plumbing, while Alera supplies reusable tokenized rows.
//! No menu item is a permanently visible surrogate toolbar action.

use super::*;
use native_components::{menu_item, menu_separator, menu_surface, popup_at};

impl Workbench {
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

    pub(super) fn show_native_popup(
        &mut self,
        kind: NativePopupKind,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        if self.settings_section.is_some() {
            return;
        }
        self.native_popup.show(kind, x, y);
        self.native_popup_needs_focus = true;
        if kind != NativePopupKind::ExplorerContext {
            self.explorer_context_menu = None;
        }
        if kind != NativePopupKind::MixedTabContext {
            self.mixed_tab_context = None;
        }
        if kind != NativePopupKind::TerminalTabContext {
            self.terminal_tab_context = None;
        }
        cx.notify();
    }

    pub(super) fn dismiss_native_popup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let previous = self.native_popup.active().map(|popup| popup.kind);
        self.native_popup.dismiss();
        self.native_popup_needs_focus = false;
        self.explorer_context_menu = None;
        self.mixed_tab_context = None;
        self.terminal_tab_context = None;
        if self.settings_section.is_some() {
            window.focus(&self.settings_modal_focus, cx);
        } else if matches!(
            previous,
            Some(NativePopupKind::ExplorerContext | NativePopupKind::ExplorerVisibility)
        ) {
            window.focus(&self.explorer_keyboard_focus, cx);
        } else if matches!(
            previous,
            Some(NativePopupKind::GitMore | NativePopupKind::GitBranch)
        ) {
            window.focus(&self.git_branch_input.read(cx).focus_handle(cx), cx);
        } else if self.center_view == CenterView::Editor {
            if let Some(editor) = self.focused_editor() {
                window.focus(&editor.read(cx).focus_handle(cx), cx);
            } else {
                window.focus(&self.terminal_keyboard_focus, cx);
            }
        } else {
            window.focus(&self.terminal_keyboard_focus, cx);
        }
        cx.notify();
    }

    fn explorer_popup_actions(&self) -> Vec<ExplorerMenuAction> {
        let Some(menu) = &self.explorer_context_menu else {
            return Vec::new();
        };
        if let Some(path) = menu.path.as_ref() {
            let mut actions = vec![
                ExplorerMenuAction::File(ExplorerAction::NewFile),
                ExplorerMenuAction::File(ExplorerAction::NewFolder),
                ExplorerMenuAction::Clipboard(ExplorerClipboardMode::Copy),
                ExplorerMenuAction::Clipboard(ExplorerClipboardMode::Cut),
                ExplorerMenuAction::Paste,
                ExplorerMenuAction::CopyAbsolutePath,
                ExplorerMenuAction::CopyRelativePath,
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

    fn choose_native_branch(
        &mut self,
        branch: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self
            .native_branch_choices()
            .iter()
            .any(|name| name == &branch)
        {
            self.dismiss_native_popup(window, cx);
            self.model
                .set_status("Branch list changed; refresh before selecting");
            cx.notify();
            return;
        }
        self.git_merge_armed = None;
        self.git_branch_input
            .update(cx, |input, cx| input.set_query(&branch, cx));
        self.dismiss_native_popup(window, cx);
        self.right_sidebar_view = RightSidebarView::Git;
        self.model.set_status(format!(
            "Selected {branch}. Use Compare or Switch to proceed."
        ));
        cx.notify();
    }

    fn choose_native_host_session(
        &mut self,
        handle: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let still_running = self
            .terminals
            .sessions
            .iter()
            .any(|s| s.running && s.handle == handle);
        self.dismiss_native_popup(window, cx);
        if still_running {
            self.select_terminal(handle, cx);
        } else {
            self.model
                .set_status("Host session changed or stopped; selection cancelled");
            cx.notify();
        }
    }

    fn popup_item_count(&self, kind: NativePopupKind) -> usize {
        match kind {
            NativePopupKind::ExplorerContext => self.explorer_popup_actions().len(),
            NativePopupKind::ExplorerVisibility => {
                if cfg!(windows) {
                    2
                } else {
                    1
                }
            }
            NativePopupKind::GitMore => 3 + usize::from(self.git_conflict_preview.is_some()),
            NativePopupKind::GitBranch => self.native_branch_choices().len(),
            NativePopupKind::TerminalAttach => {
                self.terminals.sessions.iter().filter(|s| s.running).count()
            }
            NativePopupKind::TerminalSize => 2,
            NativePopupKind::MixedTabContext => 7,
            NativePopupKind::TerminalTabContext => 7,
        }
    }

    fn activate_native_popup_index(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(popup) = self.native_popup.active() else {
            return;
        };
        match popup.kind {
            NativePopupKind::ExplorerContext => {
                if let Some(action) = self.explorer_popup_actions().get(index).copied() {
                    self.native_popup.dismiss();
                    self.run_explorer_menu_action(action, window, cx);
                }
            }
            NativePopupKind::ExplorerVisibility => {
                let mut next = self.explorer_visibility;
                match index {
                    0 => next.show_ignored = !next.show_ignored,
                    1 if cfg!(windows) => next.show_hidden = !next.show_hidden,
                    _ => return,
                }
                self.dismiss_native_popup(window, cx);
                self.update_explorer_visibility(next, cx);
            }
            NativePopupKind::GitMore => {
                match index {
                    0 | 1 => {
                        let action = if index == 0 {
                            ConfirmGitAction::StashSave
                        } else {
                            ConfirmGitAction::StashApply
                        };
                        let already_armed = self.git_sensitive_armed == Some(action);
                        self.confirm_sensitive_git_action(action, cx);
                        // First click arms, second click executes. Keep the
                        // menu visible for that explicit confirmation.
                        if already_armed {
                            self.dismiss_native_popup(window, cx);
                        }
                    }
                    2 => {
                        self.dismiss_native_popup(window, cx);
                        self.preview_git_merge_conflicts(cx);
                    }
                    3 if self.git_conflict_preview.is_some() => {
                        self.dismiss_native_popup(window, cx);
                        self.git_conflict_preview = None;
                        self.git_conflict_choices.clear();
                        self.git_conflict_selected = None;
                        self.git_conflict_confirm_armed = false;
                        self.git_action_status =
                            "Merge conflict review cancelled; no files changed".into();
                        cx.notify();
                    }
                    _ => {}
                }
            }
            NativePopupKind::GitBranch => {
                if let Some(branch) = self.native_branch_choices().get(index).cloned() {
                    self.choose_native_branch(branch, window, cx);
                }
            }
            NativePopupKind::TerminalAttach => {
                let handle = self
                    .terminals
                    .sessions
                    .iter()
                    .filter(|s| s.running)
                    .nth(index)
                    .map(|s| s.handle.clone());
                if let Some(handle) = handle {
                    self.choose_native_host_session(handle, window, cx);
                }
            }
            NativePopupKind::TerminalSize => {
                let dimensions = match index {
                    0 => (80, 24),
                    1 => (120, 36),
                    _ => return,
                };
                self.dismiss_native_popup(window, cx);
                self.resize_terminal(dimensions.0, dimensions.1, cx);
            }
            NativePopupKind::MixedTabContext => {
                let secondary = self
                    .mixed_tab_context
                    .as_ref()
                    .is_some_and(|id| self.mixed_panes.tabs_in(PaneSlot::Secondary).contains(id));
                let action = match index {
                    0 => MixedContextAction::Focus,
                    1 => MixedContextAction::Close,
                    2 => MixedContextAction::CloseOthers,
                    3 => MixedContextAction::CloseToRight,
                    4 => MixedContextAction::MoveLeft,
                    5 => MixedContextAction::MoveRight,
                    6 if secondary => MixedContextAction::FirstPane,
                    6 => MixedContextAction::SecondPane,
                    _ => return,
                };
                self.native_popup.dismiss();
                self.apply_mixed_tab_context(action, window, cx);
            }
            NativePopupKind::TerminalTabContext => {
                let action = match index {
                    0 => TerminalContextAction::Focus,
                    1 => TerminalContextAction::Close,
                    2 => TerminalContextAction::CloseOthers,
                    3 => TerminalContextAction::CloseToRight,
                    4 => TerminalContextAction::SplitToSecondary,
                    5 => TerminalContextAction::MoveLeft,
                    6 => TerminalContextAction::MoveRight,
                    _ => return,
                };
                self.native_popup.dismiss();
                self.apply_terminal_context(action, cx);
            }
        }
    }

    pub(super) fn render_native_popup(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let popup = self.native_popup.active()?;
        if self.settings_section.is_some() {
            return None;
        }
        let (width, title, rows): (f32, String, Vec<gpui::AnyElement>) = match popup.kind {
            NativePopupKind::ExplorerContext => {
                let menu = self.explorer_context_menu.as_ref()?;
                let title = menu
                    .path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Current folder".into());
                let rows = self
                    .explorer_popup_actions()
                    .into_iter()
                    .enumerate()
                    .map(|(index, action)| {
                        let destructive =
                            action == ExplorerMenuAction::File(ExplorerAction::Delete);
                        menu_item(
                            ("explorer-popup-action", index),
                            action.label(),
                            false,
                            destructive,
                        )
                        .when(popup.selected == index, |row| row.bg(rgb(ACTIVE)))
                        .on_click(cx.listener(move |view, _, window, cx| {
                            view.activate_native_popup_index(index, window, cx);
                            cx.stop_propagation();
                        }))
                        .into_any_element()
                    })
                    .collect();
                let mut rows: Vec<gpui::AnyElement> = rows;
                if rows.len() > 2 {
                    rows.insert(2, menu_separator().into_any_element());
                }
                if rows.len() > 6 {
                    rows.insert(6, menu_separator().into_any_element());
                }
                (244.0, title, rows)
            }
            NativePopupKind::ExplorerVisibility => {
                let mut choices =
                    vec![("Show ignored files", self.explorer_visibility.show_ignored)];
                if cfg!(windows) {
                    choices.push(("Show hidden items", self.explorer_visibility.show_hidden));
                }
                let rows = choices
                    .into_iter()
                    .enumerate()
                    .map(|(index, (label, selected))| {
                        menu_item(
                            ("explorer-visibility-choice", index),
                            label,
                            selected,
                            false,
                        )
                        .when(popup.selected == index, |row| row.bg(rgb(ACTIVE)))
                        .on_click(cx.listener(move |view, _, window, cx| {
                            view.activate_native_popup_index(index, window, cx);
                            cx.stop_propagation();
                        }))
                        .into_any_element()
                    })
                    .collect();
                (226.0, "Explorer display".into(), rows)
            }
            NativePopupKind::GitMore => {
                let armed = self.git_sensitive_armed;
                let mut labels = vec![
                    (
                        if armed == Some(ConfirmGitAction::StashSave) {
                            "Confirm stash tracked changes"
                        } else {
                            "Stash tracked changes"
                        },
                        armed == Some(ConfirmGitAction::StashSave),
                    ),
                    (
                        if armed == Some(ConfirmGitAction::StashApply) {
                            "Confirm apply latest stash"
                        } else {
                            "Apply latest stash"
                        },
                        armed == Some(ConfirmGitAction::StashApply),
                    ),
                    ("Inspect merge conflicts", false),
                ];
                if self.git_conflict_preview.is_some() {
                    labels.push(("Cancel conflict review", false));
                }
                let rows = labels
                    .into_iter()
                    .enumerate()
                    .map(|(index, (label, checked))| {
                        menu_item(("git-more-choice", index), label, checked, index == 3)
                            .when(popup.selected == index, |row| row.bg(rgb(ACTIVE)))
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.activate_native_popup_index(index, window, cx);
                                cx.stop_propagation();
                            }))
                            .into_any_element()
                    })
                    .collect();
                let mut rows: Vec<gpui::AnyElement> = rows;
                rows.insert(2, menu_separator().into_any_element());
                (266.0, "More Git actions".into(), rows)
            }
            NativePopupKind::GitBranch => {
                let choices = self.native_branch_choices();
                let current = self.git_history.as_ref().map(|h| h.current.as_str());
                let selected_input = self.git_branch_input.read(cx).query().to_owned();
                let rows = choices
                    .into_iter()
                    .enumerate()
                    .map(|(index, name)| {
                        let selected = current == Some(name.as_str()) || name == selected_input;
                        menu_item(("git-branch-choice", index), name.clone(), selected, false)
                            .when(popup.selected == index, |row| row.bg(rgb(ACTIVE)))
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.choose_native_branch(name.clone(), window, cx);
                                cx.stop_propagation();
                            }))
                            .into_any_element()
                    })
                    .collect();
                (
                    290.0,
                    if self.git_history_loading {
                        "Loading local branches…".into()
                    } else {
                        "Choose local branch".into()
                    },
                    rows,
                )
            }
            NativePopupKind::TerminalAttach => {
                let rows = self
                    .terminals
                    .sessions
                    .iter()
                    .filter(|session| session.running)
                    .enumerate()
                    .map(|(index, session)| {
                        let label = format!("{} · {}", session.label(), session.workspace_id);
                        let handle = session.handle.clone();
                        menu_item(("terminal-session-choice", index), label, false, false)
                            .when(popup.selected == index, |row| row.bg(rgb(ACTIVE)))
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.choose_native_host_session(handle.clone(), window, cx);
                                cx.stop_propagation();
                            }))
                            .into_any_element()
                    })
                    .collect();
                (338.0, "Attach existing Host session".into(), rows)
            }
            NativePopupKind::TerminalSize => {
                let rows = ["80 × 24", "120 × 36"]
                    .into_iter()
                    .enumerate()
                    .map(|(index, label)| {
                        menu_item(("terminal-size-choice", index), label, false, false)
                            .when(popup.selected == index, |row| row.bg(rgb(ACTIVE)))
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.activate_native_popup_index(index, window, cx);
                                cx.stop_propagation();
                            }))
                            .into_any_element()
                    })
                    .collect();
                (180.0, "Terminal size".into(), rows)
            }
            NativePopupKind::MixedTabContext | NativePopupKind::TerminalTabContext => {
                let mixed = popup.kind == NativePopupKind::MixedTabContext;
                if mixed && self.mixed_tab_context.is_none()
                    || !mixed && self.terminal_tab_context.is_none()
                {
                    return None;
                }
                let secondary = mixed
                    && self.mixed_tab_context.as_ref().is_some_and(|id| {
                        self.mixed_panes.tabs_in(PaneSlot::Secondary).contains(id)
                    });
                let labels = [
                    "Focus",
                    "Close",
                    "Close others",
                    "Close to right",
                    if mixed {
                        "Move left"
                    } else {
                        "Move to second pane"
                    },
                    if mixed { "Move right" } else { "Move left" },
                    if mixed {
                        if secondary {
                            "Move to first pane"
                        } else {
                            "Move to second pane"
                        }
                    } else {
                        "Move right"
                    },
                ];
                let rows = labels
                    .into_iter()
                    .enumerate()
                    .map(|(index, label)| {
                        menu_item(("tab-popup-action", index), label, false, index == 1)
                            .when(popup.selected == index, |row| row.bg(rgb(ACTIVE)))
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.activate_native_popup_index(index, window, cx);
                                cx.stop_propagation();
                            }))
                            .into_any_element()
                    })
                    .collect();
                (
                    222.0,
                    if mixed {
                        "Tab options".into()
                    } else {
                        "Terminal options".into()
                    },
                    rows,
                )
            }
        };
        let menu_rows = if rows.is_empty() {
            div()
                .px_2()
                .py_3()
                .text_xs()
                .text_color(rgb(MUTED))
                .child("No available options")
                .into_any_element()
        } else {
            div().children(rows).into_any_element()
        };
        let content = menu_surface(width, Some(title), menu_rows);
        Some(
            div()
                .id("alera-popup-backdrop")
                .absolute()
                .inset_0()
                .key_context("AleraNativePopup")
                .track_focus(&self.native_popup_focus)
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(|view, _, window, cx| {
                        view.dismiss_native_popup(window, cx);
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_down(
                    gpui::MouseButton::Right,
                    cx.listener(|view, _, window, cx| {
                        view.dismiss_native_popup(window, cx);
                        cx.stop_propagation();
                    }),
                )
                .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                    let key = event.keystroke.key.as_str();
                    if key.eq_ignore_ascii_case("escape") {
                        view.dismiss_native_popup(window, cx);
                    } else if key.eq_ignore_ascii_case("down")
                        || key.eq_ignore_ascii_case("arrowdown")
                    {
                        if let Some(p) = view.native_popup.active() {
                            let count = view.popup_item_count(p.kind);
                            view.native_popup.navigate(1, count);
                            cx.notify();
                        }
                    } else if key.eq_ignore_ascii_case("up") || key.eq_ignore_ascii_case("arrowup")
                    {
                        if let Some(p) = view.native_popup.active() {
                            let count = view.popup_item_count(p.kind);
                            view.native_popup.navigate(-1, count);
                            cx.notify();
                        }
                    } else if key.eq_ignore_ascii_case("enter")
                        || key.eq_ignore_ascii_case("return")
                    {
                        if let Some(p) = view.native_popup.active() {
                            view.activate_native_popup_index(p.selected, window, cx);
                        }
                    }
                    // A popover is a focused interaction: do not send typing
                    // into an unfocused shell Terminal / Editor.
                    cx.stop_propagation();
                }))
                .child(popup_at(popup.x, popup.y, content))
                .into_any_element(),
        )
    }
}
