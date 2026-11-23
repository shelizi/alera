//! Project / Workspace options belong in a GPUI Kit dialog, not a tiny
//! inline form which shifts the Classic-style sidebar during editing.
use super::*;
use alera_gpui_desktop::runtime_catalog::RuntimeWorkspace;
use native_components::{action_button, menu_item};
use native_project_sidebar_ui::SidebarWorkspaceAction;

#[derive(Clone, Debug)]
pub(super) enum SidebarOptionsTarget {
    Management(PathBuf),
    Project(PathBuf),
    Workspace {
        project_root: PathBuf,
        path: PathBuf,
    },
}

impl SidebarOptionsTarget {
    fn title(&self) -> &'static str {
        match self {
            Self::Management(_) => "Projects & Worktrees",
            Self::Project(_) => "Project options",
            Self::Workspace { .. } => "Workspace options",
        }
    }

    fn path(&self) -> &Path {
        match self {
            Self::Management(path) | Self::Project(path) | Self::Workspace { path, .. } => path,
        }
    }
}

#[derive(Clone)]
enum SidebarModalAction {
    Open(PathBuf),
    OpenFolder(PathBuf),
    CopyPath(PathBuf),
    PinProject(PathBuf, bool),
    Rename(&'static str, String),
    ForgetProject(String, PathBuf),
    NewWorktree(PathBuf),
    RefreshProjects,
    RefreshWorktrees,
    Create(&'static str, String),
    SelectParent(String, String),
    ChangeParent(String, String, bool),
    PinWorkspaceTree(String, bool),
    Workspace(SidebarWorkspaceAction),
}

struct SidebarModalRow {
    group: &'static str,
    label: String,
    action: SidebarModalAction,
    selected: bool,
    destructive: bool,
}

impl SidebarModalRow {
    fn new(group: &'static str, label: impl Into<String>, action: SidebarModalAction) -> Self {
        Self {
            group,
            label: label.into(),
            action,
            selected: false,
            destructive: false,
        }
    }

    fn selected(mut self, value: bool) -> Self {
        self.selected = value;
        self
    }

    fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }
}

impl Workbench {
    pub(super) fn open_sidebar_options_dialog(
        &mut self,
        target: SidebarOptionsTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(target, SidebarOptionsTarget::Management(_))
            && !self.project_tree_groups.iter().any(|group| {
                group.root == target.path()
                    || group
                        .worktrees
                        .iter()
                        .any(|item| item.path == target.path())
            })
        {
            self.model
                .set_status("Project or Workspace changed; refresh the sidebar");
            cx.notify();
            return;
        }
        self.cancel_sidebar_options_dialog(cx);
        self.sidebar_options_modal_open = true;
        let generation = self.workspace_generation;
        let owner = cx.entity().downgrade();
        let first_focus = std::cell::Cell::new(true);
        window.open_dialog(cx, move |dialog, window, cx| {
            if first_focus.replace(false) {
                if let Some(entity) = owner.upgrade() {
                    let focus = entity.read(cx).sidebar_options_modal_focus.clone();
                    window.focus(&focus, cx);
                }
            }
            let content = owner
                .upgrade()
                .map(|entity| {
                    entity.update(cx, |view, cx| {
                        view.render_sidebar_options_dialog(&target, generation, cx)
                    })
                })
                .unwrap_or_else(|| div().into_any_element());
            let cancel_owner = owner.clone();
            dialog
                .title(target.title())
                .w(px(580.0))
                .centered()
                .close_button(false)
                .overlay_closable(true)
                .on_cancel(move |_, _, cx| {
                    let _ =
                        cancel_owner.update(cx, |view, cx| view.cancel_sidebar_options_dialog(cx));
                    true
                })
                // Never let the dialog's default Enter invoke an implicit mutation.
                .on_ok(|_, _, _| false)
                .child(content)
        });
    }

    fn cancel_sidebar_options_dialog(&mut self, cx: &mut Context<Self>) {
        self.sidebar_options_modal_open = false;
        self.sidebar_rename_target = None;
        self.sidebar_rename_input = None;
        self.sidebar_create_target = None;
        self.sidebar_create_input = None;
        self.worktree_create_input = None;
        self.worktree_create_root = None;
        self.sidebar_pending_action = None;
        self.sidebar_project_remove_pending = None;
        cx.notify();
    }

    fn run_sidebar_modal_action(
        &mut self,
        action: SidebarModalAction,
        generation: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_generation != generation || self.worktree_busy {
            self.model
                .set_status("Workspace changed or Worktree operation busy; refresh first");
            cx.notify();
            return;
        }
        match action {
            SidebarModalAction::Open(path) => {
                self.cancel_sidebar_options_dialog(cx);
                window.close_dialog(cx);
                self.switch_workspace(path, cx);
            }
            SidebarModalAction::OpenFolder(path) => {
                self.cancel_sidebar_options_dialog(cx);
                self.sidebar_open_folder(&path, cx);
                window.close_dialog(cx);
            }
            SidebarModalAction::CopyPath(path) => self.sidebar_copy_path(&path, cx),
            SidebarModalAction::PinProject(path, pin) => {
                if pin {
                    self.pin_sidebar_project(&path, cx);
                } else {
                    self.unpin_project(&path, cx);
                }
            }
            SidebarModalAction::Rename(kind, id) => {
                let before = self.show_project_management;
                self.begin_sidebar_rename(kind, id, cx);
                self.show_project_management = before;
                if let Some(input) = &self.sidebar_rename_input {
                    window.focus(&input.read(cx).focus_handle(cx), cx);
                }
            }
            SidebarModalAction::ForgetProject(id, path) => {
                self.forget_sidebar_project(id, path, cx);
            }
            SidebarModalAction::RefreshProjects => self.refresh_project_tree(cx),
            SidebarModalAction::RefreshWorktrees => self.refresh_worktrees(cx),
            SidebarModalAction::NewWorktree(root) => {
                let before = self.show_project_management;
                self.begin_worktree_for_project(root, cx);
                self.show_project_management = before;
                if let Some(input) = &self.worktree_create_input {
                    window.focus(&input.read(cx).focus_handle(cx), cx);
                }
            }
            SidebarModalAction::Create(kind, id) => {
                let before = self.show_project_management;
                self.begin_sidebar_create(kind, id, cx);
                self.show_project_management = before;
                if let Some(input) = &self.sidebar_create_input {
                    window.focus(&input.read(cx).focus_handle(cx), cx);
                }
            }
            SidebarModalAction::SelectParent(id, project) => {
                self.select_sidebar_parent(id, project, cx)
            }
            SidebarModalAction::ChangeParent(parent, child, link) => {
                self.change_sidebar_parent(parent, child, link, cx)
            }
            SidebarModalAction::PinWorkspaceTree(id, pinned) => {
                self.pin_sidebar_workspace_tree(id, pinned, cx)
            }
            SidebarModalAction::Workspace(action) => {
                self.begin_sidebar_workspace_action(action, cx)
            }
        }
        cx.notify();
    }

    fn sidebar_modal_rows(&self, target: &SidebarOptionsTarget) -> Vec<SidebarModalRow> {
        let mut rows = Vec::new();
        match target {
            SidebarOptionsTarget::Management(root) => {
                let pinned = self.saved_projects.iter().any(|saved| saved == root);
                rows.push(SidebarModalRow::new(
                    "Actions",
                    "Refresh projects",
                    SidebarModalAction::RefreshProjects,
                ));
                rows.push(SidebarModalRow::new(
                    "Actions",
                    "Refresh Git Worktrees",
                    SidebarModalAction::RefreshWorktrees,
                ));
                rows.push(SidebarModalRow::new(
                    "Actions",
                    "New Git Worktree",
                    SidebarModalAction::NewWorktree(root.clone()),
                ));
                rows.push(
                    SidebarModalRow::new(
                        "Actions",
                        if pinned {
                            "Unpin current project"
                        } else {
                            "Pin current project"
                        },
                        SidebarModalAction::PinProject(root.clone(), !pinned),
                    )
                    .selected(pinned),
                );
                for path in self.saved_projects.iter().take(12) {
                    rows.push(SidebarModalRow::new(
                        "Saved projects",
                        format!(
                            "Open: {}",
                            path.file_name().unwrap_or_default().to_string_lossy()
                        ),
                        SidebarModalAction::Open(path.clone()),
                    ));
                }
                for path in self.recent_workspaces.iter().take(12) {
                    rows.push(SidebarModalRow::new(
                        "Recent workspaces",
                        format!(
                            "Open: {}",
                            path.file_name().unwrap_or_default().to_string_lossy()
                        ),
                        SidebarModalAction::Open(path.clone()),
                    ));
                }
                for entry in self.worktrees.iter().take(24) {
                    rows.push(SidebarModalRow::new(
                        "Git Worktrees",
                        format!(
                            "Open: {}",
                            entry.path.file_name().unwrap_or_default().to_string_lossy()
                        ),
                        SidebarModalAction::Open(entry.path.clone()),
                    ));
                }
            }
            SidebarOptionsTarget::Project(root) => {
                let pinned = self.saved_projects.iter().any(|path| path == root);
                rows.push(SidebarModalRow::new(
                    "Project",
                    "Open project",
                    SidebarModalAction::Open(root.clone()),
                ));
                rows.push(
                    SidebarModalRow::new(
                        "Project",
                        if pinned {
                            "Unpin project"
                        } else {
                            "Pin project"
                        },
                        SidebarModalAction::PinProject(root.clone(), !pinned),
                    )
                    .selected(pinned),
                );
                rows.push(SidebarModalRow::new(
                    "Manage",
                    "New Worktree",
                    SidebarModalAction::NewWorktree(root.clone()),
                ));
                if let Some(project) = self.runtime_catalog.as_ref().and_then(|catalog| {
                    catalog
                        .projects
                        .iter()
                        .find(|project| project.root == *root)
                }) {
                    rows.push(SidebarModalRow::new(
                        "Manage",
                        "Rename project",
                        SidebarModalAction::Rename("project", project.id.clone()),
                    ));
                    let pending = self
                        .sidebar_project_remove_pending
                        .as_ref()
                        .filter(|(id, _)| id == &project.id);
                    let label = if let Some((_, impact)) = pending {
                        format!(
                            "Confirm forget project ({} workspaces; {} live sessions)",
                            impact.workspace_count, impact.active_session_count
                        )
                    } else {
                        "Forget project (preview impact first)".into()
                    };
                    rows.push(
                        SidebarModalRow::new(
                            "Danger zone",
                            label,
                            SidebarModalAction::ForgetProject(project.id.clone(), root.clone()),
                        )
                        .destructive(),
                    );
                }
                rows.push(SidebarModalRow::new(
                    "Files",
                    "Open project folder",
                    SidebarModalAction::OpenFolder(root.clone()),
                ));
                rows.push(SidebarModalRow::new(
                    "Files",
                    "Copy project path",
                    SidebarModalAction::CopyPath(root.clone()),
                ));
            }
            SidebarOptionsTarget::Workspace { project_root, path } => {
                rows.push(SidebarModalRow::new(
                    "Workspace",
                    "Open Workspace",
                    SidebarModalAction::Open(path.clone()),
                ));
                rows.push(SidebarModalRow::new(
                    "Files",
                    "Open folder",
                    SidebarModalAction::OpenFolder(path.clone()),
                ));
                rows.push(SidebarModalRow::new(
                    "Files",
                    "Copy path",
                    SidebarModalAction::CopyPath(path.clone()),
                ));
                if let Some(workspace) = self.runtime_catalog.as_ref().and_then(|catalog| {
                    catalog
                        .workspaces
                        .iter()
                        .find(|workspace| workspace.path == *path)
                }) {
                    self.add_sidebar_workspace_modal_rows(&mut rows, workspace, project_root, path);
                }
            }
        }
        rows.sort_by_key(|row| match row.group {
            "Project" | "Workspace" | "Actions" => 0,
            "Manage" | "Hierarchy" => 1,
            "Tags & sections" | "Saved projects" => 2,
            "Files" | "Recent workspaces" => 3,
            "Git Worktrees" => 4,
            "Danger zone" => 5,
            _ => 6,
        });
        rows
    }

    fn add_sidebar_workspace_modal_rows(
        &self,
        rows: &mut Vec<SidebarModalRow>,
        workspace: &RuntimeWorkspace,
        project_root: &Path,
        path: &Path,
    ) {
        let id = workspace.id.clone();
        rows.push(SidebarModalRow::new(
            "Workspace",
            "Rename Workspace",
            SidebarModalAction::Rename("workspace", id.clone()),
        ));
        rows.push(
            SidebarModalRow::new(
                "Workspace",
                if workspace.is_pinned {
                    "Unpin Workspace"
                } else {
                    "Pin Workspace"
                },
                SidebarModalAction::Workspace(SidebarWorkspaceAction::Pin {
                    id: id.clone(),
                    pinned: !workspace.is_pinned,
                }),
            )
            .selected(workspace.is_pinned),
        );
        rows.push(
            SidebarModalRow::new(
                "Workspace",
                if workspace.is_archived {
                    "Restore Workspace"
                } else {
                    "Archive Workspace"
                },
                SidebarModalAction::Workspace(SidebarWorkspaceAction::Archive {
                    id: id.clone(),
                    archived: !workspace.is_archived,
                }),
            )
            .selected(workspace.is_archived),
        );
        rows.push(SidebarModalRow::new(
            "Hierarchy",
            "Use as parent",
            SidebarModalAction::SelectParent(id.clone(), workspace.project_id.clone()),
        ));
        if let Some(parent) = &workspace.parent_workspace_id {
            rows.push(SidebarModalRow::new(
                "Hierarchy",
                "Detach parent",
                SidebarModalAction::ChangeParent(parent.clone(), id.clone(), false),
            ));
        } else if let Some((parent, project)) = &self.sidebar_selected_parent {
            if project == &workspace.project_id && parent != &id {
                rows.push(SidebarModalRow::new(
                    "Hierarchy",
                    "Attach to selected parent",
                    SidebarModalAction::ChangeParent(parent.clone(), id.clone(), true),
                ));
            }
        }
        let catalog = self.runtime_catalog.as_ref();
        if catalog.is_some_and(|catalog| {
            alera_gpui_desktop::runtime_catalog::workspace_family_ids(catalog, &id).len() > 1
        }) {
            for pin in [true, false] {
                rows.push(SidebarModalRow::new(
                    "Hierarchy",
                    if pin {
                        "Pin workspace tree"
                    } else {
                        "Unpin workspace tree"
                    },
                    SidebarModalAction::PinWorkspaceTree(id.clone(), pin),
                ));
            }
        }
        rows.push(SidebarModalRow::new(
            "Tags & sections",
            "New shared tag",
            SidebarModalAction::Create("tag", id.clone()),
        ));
        rows.push(SidebarModalRow::new(
            "Tags & sections",
            "New workspace section",
            SidebarModalAction::Create("section", id.clone()),
        ));
        if let Some(catalog) = catalog {
            for tag in catalog.tags.iter().take(12) {
                let selected = workspace.tag_ids.contains(&tag.id);
                rows.push(
                    SidebarModalRow::new(
                        "Tags & sections",
                        format!(
                            "{} {}",
                            if selected { "Remove tag:" } else { "Add tag:" },
                            tag.name
                        ),
                        SidebarModalAction::Workspace(SidebarWorkspaceAction::Tag {
                            id: id.clone(),
                            tag_id: tag.id.clone(),
                            assign: !selected,
                        }),
                    )
                    .selected(selected),
                );
            }
            for section in catalog.sections.iter().take(10) {
                rows.push(
                    SidebarModalRow::new(
                        "Tags & sections",
                        format!("Section: {}", section.name),
                        SidebarModalAction::Workspace(SidebarWorkspaceAction::Section {
                            id: id.clone(),
                            section_id: Some(section.id.clone()),
                        }),
                    )
                    .selected(workspace.section_id.as_ref() == Some(&section.id)),
                );
            }
            if workspace.section_id.is_some() {
                rows.push(SidebarModalRow::new(
                    "Tags & sections",
                    "Move to Others",
                    SidebarModalAction::Workspace(SidebarWorkspaceAction::Section {
                        id: id.clone(),
                        section_id: None,
                    }),
                ));
            }
            if let Some(tag) = catalog
                .tags
                .iter()
                .find(|tag| workspace.tag_ids.contains(&tag.id))
            {
                let key = format!("tag:{}", tag.id);
                let confirmed = self.sidebar_pending_action.as_deref() == Some(key.as_str());
                rows.push(
                    SidebarModalRow::new(
                        "Danger zone",
                        if confirmed {
                            "Confirm delete assigned tag globally"
                        } else {
                            "Delete assigned tag globally (confirm)"
                        },
                        SidebarModalAction::Workspace(SidebarWorkspaceAction::DeleteTag {
                            id: tag.id.clone(),
                        }),
                    )
                    .destructive(),
                );
            }
            if let Some(section) = &workspace.section_id {
                let key = format!("section:{section}");
                let confirmed = self.sidebar_pending_action.as_deref() == Some(key.as_str());
                rows.push(
                    SidebarModalRow::new(
                        "Danger zone",
                        if confirmed {
                            "Confirm delete current section"
                        } else {
                            "Delete current section (confirm)"
                        },
                        SidebarModalAction::Workspace(SidebarWorkspaceAction::DeleteSection {
                            id: section.clone(),
                        }),
                    )
                    .destructive(),
                );
            }
        }
        if path != self.model.root() {
            let key = format!("sleep:{id}");
            let confirmed = self.sidebar_pending_action.as_deref() == Some(key.as_str());
            rows.push(
                SidebarModalRow::new(
                    "Danger zone",
                    if confirmed {
                        "Confirm Sleep Workspace"
                    } else {
                        "Sleep Workspace (confirm)"
                    },
                    SidebarModalAction::Workspace(SidebarWorkspaceAction::Sleep { id: id.clone() }),
                )
                .destructive(),
            );
            if path != project_root {
                let key = format!("remove:{id}");
                let confirmed = self.sidebar_pending_action.as_deref() == Some(key.as_str());
                rows.push(
                    SidebarModalRow::new(
                        "Danger zone",
                        if confirmed {
                            "Confirm Remove Worktree"
                        } else {
                            "Remove Worktree (confirm)"
                        },
                        SidebarModalAction::Workspace(SidebarWorkspaceAction::Remove {
                            id,
                            root: project_root.to_path_buf(),
                            target: path.to_path_buf(),
                        }),
                    )
                    .destructive(),
                );
            }
        }
    }

    fn render_sidebar_options_dialog(
        &mut self,
        target: &SidebarOptionsTarget,
        generation: u64,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        if self.workspace_generation != generation {
            return div()
                .p_3()
                .text_sm()
                .text_color(rgb(MUTED))
                .child("Workspace changed. Close this dialog and reopen options.")
                .into_any_element();
        }
        let mut body = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(target.path().display().to_string()),
            )
            .child(div().border_b_1().border_color(rgb(BORDER)));

        if let (Some((kind, _)), Some(input)) = (
            self.sidebar_rename_target.as_ref(),
            self.sidebar_rename_input.clone(),
        ) {
            return body
                .child(
                    div()
                        .text_sm()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child(if kind == "project" {
                            "Rename project"
                        } else {
                            "Rename Workspace"
                        }),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child("Display name only: Git branch and directory stay unchanged."),
                )
                .child(input)
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            action_button("sidebar-modal-rename-cancel", "Cancel", false, false)
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.sidebar_rename_target = None;
                                    view.sidebar_rename_input = None;
                                    cx.notify();
                                })),
                        )
                        .child(
                            action_button(
                                "sidebar-modal-rename-save",
                                "Save display name",
                                true,
                                false,
                            )
                            .on_click(cx.listener(|view, _, _, cx| view.submit_sidebar_rename(cx))),
                        ),
                )
                .into_any_element();
        }
        if let (Some((kind, _)), Some(input)) = (
            self.sidebar_create_target.as_ref(),
            self.sidebar_create_input.clone(),
        ) {
            return body
                .child(
                    div()
                        .text_sm()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child(if kind == "tag" {
                            "New shared tag"
                        } else {
                            "New workspace section"
                        }),
                )
                .child(input)
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            action_button("sidebar-modal-create-cancel", "Cancel", false, false)
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.sidebar_create_target = None;
                                    view.sidebar_create_input = None;
                                    cx.notify();
                                })),
                        )
                        .child(
                            action_button("sidebar-modal-create-save", "Create", true, false)
                                .on_click(
                                    cx.listener(|view, _, _, cx| view.submit_sidebar_create(cx)),
                                ),
                        ),
                )
                .into_any_element();
        }
        if let Some(input) = self.worktree_create_input.clone() {
            return body
                .child(
                    div()
                        .text_sm()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child("New Git Worktree"),
                )
                .child(div().text_xs().text_color(rgb(MUTED)).child(format!(
                        "Project: {}",
                        self.worktree_create_root
                            .as_deref()
                            .unwrap_or(target.path())
                            .display()
                    )))
                .child(input)
                .child(
                    action_button(
                        "sidebar-modal-branch-mode",
                        if self.worktree_create_from_existing {
                            "Use existing local branch (change)"
                        } else {
                            "Create new local branch (change)"
                        },
                        false,
                        false,
                    )
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.worktree_create_from_existing = !view.worktree_create_from_existing;
                        cx.notify();
                    })),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            action_button("sidebar-modal-worktree-cancel", "Cancel", false, false)
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.worktree_create_input = None;
                                    view.worktree_create_root = None;
                                    cx.notify();
                                })),
                        )
                        .child(
                            action_button(
                                "sidebar-modal-worktree-create",
                                if self.worktree_busy {
                                    "Creating..."
                                } else {
                                    "Create Worktree"
                                },
                                true,
                                false,
                            )
                            .on_click(cx.listener(|view, _, _, cx| view.create_worktree(cx))),
                        ),
                )
                .into_any_element();
        }

        let rows = self.sidebar_modal_rows(target);
        let mut previous_group: Option<&str> = None;
        for (index, row) in rows.into_iter().enumerate() {
            if previous_group != Some(row.group) {
                previous_group = Some(row.group);
                body = body.child(
                    div()
                        .mt_2()
                        .px_2()
                        .text_xs()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(MUTED))
                        .child(row.group),
                );
            }
            let action = row.action;
            body = body.child(
                menu_item(
                    ("sidebar-modal-option", index),
                    row.label,
                    row.selected,
                    row.destructive,
                )
                .on_click(cx.listener(move |view, _, window, cx| {
                    view.run_sidebar_modal_action(action.clone(), generation, window, cx);
                })),
            );
        }
        body.into_any_element()
    }
}
