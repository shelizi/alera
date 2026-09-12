//! Removal lifecycle for a managed workspace: validating ownership, removing
//! the Git worktree, deleting its branch, and dropping the runtime row.

use alera_core::git::{self as core_git, GitErrorKind};
use alera_core::runtime::{
    AutomationState, AutomationTarget, Project, RuntimeStore, Workspace, WorkspaceKind,
    LOCAL_HOST_ID,
};
use anyhow::{anyhow, bail, Context, Result};

use super::managed_workspace_storage::validate_workspace_storage_ownership;
use super::{path_equals, ManagedWorkspaceRemoveRequest};
struct ManagedWorkspaceRemoval {
    workspace: Workspace,
    project: Project,
    branch_to_delete: Option<String>,
}

pub async fn remove_managed_workspace(
    store: &RuntimeStore,
    request: ManagedWorkspaceRemoveRequest,
) -> Result<Workspace> {
    let removal = managed_workspace_removal(store, &request).await?;
    let workspace = removal.workspace;
    let project = removal.project;
    let branch_to_delete = removal.branch_to_delete;
    match core_git::remove_worktree(&project.repo_path, &workspace.path, true) {
        Ok(()) => {}
        Err(error)
            if error.kind == GitErrorKind::WorktreeNotFound
                && filesystem_entry_is_missing(&workspace.path)? => {}
        Err(error) => return Err(error).context("git worktree remove failed"),
    }
    if let Some(branch) = branch_to_delete {
        match core_git::delete_branch(&project.repo_path, &branch, true) {
            Ok(()) => {}
            Err(error) if error.kind == GitErrorKind::BranchNotFound => {}
            Err(error) => {
                return Err(error).with_context(|| format!("git branch -D {branch} failed"));
            }
        }
    }
    store.remove_workspace(&workspace.id, true).await?;
    Ok(workspace)
}

pub async fn workspace_has_active_automation_owner(
    store: &RuntimeStore,
    workspace_id: &str,
) -> Result<bool> {
    let definitions = store.list_automations(false).await?;
    if definitions.iter().any(|automation| {
        automation.state == AutomationState::Active
            && match &automation.target {
                AutomationTarget::ExistingTab {
                    workspace_id: target,
                    ..
                }
                | AutomationTarget::FreshTab {
                    workspace_id: target,
                    ..
                } => target == workspace_id,
                AutomationTarget::ManagedWorkspace {
                    source_workspace_id,
                    ..
                } => source_workspace_id == workspace_id,
            }
    }) {
        return Ok(true);
    }
    Ok(store
        .list_active_automation_runs()
        .await?
        .iter()
        .any(|run| {
            run.workspace_id.as_deref() == Some(workspace_id)
                || run
                    .target_identity
                    .as_ref()
                    .and_then(|identity| identity.workspace_id.as_deref())
                    == Some(workspace_id)
        }))
}

pub async fn validate_managed_workspace_removal(
    store: &RuntimeStore,
    request: &ManagedWorkspaceRemoveRequest,
) -> Result<()> {
    #[cfg(test)]
    super::managed_workspace_removal_test_gate::wait_if_installed(&request.id).await;
    managed_workspace_removal(store, request).await.map(drop)
}

async fn managed_workspace_removal(
    store: &RuntimeStore,
    request: &ManagedWorkspaceRemoveRequest,
) -> Result<ManagedWorkspaceRemoval> {
    let workspace = store
        .find_workspace(&request.id)
        .await?
        .ok_or_else(|| anyhow!("Workspace not found: {}", request.id))?;
    if workspace.kind == WorkspaceKind::Main {
        bail!("The main workspace cannot be removed");
    }
    if workspace.host_id != LOCAL_HOST_ID {
        bail!("Workspace is not owned by the local host");
    }
    let project = store
        .find_project(&workspace.project_id)
        .await?
        .ok_or_else(|| anyhow!("Project not found: {}", workspace.project_id))?;
    let should_delete_branch = request
        .delete_branch
        .unwrap_or(!workspace.reuses_existing_branch);
    let branch_to_delete = if should_delete_branch {
        Some(
            workspace
                .branch
                .as_deref()
                .filter(|branch| !branch.is_empty())
                .ok_or_else(|| anyhow!("Workspace Branch Is Required"))?
                .to_string(),
        )
    } else {
        None
    };
    validate_workspace_storage_ownership(store, &workspace, &project).await?;
    if workspace_has_active_automation_owner(store, &workspace.id).await? {
        bail!("Workspace is owned by an active automation");
    }
    if !filesystem_entry_is_missing(&workspace.path)? {
        let registered = core_git::list_worktrees(&project.repo_path)?
            .into_iter()
            .find(|entry| path_equals(&entry.path, &workspace.path))
            .ok_or_else(|| anyhow!("Workspace path is not a registered Git worktree"))?;
        if let Some(expected_branch) = workspace.branch.as_deref() {
            if registered.branch != expected_branch {
                bail!(
                    "Workspace branch does not match registered worktree: expected {expected_branch}, found {}",
                    registered.branch
                );
            }
        }
    }
    Ok(ManagedWorkspaceRemoval {
        workspace,
        project,
        branch_to_delete,
    })
}

fn filesystem_entry_is_missing(path: &str) -> Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => {
            Err(error).with_context(|| format!("Could not inspect workspace path \"{path}\""))
        }
    }
}
