//! Lifecycle of an Alera-managed Git worktree workspace: validating a create
//! or remove request, resolving where the worktree lives, and driving `git`.
//!
//! Applying the project's `worktree.copy` rules and `worktree.setup` commands
//! lives in [`crate::worktree_setup`].

use std::path::{Path, PathBuf};

use alera_core::git as core_git;
use alera_core::runtime::{
    Project, ProjectKind, RuntimeStore, Workspace, WorkspaceCreationResult, WorkspaceKind,
    WorkspaceStatus, LOCAL_HOST_ID,
};
use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

use crate::worktree_setup::{prepare_deferred_worktree_setup, run_worktree_setup};

#[path = "managed_workspace_removal.rs"]
mod managed_workspace_removal;
#[cfg(test)]
#[path = "managed_workspace_removal_test_gate.rs"]
pub(crate) mod managed_workspace_removal_test_gate;
#[path = "managed_workspace_storage.rs"]
mod managed_workspace_storage;

pub use managed_workspace_removal::{
    remove_managed_workspace, validate_managed_workspace_removal,
    workspace_has_active_automation_owner,
};
pub use managed_workspace_storage::{measure_workspace_storage, validate_workspace_storage_path};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedWorkspaceCreateRequest {
    #[serde(default)]
    pub id: Option<String>,
    pub project_id: String,
    #[serde(default)]
    pub name: Option<String>,
    pub branch: String,
    #[serde(default)]
    pub source_branch: Option<String>,
    #[serde(default)]
    pub reuse_existing_branch: bool,
    #[serde(default)]
    pub workspace_root: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub parent_workspace_id: Option<String>,
    /// Asks the host to prepare the worktree setup instead of running it, so
    /// the caller can show it in a terminal. Defaults to running it inline,
    /// which is what the `alera` CLI and the mobile gateway still want.
    #[serde(default)]
    pub defer_setup: bool,
    /// Skips both copy and command setup for callers with an explicit policy.
    #[serde(default)]
    pub skip_setup: bool,
    /// Directory the deferred setup script is written to. The host fills this
    /// in from its own state directory; a caller cannot choose it.
    #[serde(skip)]
    pub setup_script_directory: Option<PathBuf>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedWorkspaceRemoveRequest {
    pub id: String,
    #[serde(default)]
    pub delete_branch: Option<bool>,
    #[serde(default)]
    pub active_workspace_id: Option<String>,
    /// Explicit consent to stop this workspace's sessions before removing it.
    #[serde(default)]
    pub close_sessions: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedWorkspaceSwitchBranchRequest {
    pub id: String,
    pub branch: String,
}

pub async fn create_managed_workspace(
    store: &RuntimeStore,
    request: ManagedWorkspaceCreateRequest,
) -> Result<WorkspaceCreationResult> {
    let project = store
        .find_project(&request.project_id)
        .await?
        .ok_or_else(|| anyhow!("Project not found: {}", request.project_id))?;
    if project.kind != ProjectKind::GitRepository {
        bail!("Linked Workspaces Require a Git Repository Project");
    }
    let requested_id = match request.id.as_deref().map(str::trim) {
        Some("") => bail!("Workspace Id Is Required"),
        Some(id) => {
            if store.find_workspace(id).await?.is_some() {
                bail!("A workspace with id \"{id}\" already exists");
            }
            Some(id.to_string())
        }
        None => None,
    };
    if let Some(parent_workspace_id) = request.parent_workspace_id.as_deref() {
        if requested_id.as_deref() == Some(parent_workspace_id) {
            bail!("Workspace cannot be related to itself");
        }
        if store.find_workspace(parent_workspace_id).await?.is_none() {
            bail!("Parent workspace not found: {parent_workspace_id}");
        }
    }

    let branch = require_trimmed(&request.branch, "New Branch Name Is Required")?;
    let source_branch = request
        .source_branch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    if !request.reuse_existing_branch && source_branch.is_none() {
        bail!("Source Branch Is Required");
    }

    if !core_git::is_valid_branch_name(&branch)? {
        bail!("Invalid branch name \"{branch}\"");
    }
    if request.reuse_existing_branch {
        ensure_target_branch_exists(&project, &branch)?;
    } else {
        let source = source_branch.as_deref().expect("checked above");
        ensure_source_branch_exists(&project, source)?;
        ensure_new_branch_does_not_exist(&project, &branch)?;
    }

    let workspaces = store.list_workspaces(&project.id).await?;
    if workspaces
        .iter()
        .any(|workspace| workspace.branch.as_deref() == Some(branch.as_str()))
    {
        bail!("A workspace for branch \"{branch}\" already exists");
    }

    let display_name = request
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&branch)
        .to_string();
    let workspace_path = resolve_workspace_path(store, &project, &display_name, &request).await?;
    if workspaces
        .iter()
        .any(|workspace| path_equals(&workspace.path, &workspace_path))
    {
        bail!("A workspace already exists at \"{workspace_path}\"");
    }

    if !request.reuse_existing_branch {
        let source = source_branch.as_deref().expect("checked above");
        if let Err(error) = core_git::refresh_source_branch(&project.repo_path, source) {
            tracing::warn!(
                repo_path = %project.repo_path,
                source_branch = %source,
                "git source branch refresh failed, proceeding with local branch: {error:#}",
            );
        }
    }

    if let Some(parent) = Path::new(&workspace_path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    core_git::create_worktree(
        &project.repo_path,
        &branch,
        &workspace_path,
        source_branch.as_deref().unwrap_or(""),
        request.reuse_existing_branch,
    )
    .context("git worktree add failed")?;

    let now = Utc::now();
    let workspace = Workspace {
        id: requested_id.unwrap_or_else(|| Uuid::new_v4().to_string()),
        instance_id: Uuid::new_v4().to_string(),
        host_id: LOCAL_HOST_ID.to_string(),
        project_id: project.id.clone(),
        name: display_name,
        branch: Some(branch),
        path: workspace_path,
        created_at: now,
        updated_at: now,
        kind: WorkspaceKind::Linked,
        status: WorkspaceStatus::Active,
        source_branch: if request.reuse_existing_branch {
            None
        } else {
            source_branch
        },
        reuses_existing_branch: request.reuse_existing_branch,
        is_pinned: false,
        tag_ids: Vec::new(),
        tag_names: Vec::new(),
        parent_workspace_id: None,
        section_id: None,
        child_count: 0,
        archived_at: None,
    };
    let mut workspace = store.upsert_workspace(workspace).await?;
    if let Some(parent_workspace_id) = request.parent_workspace_id.as_deref() {
        store
            .link_workspaces(parent_workspace_id, &workspace.id)
            .await?;
        workspace = store
            .find_workspace(&workspace.id)
            .await?
            .ok_or_else(|| anyhow!("Workspace disappeared after linking: {}", workspace.id))?;
    }
    if request.skip_setup {
        return Ok(WorkspaceCreationResult {
            workspace,
            setup_report: alera_core::runtime::WorktreeSetupReport::empty(),
            deferred_setup_command: None,
        });
    }
    if request.defer_setup {
        let (setup_report, deferred_setup_command) = prepare_deferred_worktree_setup(
            store,
            &project,
            &workspace,
            request.setup_script_directory.as_deref(),
        )
        .await;
        return Ok(WorkspaceCreationResult {
            workspace,
            setup_report,
            deferred_setup_command,
        });
    }
    let setup_report = run_worktree_setup(store, &project, &workspace).await;
    Ok(WorkspaceCreationResult {
        workspace,
        setup_report,
        deferred_setup_command: None,
    })
}

pub async fn switch_managed_workspace_branch(
    store: &RuntimeStore,
    request: ManagedWorkspaceSwitchBranchRequest,
) -> Result<Workspace> {
    let mut workspace = store
        .find_workspace(&request.id)
        .await?
        .ok_or_else(|| anyhow!("Workspace not found: {}", request.id))?;
    let project = store
        .find_project(&workspace.project_id)
        .await?
        .ok_or_else(|| anyhow!("Project not found: {}", workspace.project_id))?;
    if project.kind != ProjectKind::GitRepository {
        bail!("Branch switching requires a Git repository project");
    }
    let branch = require_trimmed(&request.branch, "Branch name is required")?;
    if !core_git::is_valid_branch_name(&branch)? {
        bail!("Invalid branch name \"{branch}\"");
    }
    if workspace.branch.as_deref() == Some(branch.as_str()) {
        return Ok(workspace);
    }
    ensure_target_branch_exists(&project, &branch)?;
    let workspaces = store.list_workspaces(&project.id).await?;
    if workspaces.iter().any(|candidate| {
        candidate.id != workspace.id
            && candidate.status == WorkspaceStatus::Active
            && candidate.branch.as_deref() == Some(branch.as_str())
    }) {
        bail!("A workspace for branch \"{branch}\" already exists");
    }

    let previous_branch = workspace.branch.clone();
    let workspace_path = workspace.path.clone();
    core_git::checkout_branch(&workspace_path, &branch).context("git checkout failed")?;
    workspace.branch = Some(branch.clone());
    workspace.source_branch = None;
    workspace.reuses_existing_branch = workspace.kind != WorkspaceKind::Main;
    workspace.updated_at = Utc::now();
    match store.upsert_workspace(workspace).await {
        Ok(saved) => Ok(saved),
        Err(error) => {
            if let Some(previous) = previous_branch.as_deref().filter(|previous| {
                !previous.is_empty() && *previous != "HEAD" && *previous != branch.as_str()
            }) {
                let _ = core_git::checkout_branch(&workspace_path, previous);
            }
            Err(error.into())
        }
    }
}

fn require_trimmed(value: &str, message: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("{message}");
    }
    Ok(trimmed.to_string())
}

fn ensure_source_branch_exists(project: &Project, branch: &str) -> Result<()> {
    let branches = core_git::list_branches(&project.repo_path)?;
    if !branches.iter().any(|candidate| candidate == branch) {
        bail!("Source branch \"{branch}\" does not exist");
    }
    Ok(())
}

fn ensure_new_branch_does_not_exist(project: &Project, branch: &str) -> Result<()> {
    if core_git::branch_exists(&project.repo_path, branch)? {
        bail!("Branch \"{branch}\" already exists");
    }
    Ok(())
}

fn ensure_target_branch_exists(project: &Project, branch: &str) -> Result<()> {
    if !core_git::branch_exists(&project.repo_path, branch)? {
        bail!("Branch \"{branch}\" does not exist");
    }
    Ok(())
}

async fn resolve_workspace_path(
    store: &RuntimeStore,
    project: &Project,
    display_name: &str,
    request: &ManagedWorkspaceCreateRequest,
) -> Result<String> {
    let explicit_path = request
        .path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let explicit_root = request
        .workspace_root
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if explicit_path.is_some() && explicit_root.is_some() {
        bail!("--path and --workspace-root cannot be used together");
    }
    if let Some(path) = explicit_path {
        return Ok(path.to_string());
    }
    let root = match explicit_root {
        Some(root) => root.to_string(),
        None => store
            .get_workspace_directory()
            .await?
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(default_workspace_root),
    };
    let project_slug = slugify(
        Path::new(&project.repo_path)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(&project.name),
    )?;
    let workspace_slug = slugify(display_name)?;
    Ok(PathBuf::from(root)
        .join(format!("{project_slug}-{}", project.id))
        .join(workspace_slug)
        .to_string_lossy()
        .to_string())
}

fn default_workspace_root() -> String {
    let home = std::env::var("HOME")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| {
            std::env::var("USERPROFILE")
                .ok()
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| ".".to_string());
    PathBuf::from(home)
        .join(".alera")
        .join("workspaces")
        .to_string_lossy()
        .to_string()
}

pub(crate) fn slugify(input: &str) -> Result<String> {
    let mut output = String::new();
    let mut last_dash = false;
    for ch in input.trim().to_lowercase().chars() {
        let next = if ch.is_ascii_alphanumeric() {
            last_dash = false;
            Some(ch)
        } else if ch.is_whitespace() || ch == '_' || ch == '/' || ch == '-' {
            if last_dash {
                None
            } else {
                last_dash = true;
                Some('-')
            }
        } else if last_dash {
            None
        } else {
            last_dash = true;
            Some('-')
        };
        if let Some(next) = next {
            output.push(next);
        }
    }
    let trimmed = output.trim_matches('-').to_string();
    if trimmed.is_empty() {
        bail!("Workspace name must contain a letter or digit");
    }
    Ok(trimmed)
}

fn path_equals(left: &str, right: &str) -> bool {
    let left = canonical_path(left);
    let right = canonical_path(right);
    left == right
}

fn canonical_path(path: &str) -> String {
    let target = Path::new(path);
    if let Ok(resolved) = std::fs::canonicalize(target) {
        return resolved.to_string_lossy().trim_end_matches('/').to_string();
    }
    if let (Some(parent), Some(name)) = (target.parent(), target.file_name()) {
        if let Ok(resolved_parent) = std::fs::canonicalize(parent) {
            return resolved_parent
                .join(name)
                .to_string_lossy()
                .trim_end_matches('/')
                .to_string();
        }
    }
    path.trim_end_matches('/').to_string()
}
