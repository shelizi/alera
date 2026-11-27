//! Project/Worktree Classic RuntimeStore context actions for the integrated
//! GPUI Kit shell. All settings UI stays owned by native_settings_ui.rs.
use super::*;
use alera_gpui_desktop::runtime_catalog::{RuntimeCatalog, RuntimeWorkspace};
use gpui_kit::component::menu::PopupMenu;
use native_project_sidebar_modal_ui::SidebarOptionsTarget;

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
                        view.accept_sidebar_order_prefs(prefs);
                        order_project_groups(
                            &mut view.project_tree_groups,
                            view.runtime_catalog.as_deref(),
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

    pub(super) fn begin_sidebar_workspace_action(
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

    pub(super) fn begin_sidebar_create(&mut self, kind: &str, workspace_id: String, cx: &mut Context<Self>) {
        if !matches!(kind, "tag" | "section") {
            return;
        }
        self.sidebar_create_target = Some((kind.to_owned(), workspace_id));
        self.sidebar_create_input =
            Some(cx.new(|cx| QuickOpenInput::with_placeholder("Name…", cx)));
        if !self.sidebar_options_modal_open {
            self.show_project_management = true;
        }
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

    pub(super) fn begin_sidebar_rename(&mut self, kind: &str, id: String, cx: &mut Context<Self>) {
        if !matches!(kind, "project" | "workspace") {
            return;
        }
        let current_name = self.runtime_catalog.as_ref().and_then(|catalog| {
            match kind {
                "project" => catalog.projects.iter()
                    .find(|project| project.id == id)
                    .map(|project| project.name.as_str()),
                "workspace" => catalog.workspaces.iter()
                    .find(|workspace| workspace.id == id)
                    .map(|workspace| workspace.name.as_str()),
                _ => None,
            }
        }).unwrap_or_default().to_owned();
        self.sidebar_rename_target = Some((kind.to_owned(), id));
        let input = cx.new(|cx| QuickOpenInput::with_placeholder("New display name", cx));
        if !current_name.is_empty() {
            input.update(cx, |input, cx| { input.set_query(&current_name, cx); });
        }
        self.sidebar_rename_input = Some(input);
        if !self.sidebar_options_modal_open {
            self.show_project_management = true;
        }
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

    pub(super) fn select_sidebar_parent(
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

    pub(super) fn change_sidebar_parent(
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
        let previous_parent = self
            .runtime_catalog
            .as_ref()
            .and_then(|catalog| {
                catalog
                    .workspaces
                    .iter()
                    .find(|workspace| workspace.id == child_id)
            })
            .and_then(|workspace| workspace.parent_workspace_id.clone())
            .filter(|previous| link && previous != &parent_id);
        self.worktree_busy = true;
        let generation = self.workspace_generation;
        cx.spawn(async move |view, cx| {
            let result = cx
                .background_spawn(async move {
                    if let Some(previous) = &previous_parent {
                        bridge.set_workspace_parent(previous, &child_id, false)?;
                    }
                    let result = bridge.set_workspace_parent(&parent_id, &child_id, link);
                    if let (Err(error), Some(previous)) = (&result, previous_parent) {
                        if let Err(restore_error) =
                            bridge.set_workspace_parent(&previous, &child_id, true)
                        {
                            return Err(std::io::Error::other(format!(
                                "{error}. Previous parent restore failed: {restore_error}"
                            )));
                        }
                    }
                    result
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
                    }
                    Err(error) => view
                        .model
                        .set_status(format!("Parent relationship rejected: {error}")),
                }
                view.refresh_project_tree(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn pin_sidebar_workspace_tree(
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

    pub(super) fn forget_sidebar_project(
        &mut self,
        project_id: String,
        root: PathBuf,
        dialog: Option<gpui::AnyWindowHandle>,
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
                            "Project removal impact: {} workspaces, {} tabs, {} live sessions. Config override {}.",
                            preview.workspace_count, preview.tab_count, preview.active_session_count,
                            if preview.has_config_override { "yes" } else { "no" },
                        ));
                        view.sidebar_project_remove_pending = Some((id, preview));
                    }
                    Ok(Ok(root)) => {
                        let removal_dialog_open =
                            view.sidebar_options_modal_open && view.sidebar_project_remove_pending.is_some();
                        view.sidebar_project_remove_pending = None;
                        if let Some(dialog) = dialog.filter(|_| removal_dialog_open) {
                            view.sidebar_options_modal_open = false;
                            let _ = dialog.update(cx, |_, window, cx| window.close_dialog(cx));
                        }
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

    pub(super) fn pin_sidebar_project(&mut self, root: &Path, cx: &mut Context<Self>) {
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

    pub(super) fn sidebar_open_folder(&mut self, root: &Path, cx: &mut Context<Self>) {
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

    pub(super) fn sidebar_copy_path(&mut self, root: &Path, cx: &mut Context<Self>) {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
            root.to_string_lossy().into_owned(),
        ));
        self.model.set_status("Copied project path");
        cx.notify();
    }

    pub(super) fn sidebar_project_context_menu(
        owner: gpui::WeakEntity<Self>,
        root: PathBuf,
        project_id: Option<String>,
        can_remove: bool,
        git_project: bool,
    ) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
        move |mut menu, _, _| {
            if let Some(id) = &project_id {
                let settings_id = id.clone();
                let rename_id = id.clone();
                let rename_root = root.clone();
                menu = menu
                    .item(sidebar_menu_item("Open Project Settings", &owner, move |view, _, cx| {
                        if view.show_native_settings_section("projects", cx).is_ok() {
                            view.select_host_project_settings_project(&settings_id, cx);
                        }
                    }))
                    .item(sidebar_menu_item("Rename", &owner, move |view, window, cx| {
                        view.open_sidebar_options_dialog(SidebarOptionsTarget::Project(rename_root.clone()), window, cx);
                        if view.sidebar_options_modal_open {
                            view.begin_sidebar_rename("project", rename_id.clone(), cx);
                        }
                    }));
            }
            if project_id.is_none() {
                let path = root.clone();
                menu = menu.item(sidebar_menu_item("Open Project", &owner, move |view, _, cx| {
                    view.switch_workspace(path.clone(), cx);
                }));
            }
            let project = root.clone();
            menu = menu.item(sidebar_menu_item("New Workspace", &owner, move |view, window, cx| {
                view.open_new_sidebar_workspace(Some(project.clone()), window, cx);
            }).disabled(!git_project));
            if git_project {
                menu = menu.item(sidebar_menu_item("Refresh Worktrees", &owner, |view, _, cx| {
                    view.refresh_project_tree(cx);
                    view.refresh_worktrees(cx);
                }));
            }
            if let Some(id) = &project_id {
                let sleep_id = id.clone();
                menu = menu.item(sidebar_menu_item("Sleep Project…", &owner, move |view, window, cx| {
                    view.open_sidebar_project_sleep(sleep_id.clone(), window, cx);
                }));
                let id = id.clone();
                let root = root.clone();
                menu = menu.separator().item(sidebar_menu_item(
                    "Remove Project…",
                    &owner,
                    move |view, window, cx| view.open_sidebar_project_removal(id.clone(), root.clone(), window, cx),
                ).disabled(!can_remove));
            }
            menu
        }
    }

    pub(super) fn sidebar_workspace_context_menu(
        owner: gpui::WeakEntity<Self>,
        project_root: PathBuf,
        target: PathBuf,
        metadata: Option<RuntimeWorkspace>,
        catalog: Option<Arc<RuntimeCatalog>>,
        confirmation: Option<String>,
        is_primary: bool,
        is_active: bool,
    ) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
        move |mut menu, window, cx| {
            let action_item = |label: &str, action| {
                sidebar_workspace_action_item(label.to_owned(), &owner, action)
            };
            if let Some(workspace) = &metadata {
                let id = &workspace.id;
                let rename_id = id.clone();
                let workspace_target = SidebarOptionsTarget::Workspace {
                    project_root: project_root.clone(),
                    path: target.clone(),
                };
                let rename_target = workspace_target.clone();
                menu = menu
                    .item(sidebar_menu_item(
                        "Rename",
                        &owner,
                        move |view, window, cx| {
                            view.open_sidebar_options_dialog(rename_target.clone(), window, cx);
                            if view.sidebar_options_modal_open {
                                view.begin_sidebar_rename("workspace", rename_id.clone(), cx);
                            }
                        },
                    ))
                    .item(action_item(
                        if workspace.is_pinned {
                            "Unpin Workspace"
                        } else {
                            "Pin Workspace"
                        },
                        SidebarWorkspaceAction::Pin {
                            id: id.clone(),
                            pinned: !workspace.is_pinned,
                        },
                    ));

                if let Some(catalog) = &catalog {
                    let family =
                        alera_gpui_desktop::runtime_catalog::workspace_family_ids(catalog, id);
                    if family.len() > 1 {
                        for pinned in [true, false] {
                            let id = id.clone();
                            menu = menu.item(sidebar_menu_item(
                                if pinned {
                                    "Pin Workspace Tree"
                                } else {
                                    "Unpin Workspace Tree"
                                },
                                &owner,
                                move |view, _, cx| {
                                    view.pin_sidebar_workspace_tree(id.clone(), pinned, cx)
                                },
                            ));
                        }
                    }

                    let tag_items: Vec<_> = catalog
                        .tags
                        .iter()
                        .map(|tag| {
                            let selected = workspace.tag_ids.contains(&tag.id);
                            let label = if selected {
                                format!("✓ {}", tag.name)
                            } else {
                                tag.name.clone()
                            };
                            (
                                label,
                                SidebarWorkspaceAction::Tag {
                                    id: id.clone(),
                                    tag_id: tag.id.clone(),
                                    assign: !selected,
                                },
                            )
                        })
                        .collect();
                    let tag_owner = owner.clone();
                    let tag_id = id.clone();
                    let tag_target = workspace_target.clone();
                    menu = menu.submenu("Manage Tags", window, cx, move |menu, _, _| {
                        let id = tag_id.clone();
                        let target = tag_target.clone();
                        let menu = tag_items
                            .iter()
                            .cloned()
                            .fold(menu, |menu, (label, action)| {
                                menu.item(sidebar_workspace_action_item(label, &tag_owner, action))
                            });
                        menu.item(sidebar_menu_item(
                            "New Tag…",
                            &tag_owner,
                            move |view, window, cx| {
                                view.open_sidebar_options_dialog(target.clone(), window, cx);
                                if view.sidebar_options_modal_open {
                                    view.begin_sidebar_create("tag", id.clone(), cx);
                                }
                            },
                        ))
                        .scrollable(true)
                        .max_h(px(300.0))
                    });

                    let parents: Vec<_> = catalog
                        .workspaces
                        .iter()
                        .filter(|parent| {
                            parent.project_id == workspace.project_id
                                && !parent.is_archived
                                && !family.contains(&parent.id)
                                && Some(&parent.id) != workspace.parent_workspace_id.as_ref()
                        })
                        .cloned()
                        .collect();
                    if !parents.is_empty() {
                        let parent_owner = owner.clone();
                        let child = id.clone();
                        menu =
                            menu.submenu("Set Parent Workspace", window, cx, move |menu, _, _| {
                                parents
                                    .iter()
                                    .fold(menu, |menu, parent| {
                                        let parent_id = parent.id.clone();
                                        let child = child.clone();
                                        menu.item(sidebar_menu_item(
                                            parent.name.clone(),
                                            &parent_owner,
                                            move |view, _, cx| {
                                                view.change_sidebar_parent(
                                                    parent_id.clone(),
                                                    child.clone(),
                                                    true,
                                                    cx,
                                                );
                                            },
                                        ))
                                    })
                                    .scrollable(true)
                                    .max_h(px(300.0))
                            });
                    }
                    if let Some(parent) = &workspace.parent_workspace_id {
                        let parent = parent.clone();
                        let child = id.clone();
                        menu = menu.item(sidebar_menu_item(
                            "Clear Parent Workspace",
                            &owner,
                            move |view, _, cx| {
                                view.change_sidebar_parent(
                                    parent.clone(),
                                    child.clone(),
                                    false,
                                    cx,
                                );
                            },
                        ));
                    }

                    let section_items: Vec<_> = catalog
                        .sections
                        .iter()
                        .filter(|section| Some(&section.id) != workspace.section_id.as_ref())
                        .map(|section| {
                            (
                                section.name.clone(),
                                SidebarWorkspaceAction::Section {
                                    id: id.clone(),
                                    section_id: Some(section.id.clone()),
                                },
                            )
                        })
                        .collect();
                    let section_owner = owner.clone();
                    let section_id = id.clone();
                    let section_target = workspace_target.clone();
                    menu = menu.submenu("Set Section", window, cx, move |menu, _, _| {
                        let id = section_id.clone();
                        let target = section_target.clone();
                        let menu = section_items.iter().cloned().fold(
                            menu,
                            |menu, (label, action)| {
                                menu.item(sidebar_workspace_action_item(
                                    label,
                                    &section_owner,
                                    action,
                                ))
                            },
                        );
                        menu.item(sidebar_menu_item(
                            "New Section…",
                            &section_owner,
                            move |view, window, cx| {
                                view.open_sidebar_options_dialog(target.clone(), window, cx);
                                if view.sidebar_options_modal_open {
                                    view.begin_sidebar_create("section", id.clone(), cx);
                                }
                            },
                        ))
                        .scrollable(true)
                        .max_h(px(300.0))
                    });
                    if workspace.section_id.is_some() {
                        menu = menu.item(action_item(
                            "Clear Section",
                            SidebarWorkspaceAction::Section {
                                id: id.clone(),
                                section_id: None,
                            },
                        ));
                    }
                }
                menu = menu.separator();
            }

            let folder_label = if cfg!(windows) {
                "Open in File Explorer"
            } else if cfg!(target_os = "macos") {
                "Open in Finder"
            } else {
                "Open in File Manager"
            };
            let browser_root = target.clone();
            let folder_root = target.clone();
            let copy_root = target.clone();
            menu = menu
                .item(sidebar_menu_item(
                    "Open in Browser",
                    &owner,
                    move |view, _, cx| {
                        view.open_sidebar_repository(browser_root.clone(), cx);
                    },
                ))
                .item(sidebar_menu_item(
                    folder_label,
                    &owner,
                    move |view, _, cx| {
                        view.sidebar_open_folder(&folder_root, cx);
                    },
                ));
            if let Some(workspace) = &metadata {
                let project_id = workspace.project_id.clone();
                menu = menu.item(sidebar_menu_item(
                    "Open in Project Settings",
                    &owner,
                    move |view, _, cx| {
                        if view.show_native_settings_section("projects", cx).is_ok() {
                            view.select_host_project_settings_project(&project_id, cx);
                        }
                    },
                ));
            }
            menu = menu.item(sidebar_menu_item(
                "Copy Path",
                &owner,
                move |view, _, cx| {
                    view.sidebar_copy_path(&copy_root, cx);
                },
            ));

            if let Some(workspace) = &metadata {
                let id = &workspace.id;
                menu = menu.separator();
                if !workspace.is_archived && !is_active {
                    let armed = confirmation.as_deref() == Some(&format!("sleep:{id}"));
                    menu = menu.item(action_item(
                        if armed { "Confirm Sleep" } else { "Sleep…" },
                        SidebarWorkspaceAction::Sleep { id: id.clone() },
                    ));
                }
                if workspace.is_archived || !is_primary {
                    menu = menu.item(action_item(
                        if workspace.is_archived {
                            "Restore"
                        } else {
                            "Archive"
                        },
                        SidebarWorkspaceAction::Archive {
                            id: id.clone(),
                            archived: !workspace.is_archived,
                        },
                    ));
                }
                let armed = confirmation.as_deref() == Some(&format!("remove:{id}"));
                menu = menu.item(
                    action_item(
                        if armed {
                            "Confirm Remove Worktree"
                        } else {
                            "Remove…"
                        },
                        SidebarWorkspaceAction::Remove {
                            id: id.clone(),
                            root: project_root.clone(),
                            target: target.clone(),
                        },
                    )
                    .disabled(is_primary || is_active),
                );
            }
            menu.scrollable(true).max_h(px(360.0))
        }
    }

    fn open_sidebar_repository(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        cx.spawn(async move |view, cx| {
            let url = cx
                .background_spawn(async move {
                    alera_core::git::repository_remote_url(&root.to_string_lossy())
                        .ok()
                        .flatten()
                        .and_then(|remote| native_review_links::repository_home_url(&remote))
                })
                .await;
            let _ = view.update(cx, |view, cx| {
                if let Some(url) = url {
                    cx.open_url(&url);
                } else {
                    view.model
                        .set_status("No supported Git remote was found for this workspace");
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

fn sidebar_menu_item(
    label: impl Into<gpui::SharedString>,
    owner: &gpui::WeakEntity<Workbench>,
    action: impl Fn(&mut Workbench, &mut Window, &mut Context<Workbench>) + 'static,
) -> PopupMenuItem {
    let owner = owner.clone();
    PopupMenuItem::new(label).on_click(move |_, window, cx| {
        let _ = owner.update(cx, |view, cx| action(view, window, cx));
    })
}

fn sidebar_workspace_action_item(
    label: impl Into<gpui::SharedString>,
    owner: &gpui::WeakEntity<Workbench>,
    action: SidebarWorkspaceAction,
) -> PopupMenuItem {
    sidebar_menu_item(label, owner, move |view, _, cx| {
        view.begin_sidebar_workspace_action(action.clone(), cx);
    })
}
