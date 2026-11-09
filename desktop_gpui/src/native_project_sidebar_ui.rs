//! Project/Worktree Classic RuntimeStore context actions for the integrated
//! GPUI Kit shell. All settings UI stays owned by native_settings_ui.rs.
use super::*;
use alera_gpui_desktop::runtime_catalog::{RuntimeCatalog, RuntimeWorkspace};

#[derive(Clone)]
pub(super) enum SidebarWorkspaceAction {
    Pin {
        id: String,
        pinned: bool,
    },
    Archive {
        id: String,
        archived: bool,
    },
    Tag {
        id: String,
        tag_id: String,
        assign: bool,
    },
    Section {
        id: String,
        section_id: Option<String>,
    },
    Sleep {
        id: String,
    },
    Remove {
        id: String,
        root: PathBuf,
        target: PathBuf,
    },
}

impl Workbench {
    fn begin_sidebar_workspace_action(
        &mut self,
        action: SidebarWorkspaceAction,
        cx: &mut Context<Self>,
    ) {
        if self.worktree_busy {
            self.model
                .set_status("Another Worktree operation is already running");
            cx.notify();
            return;
        }
        if self.runtime_catalog.is_none() {
            self.model
                .set_status("Classic workspace catalog unavailable; operation refused");
            cx.notify();
            return;
        }
        let key = match &action {
            SidebarWorkspaceAction::Sleep { id } => Some(format!("sleep:{id}")),
            SidebarWorkspaceAction::Remove { id, .. } => Some(format!("remove:{id}")),
            _ => None,
        };
        if let Some(key) = &key {
            if self.sidebar_pending_action.as_deref() != Some(key) {
                self.sidebar_pending_action = Some(key.clone());
                self.model.set_status(if key.starts_with("sleep:") {
                    "Sleep closes Host sessions and tabs. Right-click and confirm Sleep again."
                } else {
                    "Remove Worktree keeps the branch but removes its checkout. Right-click and confirm again."
                });
                cx.notify();
                return;
            }
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.model
                .set_status("Classic Alera CLI unavailable; operation refused");
            cx.notify();
            return;
        };
        self.worktree_busy = true;
        self.sidebar_pending_action = None;
        let generation = self.workspace_generation;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    match action {
                        SidebarWorkspaceAction::Pin { id, pinned } => {
                            bridge.set_workspace_pinned(&id, pinned)
                        }
                        SidebarWorkspaceAction::Archive { id, archived } => {
                            bridge.set_workspace_archived(&id, archived)
                        }
                        SidebarWorkspaceAction::Tag { id, tag_id, assign } => {
                            bridge.set_workspace_tag(&id, &tag_id, assign)
                        }
                        SidebarWorkspaceAction::Section { id, section_id } => {
                            bridge.set_workspace_section(&id, section_id.as_deref())
                        }
                        SidebarWorkspaceAction::Sleep { id } => {
                            bridge.sleep_registered_workspace(&id)
                        }
                        SidebarWorkspaceAction::Remove { id, root, target } => {
                            bridge.remove_registered_worktree(&id, &root, &target)
                        }
                    }
                })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation {
                    return;
                }
                view.worktree_busy = false;
                match result {
                    Ok(()) => {
                        view.model
                            .set_status("Classic workspace change saved in RuntimeStore");
                        view.refresh_project_tree(cx);
                        view.refresh_worktrees(cx);
                    }
                    Err(error) => view
                        .model
                        .set_status(format!("Classic workspace change refused: {error}")),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn pin_sidebar_project(&mut self, root: &Path, cx: &mut Context<Self>) {
        let Some(store) = &self.session_store else {
            self.model.set_status("Preview favorites unavailable");
            cx.notify();
            return;
        };
        match store
            .save_project(root)
            .and_then(|_| store.saved_projects())
        {
            Ok(projects) => {
                self.saved_projects = projects;
                self.refresh_project_tree(cx);
            }
            Err(error) => self
                .model
                .set_status(format!("Cannot pin project: {error}")),
        }
        cx.notify();
    }

    fn sidebar_open_folder(&mut self, root: &Path, cx: &mut Context<Self>) {
        let program = if cfg!(windows) {
            "explorer.exe"
        } else if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        match std::process::Command::new(program).arg(root).spawn() {
            Ok(_) => self.model.set_status("Opened project folder"),
            Err(error) => self
                .model
                .set_status(format!("Cannot open folder: {error}")),
        }
        cx.notify();
    }

    fn sidebar_copy_path(&mut self, root: &Path, cx: &mut Context<Self>) {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
            root.to_string_lossy().into_owned(),
        ));
        self.model.set_status("Copied project path");
        cx.notify();
    }

    pub(super) fn sidebar_project_context_menu(
        owner: gpui::WeakEntity<Self>,
        root: PathBuf,
        pinned: bool,
    ) -> impl Fn(
        gpui_kit::component::menu::PopupMenu,
        &mut Window,
        &mut Context<gpui_kit::component::menu::PopupMenu>,
    ) -> gpui_kit::component::menu::PopupMenu
           + 'static {
        move |mut menu, _, _| {
            let target = root.clone();
            let owner_open = owner.clone();
            menu = menu.item(
                PopupMenuItem::new("Open project").on_click(move |_, _, cx| {
                    let _ =
                        owner_open.update(cx, |view, cx| view.switch_workspace(target.clone(), cx));
                }),
            );
            let owner_pin = owner.clone();
            let target = root.clone();
            menu = menu.item(
                PopupMenuItem::new(if pinned {
                    "Unpin project"
                } else {
                    "Pin project"
                })
                .on_click(move |_, _, cx| {
                    let _ = owner_pin.update(cx, |view, cx| {
                        if pinned {
                            view.unpin_project(&target, cx)
                        } else {
                            view.pin_sidebar_project(&target, cx)
                        }
                    });
                }),
            );
            let owner_worktree = owner.clone();
            let worktree_root = root.clone();
            menu = menu.item(
                PopupMenuItem::new("New Worktree…").on_click(move |_, _, cx| {
                    let _ = owner_worktree.update(cx, |view, cx| {
                        view.begin_worktree_for_project(worktree_root.clone(), cx);
                    });
                }),
            );
            let owner_folder = owner.clone();
            let target = root.clone();
            menu = menu.item(PopupMenuItem::new("Open folder").on_click(move |_, _, cx| {
                let _ = owner_folder.update(cx, |view, cx| view.sidebar_open_folder(&target, cx));
            }));
            let owner_copy = owner.clone();
            let target = root.clone();
            menu.item(PopupMenuItem::new("Copy path").on_click(move |_, _, cx| {
                let _ = owner_copy.update(cx, |view, cx| view.sidebar_copy_path(&target, cx));
            }))
        }
    }

    pub(super) fn sidebar_workspace_context_menu(
        owner: gpui::WeakEntity<Self>,
        project_root: PathBuf,
        target: PathBuf,
        metadata: Option<RuntimeWorkspace>,
        catalog: Option<RuntimeCatalog>,
        confirmation: Option<String>,
        is_active: bool,
    ) -> impl Fn(
        gpui_kit::component::menu::PopupMenu,
        &mut Window,
        &mut Context<gpui_kit::component::menu::PopupMenu>,
    ) -> gpui_kit::component::menu::PopupMenu
           + 'static {
        move |mut menu, _, _| {
            let owner_open = owner.clone();
            let root = target.clone();
            menu = menu.item(
                PopupMenuItem::new("Open Workspace").on_click(move |_, _, cx| {
                    let _ =
                        owner_open.update(cx, |view, cx| view.switch_workspace(root.clone(), cx));
                }),
            );
            let owner_folder = owner.clone();
            let root = target.clone();
            menu = menu.item(PopupMenuItem::new("Open folder").on_click(move |_, _, cx| {
                let _ = owner_folder.update(cx, |view, cx| view.sidebar_open_folder(&root, cx));
            }));
            let owner_copy = owner.clone();
            let root = target.clone();
            menu = menu.item(PopupMenuItem::new("Copy path").on_click(move |_, _, cx| {
                let _ = owner_copy.update(cx, |view, cx| view.sidebar_copy_path(&root, cx));
            }));
            if let Some(workspace) = metadata.clone() {
                let id = workspace.id.clone();
                for (label, action) in [
                    (
                        if workspace.is_pinned {
                            "Unpin Workspace"
                        } else {
                            "Pin Workspace"
                        },
                        SidebarWorkspaceAction::Pin {
                            id: id.clone(),
                            pinned: !workspace.is_pinned,
                        },
                    ),
                    (
                        if workspace.is_archived {
                            "Restore Workspace"
                        } else {
                            "Archive Workspace"
                        },
                        SidebarWorkspaceAction::Archive {
                            id: id.clone(),
                            archived: !workspace.is_archived,
                        },
                    ),
                ] {
                    let call_owner = owner.clone();
                    menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                        let action = action.clone();
                        let _ = call_owner.update(cx, |view, cx| {
                            view.begin_sidebar_workspace_action(action, cx)
                        });
                    }));
                }
                if let Some(catalog) = catalog.as_ref() {
                    for tag in catalog.tags.iter().take(12) {
                        let tag_id = tag.id.clone();
                        let selected = workspace.tag_ids.contains(&tag_id);
                        let action = SidebarWorkspaceAction::Tag {
                            id: id.clone(),
                            tag_id,
                            assign: !selected,
                        };
                        let call_owner = owner.clone();
                        menu = menu.item(
                            PopupMenuItem::new(format!(
                                "{} {}",
                                if selected { "✓" } else { "+" },
                                tag.name
                            ))
                            .on_click(move |_, _, cx| {
                                let action = action.clone();
                                let _ = call_owner.update(cx, |view, cx| {
                                    view.begin_sidebar_workspace_action(action, cx)
                                });
                            }),
                        );
                    }
                    if workspace.section_id.is_some() {
                        let action = SidebarWorkspaceAction::Section {
                            id: id.clone(),
                            section_id: None,
                        };
                        let call_owner = owner.clone();
                        menu = menu.item(PopupMenuItem::new("Move to Others").on_click(
                            move |_, _, cx| {
                                let action = action.clone();
                                let _ = call_owner.update(cx, |view, cx| {
                                    view.begin_sidebar_workspace_action(action, cx)
                                });
                            },
                        ));
                    }
                    for section in catalog
                        .sections
                        .iter()
                        .filter(|section| Some(&section.id) != workspace.section_id.as_ref())
                        .take(10)
                    {
                        let action = SidebarWorkspaceAction::Section {
                            id: id.clone(),
                            section_id: Some(section.id.clone()),
                        };
                        let call_owner = owner.clone();
                        menu = menu.item(
                            PopupMenuItem::new(format!("Section: {}", section.name)).on_click(
                                move |_, _, cx| {
                                    let action = action.clone();
                                    let _ = call_owner.update(cx, |view, cx| {
                                        view.begin_sidebar_workspace_action(action, cx)
                                    });
                                },
                            ),
                        );
                    }
                }
                if !is_active {
                    let key = format!("sleep:{id}");
                    let label = if confirmation.as_deref() == Some(&key) {
                        "Confirm Sleep Workspace"
                    } else {
                        "Sleep Workspace…"
                    };
                    let action = SidebarWorkspaceAction::Sleep { id: id.clone() };
                    let call_owner = owner.clone();
                    menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                        let action = action.clone();
                        let _ = call_owner.update(cx, |view, cx| {
                            view.begin_sidebar_workspace_action(action, cx)
                        });
                    }));
                    if target != project_root {
                        let key = format!("remove:{id}");
                        let label = if confirmation.as_deref() == Some(&key) {
                            "Confirm Remove Worktree"
                        } else {
                            "Remove Worktree…"
                        };
                        let action = SidebarWorkspaceAction::Remove {
                            id,
                            root: project_root.clone(),
                            target: target.clone(),
                        };
                        let call_owner = owner.clone();
                        menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                            let action = action.clone();
                            let _ = call_owner.update(cx, |view, cx| {
                                view.begin_sidebar_workspace_action(action, cx)
                            });
                        }));
                    }
                }
            }
            menu.scrollable(true).max_h(px(360.0))
        }
    }
}
