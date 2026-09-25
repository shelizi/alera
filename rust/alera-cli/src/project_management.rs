use std::path::{Path, PathBuf};

use alera_core::{
    git as core_git,
    runtime::{
        Project, ProjectConfig, ProjectKind, RuntimeStore, Workspace, WorkspaceKind,
        WorkspaceStatus, LOCAL_HOST_ID,
    },
};
use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRegistration {
    pub project: Project,
    pub main_workspace: Workspace,
    pub created: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostDirectoryRoot {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostDirectoryEntry {
    pub name: String,
    pub path: String,
    pub is_symlink: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostDirectoryListing {
    pub path: String,
    pub parent_path: Option<String>,
    pub entries: Vec<HostDirectoryEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveProjectConfigPayload {
    pub config: ProjectConfig,
    pub origin: &'static str,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PreparedProjectRegistration {
    pub canonical_path: String,
    pub name: String,
    pub branch: Option<String>,
    pub kind: ProjectKind,
}

pub async fn register_project(
    store: &RuntimeStore,
    raw_path: &str,
    requested_name: Option<&str>,
) -> Result<ProjectRegistration> {
    let prepared = prepare_project_registration(raw_path, requested_name)?;
    commit_project_registration(store, prepared).await
}

pub fn prepare_project_registration(
    raw_path: &str,
    requested_name: Option<&str>,
) -> Result<PreparedProjectRegistration> {
    let path = validate_existing_directory(raw_path)?;
    let canonical_path = canonical_string(&path)?;
    let branch = core_git::current_branch(&canonical_path).ok();
    let kind = if branch.is_some() || Path::new(&canonical_path).join(".git").exists() {
        ProjectKind::GitRepository
    } else {
        ProjectKind::Folder
    };
    let name = normalized_project_name(requested_name, &path)?;
    Ok(PreparedProjectRegistration {
        canonical_path,
        name,
        branch,
        kind,
    })
}

pub async fn commit_project_registration(
    store: &RuntimeStore,
    prepared: PreparedProjectRegistration,
) -> Result<ProjectRegistration> {
    for project in store.list_projects().await? {
        if paths_equal(&project.repo_path, &prepared.canonical_path) {
            let branch = (project.kind == ProjectKind::GitRepository)
                .then(|| prepared.branch.as_deref())
                .flatten();
            let main_workspace = ensure_main_workspace(store, &project, branch).await?;
            return Ok(ProjectRegistration {
                project,
                main_workspace,
                created: false,
            });
        }
    }

    let PreparedProjectRegistration {
        canonical_path,
        name,
        branch,
        kind,
    } = prepared;
    let main_workspace_name = default_main_workspace_name(branch.as_deref(), &name);
    let now = Utc::now();
    let project = Project {
        id: Uuid::new_v4().to_string(),
        name: name.clone(),
        repo_path: canonical_path.clone(),
        created_at: now,
        updated_at: now,
        kind,
    };
    let main_workspace = Workspace {
        id: Uuid::new_v4().to_string(),
        instance_id: Uuid::new_v4().to_string(),
        host_id: LOCAL_HOST_ID.to_string(),
        project_id: project.id.clone(),
        name: main_workspace_name,
        branch,
        path: canonical_path,
        created_at: now,
        updated_at: now,
        kind: WorkspaceKind::Main,
        status: WorkspaceStatus::Active,
        source_branch: None,
        reuses_existing_branch: false,
        is_pinned: false,
        tag_ids: Vec::new(),
        tag_names: Vec::new(),
        parent_workspace_id: None,
        section_id: None,
        child_count: 0,
        archived_at: None,
    };
    store.upsert_project(project.clone()).await?;
    if let Err(error) = store.upsert_workspace(main_workspace.clone()).await {
        let _ = store.remove_project(&project.id).await;
        return Err(error);
    }
    Ok(ProjectRegistration {
        project,
        main_workspace,
        created: true,
    })
}

pub async fn rename_project(store: &RuntimeStore, id: &str, name: &str) -> Result<Project> {
    let name = name.trim();
    if name.is_empty() {
        bail!("Project name cannot be empty.");
    }
    let mut project = store
        .find_project(id)
        .await?
        .ok_or_else(|| anyhow!("Project not found: {id}"))?;
    project.name = name.to_string();
    project.updated_at = Utc::now();
    store.upsert_project(project).await
}

pub fn host_directory_roots() -> Vec<HostDirectoryRoot> {
    let mut roots = Vec::new();
    if let Some(home) = dirs::home_dir() {
        roots.push(HostDirectoryRoot {
            name: "Home".to_string(),
            path: home.to_string_lossy().to_string(),
        });
    }
    #[cfg(windows)]
    {
        for letter in b'A'..=b'Z' {
            let path = format!("{}:\\", letter as char);
            if Path::new(&path).is_dir() {
                roots.push(HostDirectoryRoot {
                    name: format!("{}:", letter as char),
                    path,
                });
            }
        }
    }
    #[cfg(not(windows))]
    roots.push(HostDirectoryRoot {
        name: "File System".to_string(),
        path: "/".to_string(),
    });
    roots.dedup_by(|left, right| paths_equal(&left.path, &right.path));
    roots
}

pub fn list_host_directory(raw_path: &str) -> Result<HostDirectoryListing> {
    let path = validate_existing_directory(raw_path)?;
    let canonical = dunce::canonicalize(&path)
        .with_context(|| format!("Could not open directory: {}", path.display()))?;
    let mut entries = Vec::new();
    for item in std::fs::read_dir(&canonical)
        .with_context(|| format!("Could not read directory: {}", canonical.display()))?
    {
        let item = item?;
        let file_type = item.file_type()?;
        if !file_type.is_dir() && !file_type.is_symlink() {
            continue;
        }
        let item_path = item.path();
        if file_type.is_symlink() && !item_path.is_dir() {
            continue;
        }
        entries.push(HostDirectoryEntry {
            name: item.file_name().to_string_lossy().to_string(),
            path: item_path.to_string_lossy().to_string(),
            is_symlink: file_type.is_symlink(),
        });
    }
    entries.sort_by_key(|entry| entry.name.to_lowercase());
    Ok(HostDirectoryListing {
        path: canonical.to_string_lossy().to_string(),
        parent_path: canonical
            .parent()
            .filter(|parent| *parent != canonical)
            .map(|parent| parent.to_string_lossy().to_string()),
        entries,
    })
}

pub async fn effective_project_config(
    store: &RuntimeStore,
    project_id: &str,
) -> Result<EffectiveProjectConfigPayload> {
    let project = store
        .find_project(project_id)
        .await?
        .ok_or_else(|| anyhow!("Project not found: {project_id}"))?;
    if let Some(config) = store.find_project_config(project_id).await? {
        return Ok(EffectiveProjectConfigPayload {
            config,
            origin: "uiOverride",
            error: None,
        });
    }
    let config_path = Path::new(&project.repo_path).join("alera.toml");
    let contents = match tokio::fs::read_to_string(&config_path).await {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(EffectiveProjectConfigPayload {
                config: ProjectConfig::default(),
                origin: "none",
                error: None,
            });
        }
        result => result.with_context(|| format!("Could not load {}", config_path.display())),
    };
    let parsed = contents
        .and_then(|contents| crate::project_config_toml::parse_project_config_toml(&contents));
    match parsed {
        Ok(config) => Ok(EffectiveProjectConfigPayload {
            config,
            origin: "repoFile",
            error: None,
        }),
        Err(error) => Ok(EffectiveProjectConfigPayload {
            config: ProjectConfig::default(),
            origin: "repoFile",
            error: Some(error.to_string()),
        }),
    }
}

pub fn validate_clone_destination(parent_path: &str, directory_name: &str) -> Result<PathBuf> {
    let parent = validate_existing_directory(parent_path)?;
    let name = directory_name.trim();
    if name.is_empty() || name == "." || name == ".." || Path::new(name).components().count() != 1 {
        bail!("Clone directory name must be a single path segment.");
    }
    let parent = dunce::canonicalize(parent)?;
    let destination = parent.join(name);
    if destination.exists() {
        bail!(
            "Clone destination already exists: {}",
            destination.display()
        );
    }
    Ok(destination)
}

pub fn remove_owned_clone_destination(parent_path: &str, destination_path: &str) -> Result<()> {
    let parent = dunce::canonicalize(parent_path)?;
    let destination = PathBuf::from(destination_path);
    if destination.parent() != Some(parent.as_path()) || destination.file_name().is_none() {
        bail!("Refusing to clean an unsafe clone destination.");
    }
    if let Ok(metadata) = std::fs::symlink_metadata(&destination) {
        if metadata.file_type().is_symlink() || metadata.is_file() {
            std::fs::remove_file(destination)?;
        } else if metadata.is_dir() {
            std::fs::remove_dir_all(destination)?;
        }
    }
    Ok(())
}

fn validate_existing_directory(raw_path: &str) -> Result<PathBuf> {
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        bail!("Project path cannot be empty.");
    }
    let path = PathBuf::from(trimmed);
    let metadata = std::fs::metadata(&path)
        .with_context(|| format!("Directory does not exist or is not accessible: {trimmed}"))?;
    if !metadata.is_dir() {
        bail!("Path is not a directory: {trimmed}");
    }
    Ok(path)
}

async fn ensure_main_workspace(
    store: &RuntimeStore,
    project: &Project,
    prepared_branch: Option<&str>,
) -> Result<Workspace> {
    if let Some(workspace) = store
        .list_workspaces(&project.id)
        .await?
        .into_iter()
        .find(|workspace| workspace.kind == WorkspaceKind::Main)
    {
        return Ok(workspace);
    }
    let now = Utc::now();
    let branch = (project.kind == ProjectKind::GitRepository)
        .then(|| prepared_branch.map(str::to_string))
        .flatten();
    store
        .upsert_workspace(Workspace {
            id: Uuid::new_v4().to_string(),
            instance_id: Uuid::new_v4().to_string(),
            host_id: LOCAL_HOST_ID.to_string(),
            project_id: project.id.clone(),
            name: default_main_workspace_name(branch.as_deref(), &project.name),
            branch,
            path: project.repo_path.clone(),
            created_at: now,
            updated_at: now,
            kind: WorkspaceKind::Main,
            status: WorkspaceStatus::Active,
            source_branch: None,
            reuses_existing_branch: false,
            is_pinned: false,
            tag_ids: Vec::new(),
            tag_names: Vec::new(),
            parent_workspace_id: None,
            section_id: None,
            child_count: 0,
            archived_at: None,
        })
        .await
}

fn default_main_workspace_name(branch: Option<&str>, project_name: &str) -> String {
    branch
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "HEAD")
        .unwrap_or(project_name)
        .to_string()
}

fn normalized_project_name(requested_name: Option<&str>, path: &Path) -> Result<String> {
    let requested = requested_name.unwrap_or_default().trim();
    if !requested.is_empty() {
        return Ok(requested.to_string());
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| anyhow!("Project name cannot be derived from the selected path."))
}

// `std::fs::canonicalize` returns the verbatim `\\?\E:\...` form on Windows,
// which every consumer of a stored repo path (Dart path comparisons, child
// process cwds, cmd.exe) would otherwise have to special-case. `dunce` keeps
// the plain form whenever it is representable and is `std` elsewhere.
fn canonical_string(path: &Path) -> Result<String> {
    Ok(dunce::canonicalize(path)?.to_string_lossy().to_string())
}

fn paths_equal(left: &str, right: &str) -> bool {
    #[cfg(windows)]
    {
        // Rows registered before `canonical_string` used `dunce` still carry
        // the verbatim prefix and must match the plain form of the same path.
        let left = dunce::simplified(Path::new(left));
        let right = dunce::simplified(Path::new(right));
        left.as_os_str().eq_ignore_ascii_case(right.as_os_str())
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

#[cfg(test)]
#[path = "project_management_tests.rs"]
mod tests;
