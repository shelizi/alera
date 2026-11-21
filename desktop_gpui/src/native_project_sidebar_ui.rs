//! Project/Worktree Classic RuntimeStore context actions for the integrated
//! GPUI Kit shell. All settings UI stays owned by native_settings_ui.rs.
use super::*;
use alera_gpui_desktop::runtime_catalog::{RuntimeCatalog, RuntimeWorkspace};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SidebarOrderKind {
    Project,
    Workspace,
}

#[derive(Clone)]
pub(super) struct SidebarOrderDrag {
    pub kind: SidebarOrderKind,
    pub id: String,
    pub generation: u64,
    pub hold: Arc<Mutex<SidebarReorderHoldGate>>,
}

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
    DeleteTag {
        id: String,
    },
    DeleteSection {
        id: String,
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
    pub(super) fn drop_sidebar_order(
        &mut self,
        drag: &SidebarOrderDrag,
        target_kind: SidebarOrderKind,
        target_id: Option<String>,
        visible_ids: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(target_id) = target_id else { return };
        let source_key = format!(
            "{}:{}",
            if drag.kind == SidebarOrderKind::Project {
                "project"
            } else {
                "workspace"
            },
            drag.id
        );
        // A stale drag (or a change made in Classic) must never enable manual
        // sorting behind the user's back. Host CLI validates again at save.
        let manual = self
            .sidebar_order_prefs
            .as_ref()
            .is_some_and(|prefs| match target_kind {
                SidebarOrderKind::Project => prefs.project_is_manual(),
                SidebarOrderKind::Workspace => prefs.workspace_is_manual(),
            });
        if !manual {
            self.model
                .set_status("Select Manual sorting to reorder Projects or Worktrees");
            cx.notify();
            return;
        }
        let hold_completed = drag
            .hold
            .lock()
            .map(|mut hold| hold.consume_drop(&source_key))
            .unwrap_or(false);
        if !hold_completed {
            self.model.set_status(tr(
                self.settings_menu_locale,
                "Hold the handle for 0.5s to reorder",
            ));
            cx.notify();
            return;
        }
        if drag.generation != self.workspace_generation
            || drag.kind != target_kind
            || drag.id == target_id
            || self.sidebar_order_busy
            || !self.sidebar_search_input.read(cx).query().trim().is_empty()
        {
            return;
        }
        let Some(catalog) = self.runtime_catalog.as_ref() else {
            self.model
                .set_status("Classic Runtime Host catalog unavailable; sorting disabled");
            cx.notify();
            return;
        };
        let valid = match target_kind {
            SidebarOrderKind::Project => {
                catalog.projects.iter().any(|project| project.id == drag.id)
                    && catalog
                        .projects
                        .iter()
                        .any(|project| project.id == target_id)
            }
            SidebarOrderKind::Workspace => {
                let source = catalog
                    .workspaces
                    .iter()
                    .find(|workspace| workspace.id == drag.id);
                let target = catalog
                    .workspaces
                    .iter()
                    .find(|workspace| workspace.id == target_id);
                match (source, target) {
                    (Some(source), Some(target)) => {
                        source.project_id == target.project_id
                            && source.parent_workspace_id == target.parent_workspace_id
                            && source.section_id == target.section_id
                            && source.is_pinned == target.is_pinned
                            && source.is_archived == target.is_archived
                    }
                    _ => false,
                }
            }
        };
        if !valid {
            self.model
                .set_status("Reorder only within the same Project and Worktree group");
            cx.notify();
            return;
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.model
                .set_status("Classic Runtime Host CLI unavailable");
            cx.notify();
            return;
        };
        let kind = match target_kind {
            SidebarOrderKind::Project => "project",
            SidebarOrderKind::Workspace => "workspace",
        };
        let source = drag.id.clone();
        let generation = self.workspace_generation;
        self.sidebar_order_busy = true;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    bridge.move_sidebar_order(kind, &source, &target_id, &visible_ids)
                })
                .await;
            view.update(cx, |view, cx| {
                view.sidebar_order_busy = false;
                if view.workspace_generation != generation {
                    cx.notify();
                    return;
                }
                match result {
                    Ok(prefs) => {
                        view.sidebar_order_prefs = Some(prefs);
                        order_project_groups(
                            &mut view.project_tree_groups,
                            view.runtime_catalog.as_ref(),
                            view.sidebar_order_prefs.as_ref(),
                        );
                        view.model
                            .set_status("Project / Worktree manual order saved to shared Host");
                        view.refresh_project_tree(cx);
                    }
                    Err(error) => {
                        view.model
                            .set_status(format!("Sidebar order not saved: {error}"));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

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
            SidebarWorkspaceAction::DeleteTag { id } => Some(format!("tag:{id}")),
            SidebarWorkspaceAction::DeleteSection { id } => Some(format!("section:{id}")),
            _ => None,
        };
        if let Some(key) = &key {
            if self.sidebar_pending_action.as_deref() != Some(key) {
                self.sidebar_pending_action = Some(key.clone());
                self.model.set_status(if key.starts_with("sleep:") {
                    "Sleep closes Host sessions and tabs. Right-click and confirm Sleep again."
                } else if key.starts_with("tag:") {
                    "Deleting this shared tag affects all assigned Workspaces. Right-click to confirm."
                } else if key.starts_with("section:") {
                    "Deleting this section moves its members to Others. Right-click to confirm."
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
                        SidebarWorkspaceAction::DeleteTag { id } => bridge.remove_global_tag(&id),
                        SidebarWorkspaceAction::DeleteSection { id } => {
                            bridge.remove_workspace_section(&id)
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

    fn begin_sidebar_create(&mut self, kind: &str, workspace_id: String, cx: &mut Context<Self>) {
        if !matches!(kind, "tag" | "section") {
            return;
        }
        self.sidebar_create_target = Some((kind.to_owned(), workspace_id));
        self.sidebar_create_input =
            Some(cx.new(|cx| QuickOpenInput::with_placeholder("Name…", cx)));
        self.show_project_management = true;
        cx.notify();
    }

    pub(super) fn submit_sidebar_create(&mut self, cx: &mut Context<Self>) {
        let (Some((kind, workspace_id)), Some(input)) = (
            self.sidebar_create_target.clone(),
            self.sidebar_create_input.as_ref(),
        ) else {
            return;
        };
        let name = input.read(cx).query().trim().to_owned();
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.model
                .set_status("Classic Host unavailable; catalog unchanged");
            cx.notify();
            return;
        };
        if self.worktree_busy {
            return;
        }
        self.worktree_busy = true;
        let generation = self.workspace_generation;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    match kind.as_str() {
                        "tag" => {
                            let tag_id = bridge.create_workspace_tag(&name)?;
                            bridge.set_workspace_tag(&workspace_id, &tag_id, true)
                        }
                        "section" => bridge.create_workspace_section(&workspace_id, &name),
                        _ => Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "Invalid catalog action",
                        )),
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
                        view.sidebar_create_target = None;
                        view.sidebar_create_input = None;
                        view.model
                            .set_status("Classic catalog entry created and assigned");
                        view.refresh_project_tree(cx);
                    }
                    Err(error) => view
                        .model
                        .set_status(format!("Classic catalog creation refused: {error}")),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn begin_sidebar_rename(&mut self, kind: &str, id: String, cx: &mut Context<Self>) {
        if !matches!(kind, "project" | "workspace") {
            return;
        }
        self.sidebar_rename_target = Some((kind.to_owned(), id));
        self.sidebar_rename_input =
            Some(cx.new(|cx| QuickOpenInput::with_placeholder("New display name…", cx)));
        self.show_project_management = true;
        cx.notify();
    }

    pub(super) fn submit_sidebar_rename(&mut self, cx: &mut Context<Self>) {
        let (Some((kind, id)), Some(input)) = (
            self.sidebar_rename_target.clone(),
            self.sidebar_rename_input.as_ref(),
        ) else {
            return;
        };
        let name = input.read(cx).query().trim().to_owned();
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.model
                .set_status("Classic Host unavailable; name unchanged");
            cx.notify();
            return;
        };
        if self.worktree_busy {
            return;
        }
        self.worktree_busy = true;
        let generation = self.workspace_generation;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move { bridge.rename_catalog_record(&kind, &id, &name) })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation {
                    return;
                }
                view.worktree_busy = false;
                match result {
                    Ok(()) => {
                        view.sidebar_rename_target = None;
                        view.sidebar_rename_input = None;
                        view.model.set_status(
                            "Classic display name updated (Git path and branch unchanged)",
                        );
                        view.refresh_project_tree(cx);
                    }
                    Err(error) => view
                        .model
                        .set_status(format!("Classic rename refused: {error}")),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn select_sidebar_parent(
        &mut self,
        workspace_id: String,
        project_id: String,
        cx: &mut Context<Self>,
    ) {
        self.sidebar_selected_parent = Some((workspace_id, project_id));
        self.model.set_status(
            "Parent selected. Right-click another Workspace in the same project to attach.",
        );
        cx.notify();
    }

    fn change_sidebar_parent(
        &mut self,
        parent_id: String,
        child_id: String,
        link: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.model
                .set_status("Classic Host unavailable; parent unchanged");
            cx.notify();
            return;
        };
        if self.worktree_busy {
            return;
        }
        self.worktree_busy = true;
        let generation = self.workspace_generation;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    bridge.set_workspace_parent(&parent_id, &child_id, link)
                })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation {
                    return;
                }
                view.worktree_busy = false;
                match result {
                    Ok(()) => {
                        view.sidebar_selected_parent = None;
                        view.model
                            .set_status("Classic workspace parent relationship updated");
                        view.refresh_project_tree(cx);
                    }
                    Err(error) => view
                        .model
                        .set_status(format!("Parent relationship rejected: {error}")),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn pin_sidebar_workspace_tree(
        &mut self,
        root_id: String,
        pinned: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(catalog) = self.runtime_catalog.as_ref() else {
            self.model
                .set_status("Classic workspace catalog unavailable; tree not modified");
            cx.notify();
            return;
        };
        let ids = alera_gpui_desktop::runtime_catalog::workspace_family_ids(catalog, &root_id)
            .into_iter()
            .filter(|id| {
                catalog
                    .workspaces
                    .iter()
                    .any(|workspace| workspace.id == *id && workspace.is_pinned != pinned)
            })
            .collect::<Vec<_>>();
        if ids.is_empty() {
            self.model
                .set_status("Workspace tree already has requested pin state");
            cx.notify();
            return;
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.model
                .set_status("Classic Host unavailable; workspace tree unchanged");
            cx.notify();
            return;
        };
        if self.worktree_busy {
            return;
        }
        self.worktree_busy = true;
        let generation = self.workspace_generation;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    let mut updated = 0usize;
                    for id in ids {
                        if let Err(error) = bridge.set_workspace_pinned(&id, pinned) {
                            return Err::<usize, (usize, std::io::Error)>((updated, error));
                        }
                        updated += 1;
                    }
                    Ok::<usize, (usize, std::io::Error)>(updated)
                })
                .await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation {
                    return;
                }
                view.worktree_busy = false;
                match result {
                    Ok(updated) => view.model.set_status(format!(
                        "Workspace tree {}: {updated} updated",
                        if pinned { "pinned" } else { "unpinned" }
                    )),
                    Err((updated, error)) => view.model.set_status(format!(
                        "Workspace tree partially updated ({updated} saved): {error}"
                    )),
                }
                view.refresh_project_tree(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn forget_sidebar_project(
        &mut self,
        project_id: String,
        root: PathBuf,
        cx: &mut Context<Self>,
    ) {
        if self.worktree_busy
            || self
                .project_tree_groups
                .iter()
                .any(|group| group.root == root && group.is_active)
        {
            self.model.set_status(
                "Cannot forget the active project or during another Worktree operation",
            );
            cx.notify();
            return;
        }
        let Some(bridge) = self.runtime_bridge.clone() else {
            self.model
                .set_status("Classic Host unavailable; project registration unchanged");
            cx.notify();
            return;
        };
        let prior_preview = self
            .sidebar_project_remove_pending
            .as_ref()
            .filter(|(id, _)| id == &project_id)
            .map(|(_, impact)| impact.clone());
        self.worktree_busy = true;
        let generation = self.workspace_generation;
        cx.spawn(async move |view, cx| {
            let result = cx.background_spawn(async move {
                if let Some(preview) = prior_preview {
                    bridge.remove_project_record(&project_id, &root, &preview)?;
                    Ok::<_, std::io::Error>(Ok(root))
                } else {
                    let impact = bridge.preview_project_removal(&project_id)?;
                    Ok(Err((project_id, impact)))
                }
            }).await;
            view.update(cx, |view, cx| {
                if view.workspace_generation != generation { return; }
                view.worktree_busy = false;
                match result {
                    Ok(Err((id, preview))) => {
                        view.model.set_status(format!(
                            "Project removal impact: {} workspaces, {} tabs, {} live sessions; config override {}. Right-click again to confirm.",
                            preview.workspace_count, preview.tab_count, preview.active_session_count,
                            if preview.has_config_override { "yes" } else { "no" },
                        ));
                        view.sidebar_project_remove_pending = Some((id, preview));
                    }
                    Ok(Ok(root)) => {
                        view.sidebar_project_remove_pending = None;
                        if let Some(store) = &view.session_store {
                            let _ = store.unsave_project(&root);
                            let _ = store.forget_recent_workspace(&root);
                        }
                        view.saved_projects.retain(|path| path != &root);
                        view.recent_workspaces.retain(|path| path != &root);
                        view.model.set_status("Classic project registration removed; Git files/branches retained");
                        view.refresh_project_tree(cx);
                    }
                    Err(error) => {
                        view.sidebar_project_remove_pending = None;
                        view.model.set_status(format!("Project removal refused: {error}"));
                    }
                }
                cx.notify();
            }).ok();
        }).detach();
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
        project_id: Option<String>,
        remove_armed: bool,
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
            if let Some(project_id) = project_id.clone() {
                let rename_owner = owner.clone();
                menu = menu.item(PopupMenuItem::new("Rename project…").on_click(
                    move |_, _, cx| {
                        let _ = rename_owner.update(cx, |view, cx| {
                            view.begin_sidebar_rename("project", project_id.clone(), cx);
                        });
                    },
                ));
            }
            if let Some(project_id) = project_id.clone() {
                let remove_owner = owner.clone();
                let remove_root = root.clone();
                menu = menu.item(
                    PopupMenuItem::new(if remove_armed {
                        "Confirm forget project"
                    } else {
                        "Forget project…"
                    })
                    .on_click(move |_, _, cx| {
                        let _ = remove_owner.update(cx, |view, cx| {
                            view.forget_sidebar_project(
                                project_id.clone(),
                                remove_root.clone(),
                                cx,
                            );
                        });
                    }),
                );
            }
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
        selected_parent: Option<(String, String)>,
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
                for (kind, label) in [
                    ("tag", "New shared tag…"),
                    ("section", "New workspace section…"),
                ] {
                    let owner_create = owner.clone();
                    let workspace_id = id.clone();
                    menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                        let _ = owner_create.update(cx, |view, cx| {
                            view.begin_sidebar_create(kind, workspace_id.clone(), cx);
                        });
                    }));
                }

                let rename_owner = owner.clone();
                let rename_id = id.clone();
                menu = menu.item(PopupMenuItem::new("Rename Workspace…").on_click(
                    move |_, _, cx| {
                        let _ = rename_owner.update(cx, |view, cx| {
                            view.begin_sidebar_rename("workspace", rename_id.clone(), cx);
                        });
                    },
                ));
                let select_parent_owner = owner.clone();
                let select_id = id.clone();
                let select_project = workspace.project_id.clone();
                menu = menu.item(
                    PopupMenuItem::new("Use as parent").on_click(move |_, _, cx| {
                        let _ = select_parent_owner.update(cx, |view, cx| {
                            view.select_sidebar_parent(
                                select_id.clone(),
                                select_project.clone(),
                                cx,
                            );
                        });
                    }),
                );
                if let Some(parent_id) = workspace.parent_workspace_id.clone() {
                    let detach_owner = owner.clone();
                    let child = id.clone();
                    menu = menu.item(PopupMenuItem::new("Detach parent").on_click(
                        move |_, _, cx| {
                            let _ = detach_owner.update(cx, |view, cx| {
                                view.change_sidebar_parent(
                                    parent_id.clone(),
                                    child.clone(),
                                    false,
                                    cx,
                                );
                            });
                        },
                    ));
                } else if let Some((parent_id, project_id)) = selected_parent.clone() {
                    if project_id == workspace.project_id && parent_id != id {
                        let attach_owner = owner.clone();
                        let child = id.clone();
                        menu = menu.item(PopupMenuItem::new("Attach to selected parent").on_click(
                            move |_, _, cx| {
                                let _ = attach_owner.update(cx, |view, cx| {
                                    view.change_sidebar_parent(
                                        parent_id.clone(),
                                        child.clone(),
                                        true,
                                        cx,
                                    );
                                });
                            },
                        ));
                    }
                }
                if catalog.as_ref().is_some_and(|catalog| {
                    alera_gpui_desktop::runtime_catalog::workspace_family_ids(catalog, &id).len()
                        > 1
                }) {
                    for pinned in [true, false] {
                        let tree_owner = owner.clone();
                        let root_id = id.clone();
                        menu = menu.item(
                            PopupMenuItem::new(if pinned {
                                "Pin workspace tree"
                            } else {
                                "Unpin workspace tree"
                            })
                            .on_click(move |_, _, cx| {
                                let _ = tree_owner.update(cx, |view, cx| {
                                    view.pin_sidebar_workspace_tree(root_id.clone(), pinned, cx);
                                });
                            }),
                        );
                    }
                }

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
                    if let Some(tag) = catalog
                        .tags
                        .iter()
                        .find(|tag| workspace.tag_ids.contains(&tag.id))
                    {
                        let tag_id = tag.id.clone();
                        let key = format!("tag:{tag_id}");
                        let confirmed = confirmation.as_deref() == Some(key.as_str());
                        let remove_owner = owner.clone();
                        let action = SidebarWorkspaceAction::DeleteTag { id: tag_id };
                        menu = menu.item(
                            PopupMenuItem::new(if confirmed {
                                "Confirm delete assigned tag globally"
                            } else {
                                "Delete assigned tag globally…"
                            })
                            .on_click(move |_, _, cx| {
                                let action = action.clone();
                                let _ = remove_owner.update(cx, |view, cx| {
                                    view.begin_sidebar_workspace_action(action, cx);
                                });
                            }),
                        );
                    }
                    if let Some(section_id) = workspace.section_id.clone() {
                        let key = format!("section:{section_id}");
                        let confirmed = confirmation.as_deref() == Some(key.as_str());
                        let remove_owner = owner.clone();
                        let action = SidebarWorkspaceAction::DeleteSection { id: section_id };
                        menu = menu.item(
                            PopupMenuItem::new(if confirmed {
                                "Confirm delete current section"
                            } else {
                                "Delete current section…"
                            })
                            .on_click(move |_, _, cx| {
                                let action = action.clone();
                                let _ = remove_owner.update(cx, |view, cx| {
                                    view.begin_sidebar_workspace_action(action, cx);
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
