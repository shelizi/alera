//! Storage measurement and ownership validation for managed workspaces.

use std::path::{Path, PathBuf};

use alera_core::runtime::{Project, RuntimeStore, Workspace, WorkspaceKind, LOCAL_HOST_ID};
use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use serde::Serialize;

use super::{default_workspace_root, path_equals};
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceStorageImpact {
    pub workspace_id: String,
    pub path: String,
    pub size_bytes: u64,
    pub entry_count: u64,
    pub measured_at: chrono::DateTime<Utc>,
    pub last_activity_at: chrono::DateTime<Utc>,
    pub safe_to_clean: bool,
    pub blockers: Vec<String>,
}

/// Measures only the managed worktree itself. Directory links are counted as
/// entries and never followed, so measurement cannot escape into source repos
/// or network mounts through a workspace symlink.
pub async fn measure_workspace_storage(
    store: &RuntimeStore,
    workspace_id: &str,
    mut blockers: Vec<String>,
) -> Result<WorkspaceStorageImpact> {
    let workspace = store
        .find_workspace(workspace_id)
        .await?
        .ok_or_else(|| anyhow!("Workspace not found: {workspace_id}"))?;
    let project = store
        .find_project(&workspace.project_id)
        .await?
        .ok_or_else(|| anyhow!("Project not found: {}", workspace.project_id))?;
    if workspace.kind == WorkspaceKind::Main {
        blockers.push("The main workspace is a source repository".to_string());
    }
    let path = PathBuf::from(&workspace.path);
    if let Err(error) = validate_workspace_storage_ownership(store, &workspace, &project).await {
        blockers.push(error.to_string());
    }
    let updated_at = workspace.updated_at;
    let measured = tokio::task::spawn_blocking(move || measure_tree_without_following_links(&path))
        .await
        .context("workspace measurement task failed")??;
    Ok(WorkspaceStorageImpact {
        workspace_id: workspace.id,
        path: workspace.path,
        size_bytes: measured.0,
        entry_count: measured.1,
        measured_at: Utc::now(),
        last_activity_at: updated_at,
        safe_to_clean: blockers.is_empty(),
        blockers,
    })
}

#[cfg(test)]
pub async fn validate_workspace_storage_path(
    store: &RuntimeStore,
    workspace_id: &str,
) -> Result<()> {
    let workspace = store
        .find_workspace(workspace_id)
        .await?
        .ok_or_else(|| anyhow!("Workspace not found: {workspace_id}"))?;
    let project = store
        .find_project(&workspace.project_id)
        .await?
        .ok_or_else(|| anyhow!("Project not found: {}", workspace.project_id))?;
    if workspace.kind == WorkspaceKind::Main {
        bail!("The main workspace cannot be removed");
    }
    validate_workspace_storage_ownership(store, &workspace, &project).await
}

fn is_host_owned_workspace_path(root: &Path, target: &Path) -> Result<bool> {
    let root = match dunce::canonicalize(root) {
        Ok(root) => root,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error).context("Could not resolve managed workspace root"),
    };
    let metadata = match std::fs::symlink_metadata(target) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error).context("Could not inspect workspace path"),
    };
    if metadata
        .as_ref()
        .is_some_and(|metadata| metadata.file_type().is_symlink())
    {
        return Ok(false);
    }
    let resolved = match dunce::canonicalize(target) {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let Some(parent) = target.parent() else {
                return Ok(false);
            };
            let Some(name) = target.file_name() else {
                return Ok(false);
            };
            match dunce::canonicalize(parent) {
                Ok(parent) => parent.join(name),
                Err(_) => return Ok(false),
            }
        }
        Err(error) => return Err(error).context("Could not resolve workspace path"),
    };
    Ok(resolved != root && resolved.starts_with(root))
}

fn measure_tree_without_following_links(root: &Path) -> Result<(u64, u64)> {
    let metadata = match std::fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok((0, 0)),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() {
        return Ok((metadata.len(), 1));
    }
    let mut bytes = metadata.len();
    let mut entries = 1;
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            let metadata = entry.file_type().and_then(|kind| {
                if kind.is_symlink() {
                    std::fs::symlink_metadata(entry.path())
                } else {
                    entry.metadata()
                }
            })?;
            entries += 1;
            bytes = bytes.saturating_add(metadata.len());
            if metadata.is_dir() && !metadata.file_type().is_symlink() {
                pending.push(entry.path());
            }
        }
    }
    Ok((bytes, entries))
}

pub(super) async fn validate_workspace_storage_ownership(
    store: &RuntimeStore,
    workspace: &Workspace,
    project: &Project,
) -> Result<()> {
    if workspace.host_id != LOCAL_HOST_ID {
        bail!("Workspace is not owned by the local host");
    }
    let root = store
        .get_workspace_directory()
        .await?
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(default_workspace_root);
    if path_equals(&workspace.path, &project.repo_path)
        || !is_host_owned_workspace_path(Path::new(&root), Path::new(&workspace.path))?
    {
        bail!("Workspace path is outside Alera-managed storage");
    }
    if store
        .list_projects()
        .await?
        .iter()
        .any(|candidate| path_equals(&candidate.repo_path, &workspace.path))
    {
        bail!("Workspace path is registered as a project source repository");
    }
    if store.list_all_workspaces().await?.iter().any(|candidate| {
        candidate.id != workspace.id && path_equals(&candidate.path, &workspace.path)
    }) {
        bail!("Workspace path has another runtime owner");
    }
    Ok(())
}
