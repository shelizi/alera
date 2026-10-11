use std::path::{Component, Path, PathBuf};

use git2::{Delta, DiffFormat, DiffOptions, Index, Oid, Repository, Status, StatusOptions};

use alera_core::runtime::{RuntimeStore, Workspace};
use alera_core::workspace_files::{
    is_protected_workspace_path, list_workspace_directory_from_root, open_workspace_file_root,
    read_workspace_file_range_from_root, search_workspace_quick_open_session,
    start_workspace_quick_open_session_without_symlinks, stop_workspace_quick_open_session,
    write_workspace_text_from_root, WorkspaceFileErrorKind, WorkspaceFileRoot,
    WorkspaceQuickOpenSession, DEFAULT_QUICK_OPEN_EXCLUDED_DIRECTORIES, MAX_REMOTE_EDIT_BYTES,
};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde_json::{json, Value};

use crate::terminal_host::host_error::{HostError, HostResult};
use crate::terminal_host::protocol::{error_response, ok_response};

use super::mobile_workspace_file_paths::prompt_attachment_root;
use super::request_route_policy::MobileWorkspaceFileOperation;
use super::requests::{optional_string_key, require_string_key};
use super::{ServerActor, ServerCommand};

impl ServerActor {
    pub(super) fn start_mobile_workspace_file_request(
        &self,
        client_id: u64,
        request_id: i64,
        operation: MobileWorkspaceFileOperation,
        request_type: &str,
        payload: &Value,
    ) -> HostResult<()> {
        let runtime_store = self.runtime_store.clone();
        let runtime_dir = self.runtime_dir.clone();
        let payload = payload.clone();
        let inbox = self.inbox.clone();
        self.deferred_admission.schedule(
            super::deferred_admission::DeferredRequestClass::Bulk,
            request_type,
            Some(client_id),
            async move {
                let result = handle_mobile_workspace_file_request(
                    runtime_store,
                    runtime_dir,
                    operation,
                    &payload,
                )
                .await;
                let _ = inbox.send(ServerCommand::MobileWorkspaceFileFinished {
                    client_id,
                    request_id,
                    operation,
                    result,
                });
            },
        )?;
        Ok(())
    }

    pub(super) fn handle_mobile_workspace_file_finished(
        &self,
        client_id: u64,
        request_id: i64,
        operation: MobileWorkspaceFileOperation,
        result: HostResult<Value>,
    ) {
        if !self.clients.contains_key(&client_id) {
            cleanup_orphaned_workspace_file_result(operation, &result);
            return;
        }
        let response = match &result {
            Ok(value) => ok_response(request_id, value.clone()),
            Err(error) => error_response(request_id, error),
        };
        if !self.try_client_write(client_id, response) {
            cleanup_orphaned_workspace_file_result(operation, &result);
        }
    }

    pub(super) fn stop_mobile_workspace_quick_open(&self, payload: &Value) -> HostResult<Value> {
        let session_id = require_string_key(payload, "sessionId")?;
        stop_workspace_quick_open_session(WorkspaceQuickOpenSession {
            id: session_id,
            indexed_file_count: 0,
        });
        Ok(json!({}))
    }
}

async fn handle_mobile_workspace_file_request(
    runtime_store: RuntimeStore,
    runtime_dir: PathBuf,
    operation: MobileWorkspaceFileOperation,
    payload: &Value,
) -> HostResult<Value> {
    match operation {
        MobileWorkspaceFileOperation::QuickOpenStart => {
            let workspace = workspace_for_mobile_file_request(&runtime_store, payload).await?;
            let root = mobile_workspace_file_root(&runtime_store, payload, &workspace).await?;
            spawn_blocking_workspace("Quick Open indexing", move || {
                start_workspace_quick_open_session_without_symlinks(
                    root,
                    DEFAULT_QUICK_OPEN_EXCLUDED_DIRECTORIES
                        .iter()
                        .map(|value| (*value).to_string())
                        .collect(),
                )
                .map_err(workspace_file_error)
                .map(|session| {
                    json!({
                        "sessionId": session.id,
                        "indexedFileCount": session.indexed_file_count,
                    })
                })
            })
            .await
        }
        MobileWorkspaceFileOperation::QuickOpenSearch => {
            search_mobile_workspace_quick_open(payload).await
        }
        MobileWorkspaceFileOperation::WorkspaceDirectoryList => {
            list_mobile_workspace_directory(&runtime_store, payload).await
        }
        MobileWorkspaceFileOperation::WorkspaceFileRead => {
            read_mobile_workspace_file(&runtime_store, payload).await
        }
        MobileWorkspaceFileOperation::WorkspaceFileWrite => {
            write_mobile_workspace_text(&runtime_store, payload).await
        }
        MobileWorkspaceFileOperation::WorkspaceFileDiff => {
            read_mobile_workspace_diff(&runtime_store, payload).await
        }
        MobileWorkspaceFileOperation::WorkspaceGitStatus => {
            read_mobile_workspace_git_status(&runtime_store, payload).await
        }
        MobileWorkspaceFileOperation::WorkspaceGitStage => {
            update_mobile_workspace_git_stage(&runtime_store, payload).await
        }
        MobileWorkspaceFileOperation::WorkspaceGitHistory => {
            read_mobile_workspace_git_history(&runtime_store, payload).await
        }
        MobileWorkspaceFileOperation::WorkspaceGitCompare => {
            read_mobile_workspace_git_comparison(&runtime_store, payload).await
        }
        MobileWorkspaceFileOperation::WorkspaceGitRevisionPatch => {
            read_mobile_workspace_git_revision_patch(&runtime_store, payload).await
        }
        MobileWorkspaceFileOperation::PromptAttachmentRead => {
            read_mobile_prompt_attachment(runtime_dir, payload).await
        }
    }
}

fn cleanup_orphaned_workspace_file_result(
    operation: MobileWorkspaceFileOperation,
    result: &HostResult<Value>,
) {
    if operation != MobileWorkspaceFileOperation::QuickOpenStart {
        return;
    }
    if let Some(session_id) = result
        .as_ref()
        .ok()
        .and_then(|value| value.get("sessionId"))
        .and_then(Value::as_str)
    {
        stop_workspace_quick_open_session(WorkspaceQuickOpenSession {
            id: session_id.to_string(),
            indexed_file_count: 0,
        });
    }
}

async fn search_mobile_workspace_quick_open(payload: &Value) -> HostResult<Value> {
    let session_id = require_string_key(payload, "sessionId")?;
    let indexed_file_count = payload
        .get("indexedFileCount")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0);
    let query = optional_string_key(payload, "query").unwrap_or_default();
    let limit = payload
        .get("limit")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(20)
        .min(100);
    spawn_blocking_workspace("Quick Open search", move || {
        search_workspace_quick_open_session(
            WorkspaceQuickOpenSession {
                id: session_id,
                indexed_file_count,
            },
            query,
            limit,
            false,
        )
        .map_err(workspace_file_error)
        .map(|matches| {
            json!({
                "items": matches.into_iter().map(|item| json!({
                    "relativePath": item.relative_path,
                    "score": item.score,
                })).collect::<Vec<_>>(),
            })
        })
    })
    .await
}

async fn list_mobile_workspace_directory(
    runtime_store: &RuntimeStore,
    payload: &Value,
) -> HostResult<Value> {
    let workspace = workspace_for_mobile_file_request(runtime_store, payload).await?;
    let directory = optional_string_key(payload, "relativePath").unwrap_or_default();
    let offset = payload
        .get("offset")
        .and_then(Value::as_u64)
        .and_then(|v| usize::try_from(v).ok())
        .unwrap_or(0);
    let limit = payload
        .get("limit")
        .and_then(Value::as_u64)
        .and_then(|v| usize::try_from(v).ok())
        .unwrap_or(100);
    let show_hidden = payload.get("showHidden") == Some(&Value::Bool(true));
    let roots = known_workspace_paths(runtime_store).await?;
    spawn_blocking_workspace("Workspace directory listing", move || {
        let root = open_validated_mobile_workspace_root(&workspace.path, roots)?;
        let page =
            list_workspace_directory_from_root(&root, &directory, offset, limit, show_hidden)
                .map_err(workspace_file_error)?;
        Ok(json!({
            "items": page.entries.iter().map(|entry| json!({
                "name": entry.name,
                "relativePath": entry.relative_path,
                "isDirectory": entry.is_directory,
                "sizeBytes": entry.size_bytes,
            })).collect::<Vec<_>>(),
            "hasMore": page.has_more,
            "nextOffset": offset + page.entries.len(),
        }))
    })
    .await
}

async fn write_mobile_workspace_text(
    runtime_store: &RuntimeStore,
    payload: &Value,
) -> HostResult<Value> {
    let workspace = workspace_for_mobile_file_request(runtime_store, payload).await?;
    let path = require_string_key(payload, "relativePath")?;
    let expected_sha256 = require_string_key(payload, "expectedSha256")?;
    // Empty content is valid when clearing an existing text file.
    let new_text = payload
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| HostError::format("Text content must be a string"))?
        .to_string();
    if new_text.len() > MAX_REMOTE_EDIT_BYTES {
        return Err(HostError::state("Mobile edits are limited to 256 KiB"));
    }
    let roots = known_workspace_paths(runtime_store).await?;
    spawn_blocking_workspace("Workspace text save", move || {
        let root = open_validated_mobile_workspace_root(&workspace.path, roots)?;
        let written = write_workspace_text_from_root(&root, &path, &expected_sha256, &new_text)
            .map_err(|error| {
                if error.kind == WorkspaceFileErrorKind::Conflict {
                    HostError::conflict(
                        "mobile_workspace_file_changed",
                        error.to_string(),
                        json!({ "relativePath": path }),
                    )
                } else {
                    workspace_file_error(error)
                }
            })?;
        Ok(json!({
            "relativePath": path,
            "sha256": written.sha256,
            "sizeBytes": written.size_bytes,
        }))
    })
    .await
}

async fn read_mobile_workspace_file(
    runtime_store: &RuntimeStore,
    payload: &Value,
) -> HostResult<Value> {
    let workspace = workspace_for_mobile_file_request(runtime_store, payload).await?;
    let requested_path = require_string_key(payload, "relativePath")?;
    let candidate_root =
        optional_string_key(payload, "cwd").unwrap_or_else(|| workspace.path.clone());
    let known_roots = known_workspace_paths(runtime_store).await?;
    let offset = payload.get("offset").and_then(Value::as_u64).unwrap_or(0);
    let length = payload
        .get("length")
        .and_then(Value::as_u64)
        .unwrap_or(alera_core::workspace_files::MAX_REMOTE_READ_BYTES);
    spawn_blocking_workspace("Workspace file read", move || {
        let (root, relative_path) = if Path::new(&requested_path).is_absolute() {
            absolute_workspace_file_target(&requested_path, known_roots)?
        } else {
            (
                open_validated_mobile_workspace_root(&candidate_root, known_roots)?,
                requested_path,
            )
        };
        let range = read_workspace_file_range_from_root(&root, &relative_path, offset, length)
            .map_err(workspace_file_error)?;
        Ok(workspace_range_response(relative_path, range))
    })
    .await
}

const MAX_MOBILE_GIT_DIFF_BYTES: usize = 512 * 1024;

async fn read_mobile_workspace_diff(
    runtime_store: &RuntimeStore,
    payload: &Value,
) -> HostResult<Value> {
    let workspace = workspace_for_mobile_file_request(runtime_store, payload).await?;
    let requested_path = require_string_key(payload, "relativePath")?;
    let known_roots = known_workspace_paths(runtime_store).await?;
    spawn_blocking_workspace("Workspace Git diff", move || {
        let root = open_validated_mobile_workspace_root(&workspace.path, known_roots)?;
        workspace_git_patch(root.canonical_path(), &requested_path)
    })
    .await
}

fn workspace_git_patch(root: &Path, relative_path: &str) -> HostResult<Value> {
    let path = Path::new(relative_path);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || is_protected_workspace_path(path)
    {
        return Err(HostError::state("Workspace Git diff path is invalid."));
    }
    let requested = root.join(path);
    if requested.exists() {
        let canonical = dunce::canonicalize(&requested)
            .map_err(|_| HostError::state("Workspace Git diff path is unavailable."))?;
        if !canonical.starts_with(root) {
            return Err(HostError::state(
                "Workspace Git diff cannot leave the workspace.",
            ));
        }
    }
    let repo = Repository::open(root)
        .map_err(|_| HostError::state("Workspace is not a Git repository."))?;
    let actual_root = repo
        .workdir()
        .and_then(|directory| dunce::canonicalize(directory).ok())
        .ok_or_else(|| HostError::state("Workspace Git worktree is unavailable."))?;
    if actual_root != root {
        return Err(HostError::state(
            "Workspace Git root is not the selected workspace.",
        ));
    }
    let tree = repo
        .head()
        .and_then(|head| head.peel_to_tree())
        .map_err(|_| HostError::state("Workspace has no Git HEAD to compare."))?;
    let mut options = DiffOptions::new();
    options.pathspec(relative_path).disable_pathspec_match(true);
    let diff = repo
        .diff_tree_to_workdir_with_index(Some(&tree), Some(&mut options))
        .map_err(|error| HostError::state(format!("Git diff failed: {error}")))?;
    let mut patch = Vec::new();
    let mut truncated = false;
    diff.print(DiffFormat::Patch, |_delta, _hunk, line| {
        // libgit2 provides the line bytes without the patch prefix. Preserve
        // it explicitly for additions, removals and unchanged context lines.
        let prefix = line.origin();
        if matches!(prefix, '+' | '-' | ' ') {
            if patch.len() < MAX_MOBILE_GIT_DIFF_BYTES {
                patch.push(prefix as u8);
            } else {
                truncated = true;
            }
        }
        let available = MAX_MOBILE_GIT_DIFF_BYTES.saturating_sub(patch.len());
        let content = line.content();
        patch.extend_from_slice(&content[..content.len().min(available)]);
        if content.len() > available {
            truncated = true;
        }
        true
    })
    .map_err(|error| HostError::state(format!("Git diff failed: {error}")))?;
    Ok(json!({
        "relativePath": relative_path,
        "patch": String::from_utf8_lossy(&patch),
        "truncated": truncated,
    }))
}

const MAX_MOBILE_GIT_STATUS_ITEMS: usize = 200;

async fn read_mobile_workspace_git_status(
    runtime_store: &RuntimeStore,
    payload: &Value,
) -> HostResult<Value> {
    let workspace = workspace_for_mobile_file_request(runtime_store, payload).await?;
    let known_roots = known_workspace_paths(runtime_store).await?;
    spawn_blocking_workspace("Workspace Git status", move || {
        let root = open_validated_mobile_workspace_root(&workspace.path, known_roots)?;
        workspace_git_status(root.canonical_path())
    })
    .await
}

fn workspace_git_status(root: &Path) -> HostResult<Value> {
    let repo = Repository::open(root)
        .map_err(|_| HostError::state("Workspace is not a Git repository."))?;
    let workdir = repo
        .workdir()
        .and_then(|directory| dunce::canonicalize(directory).ok())
        .ok_or_else(|| HostError::state("Workspace Git worktree is unavailable."))?;
    if workdir != root {
        return Err(HostError::state(
            "Workspace Git root is not the selected workspace.",
        ));
    }
    let mut options = StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .include_unmodified(false);
    let statuses = repo
        .statuses(Some(&mut options))
        .map_err(|error| HostError::state(format!("Git status failed: {error}")))?;
    let mut items = Vec::new();
    for entry in statuses.iter() {
        let Ok(path) = entry.path() else { continue };
        if is_protected_workspace_path(Path::new(path)) {
            continue;
        }
        let status = entry.status();
        let label = if status.contains(Status::CONFLICTED) {
            "conflict"
        } else if status.intersects(Status::INDEX_DELETED | Status::WT_DELETED) {
            "deleted"
        } else if status.intersects(Status::INDEX_NEW | Status::WT_NEW) {
            "added"
        } else if status.intersects(Status::INDEX_RENAMED | Status::WT_RENAMED) {
            "renamed"
        } else {
            "modified"
        };
        let staged = status.intersects(
            Status::INDEX_NEW
                | Status::INDEX_MODIFIED
                | Status::INDEX_DELETED
                | Status::INDEX_RENAMED
                | Status::INDEX_TYPECHANGE,
        );
        let unstaged = status.intersects(
            Status::WT_NEW
                | Status::WT_MODIFIED
                | Status::WT_DELETED
                | Status::WT_RENAMED
                | Status::WT_TYPECHANGE
                | Status::CONFLICTED,
        );
        items.push(json!({
            "relativePath": path,
            "status": label,
            "staged": staged,
            "unstaged": unstaged,
        }));
        if items.len() > MAX_MOBILE_GIT_STATUS_ITEMS {
            break;
        }
    }
    let truncated = items.len() > MAX_MOBILE_GIT_STATUS_ITEMS;
    items.truncate(MAX_MOBILE_GIT_STATUS_ITEMS);
    Ok(json!({"items": items, "truncated": truncated}))
}

async fn update_mobile_workspace_git_stage(
    runtime_store: &RuntimeStore,
    payload: &Value,
) -> HostResult<Value> {
    let workspace = workspace_for_mobile_file_request(runtime_store, payload).await?;
    let path = require_string_key(payload, "relativePath")?;
    let stage = payload
        .get("stage")
        .and_then(Value::as_bool)
        .ok_or_else(|| HostError::format("Git staging action must be a boolean"))?;
    let roots = known_workspace_paths(runtime_store).await?;
    spawn_blocking_workspace("Workspace Git staging", move || {
        let root = open_validated_mobile_workspace_root(&workspace.path, roots)?;
        mutate_mobile_git_stage(root.canonical_path(), &path, stage)
    })
    .await
}

fn mutate_mobile_git_stage(root: &Path, relative_path: &str, stage: bool) -> HostResult<Value> {
    let path = Path::new(relative_path);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        || is_protected_workspace_path(path)
    {
        return Err(HostError::state(
            "Git stage path must be one workspace-relative file.",
        ));
    }
    let repo = Repository::open(root)
        .map_err(|_| HostError::state("Workspace is not a Git repository."))?;
    let workdir = repo
        .workdir()
        .and_then(|directory| dunce::canonicalize(directory).ok())
        .ok_or_else(|| HostError::state("Git worktree is unavailable."))?;
    if workdir != root {
        return Err(HostError::state(
            "Git root does not match the selected workspace.",
        ));
    }
    let status = repo
        .status_file(path)
        .map_err(|_| HostError::state("Git stage path is not a changed file."))?;
    if status.contains(Status::CONFLICTED)
        || status.intersects(Status::INDEX_RENAMED | Status::WT_RENAMED)
    {
        return Err(HostError::state(
            "Conflicted or renamed paths need desktop Git review first.",
        ));
    }
    let mut index = repo
        .index()
        .map_err(|error| HostError::state(format!("Git index failed: {error}")))?;
    if stage {
        let has_worktree_changes = status.intersects(
            Status::WT_NEW | Status::WT_MODIFIED | Status::WT_DELETED | Status::WT_TYPECHANGE,
        );
        if !has_worktree_changes {
            return Err(HostError::state("There are no new file changes to stage."));
        }
        if status.contains(Status::WT_DELETED) {
            if index.get_path(path, 0).is_none() {
                return Err(HostError::state("Cannot stage a deleted untracked file."));
            }
            index
                .remove_path(path)
                .map_err(|error| HostError::state(format!("Git stage failed: {error}")))?;
        } else {
            // Use the same no-follow file verification as the file browser.
            // Do not let an untrusted mobile path add a symlink or directory.
            let handle =
                open_workspace_file_root(&root.to_string_lossy()).map_err(workspace_file_error)?;
            read_workspace_file_range_from_root(&handle, relative_path, 0, 1)
                .map_err(workspace_file_error)?;
            index
                .add_path(path)
                .map_err(|error| HostError::state(format!("Git stage failed: {error}")))?;
        }
    } else {
        if !status.intersects(
            Status::INDEX_NEW
                | Status::INDEX_MODIFIED
                | Status::INDEX_DELETED
                | Status::INDEX_TYPECHANGE,
        ) {
            return Err(HostError::state("There are no staged changes to unstage."));
        }
        let head = repo.head().ok().and_then(|head| head.peel_to_tree().ok());
        let mut head_index = Index::new()
            .map_err(|error| HostError::state(format!("Git HEAD index failed: {error}")))?;
        if let Some(tree) = head.as_ref() {
            head_index
                .read_tree(tree)
                .map_err(|error| HostError::state(format!("Git HEAD index failed: {error}")))?;
        }
        if let Some(entry) = head_index.get_path(path, 0) {
            index
                .add(&entry)
                .map_err(|error| HostError::state(format!("Git unstage failed: {error}")))?;
        } else if index.get_path(path, 0).is_some() {
            index
                .remove_path(path)
                .map_err(|error| HostError::state(format!("Git unstage failed: {error}")))?;
        }
    }
    index
        .write()
        .map_err(|error| HostError::state(format!("Git index write failed: {error}")))?;
    Ok(json!({"relativePath": relative_path, "staged": stage}))
}

const MAX_MOBILE_GIT_HISTORY_COMMITS: usize = 100;

async fn read_mobile_workspace_git_history(
    runtime_store: &RuntimeStore,
    payload: &Value,
) -> HostResult<Value> {
    let workspace = workspace_for_mobile_file_request(runtime_store, payload).await?;
    let limit = payload
        .get("limit")
        .and_then(Value::as_u64)
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(40);
    if limit == 0 || limit > MAX_MOBILE_GIT_HISTORY_COMMITS {
        return Err(HostError::format(
            "Git history limit must be between 1 and 100",
        ));
    }
    let roots = known_workspace_paths(runtime_store).await?;
    spawn_blocking_workspace("Workspace Git history", move || {
        let root = open_validated_mobile_workspace_root(&workspace.path, roots)?;
        workspace_git_history(root.canonical_path(), limit)
    })
    .await
}

fn workspace_git_history(root: &Path, limit: usize) -> HostResult<Value> {
    let repo = Repository::open(root)
        .map_err(|_| HostError::state("Workspace is not a Git repository."))?;
    let workdir = repo
        .workdir()
        .and_then(|directory| dunce::canonicalize(directory).ok())
        .ok_or_else(|| HostError::state("Git worktree is unavailable."))?;
    if workdir != root {
        return Err(HostError::state("Git root is not the selected workspace."));
    }
    let reference = repo
        .head()
        .map_err(|_| HostError::state("Workspace has no Git commits."))?;
    let branch = reference.shorthand().unwrap_or("HEAD").to_string();
    let mut walk = repo
        .revwalk()
        .map_err(|error| HostError::state(format!("Git history failed: {error}")))?;
    walk.push_head()
        .map_err(|error| HostError::state(format!("Git history failed: {error}")))?;
    walk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)
        .map_err(|error| HostError::state(format!("Git history failed: {error}")))?;
    let mut commits = Vec::new();
    for id in walk.take(limit) {
        let id = id.map_err(|error| HostError::state(format!("Git history failed: {error}")))?;
        let commit = repo
            .find_commit(id)
            .map_err(|error| HostError::state(format!("Git history failed: {error}")))?;
        commits.push(json!({
            "id": id.to_string(),
            "summary": commit.summary().ok().flatten().unwrap_or("").chars().take(240).collect::<String>(),
            "message": commit.message().unwrap_or("").chars().take(8192).collect::<String>(),
            "author": commit.author().name().unwrap_or("").chars().take(180).collect::<String>(),
            "timestamp": commit.time().seconds(),
            "parents": commit.parent_ids().map(|parent| parent.to_string()).take(8).collect::<Vec<_>>(),
        }));
    }
    Ok(json!({"branch": branch, "commits": commits}))
}

const MAX_MOBILE_GIT_COMPARE_ITEMS: usize = 200;

async fn read_mobile_workspace_git_comparison(
    runtime_store: &RuntimeStore,
    payload: &Value,
) -> HostResult<Value> {
    let workspace = workspace_for_mobile_file_request(runtime_store, payload).await?;
    let target_commit = require_string_key(payload, "targetCommit")?;
    let base_commit = optional_string_key(payload, "baseCommit");
    let roots = known_workspace_paths(runtime_store).await?;
    spawn_blocking_workspace("Workspace Git commit comparison", move || {
        let root = open_validated_mobile_workspace_root(&workspace.path, roots)?;
        workspace_git_compare(
            root.canonical_path(),
            base_commit.as_deref(),
            &target_commit,
        )
    })
    .await
}

async fn read_mobile_workspace_git_revision_patch(
    runtime_store: &RuntimeStore,
    payload: &Value,
) -> HostResult<Value> {
    let workspace = workspace_for_mobile_file_request(runtime_store, payload).await?;
    let target_commit = require_string_key(payload, "targetCommit")?;
    let base_commit = optional_string_key(payload, "baseCommit");
    let relative_path = require_string_key(payload, "relativePath")?;
    let roots = known_workspace_paths(runtime_store).await?;
    spawn_blocking_workspace("Workspace Git revision patch", move || {
        let root = open_validated_mobile_workspace_root(&workspace.path, roots)?;
        workspace_git_revision_patch(
            root.canonical_path(),
            base_commit.as_deref(),
            &target_commit,
            &relative_path,
        )
    })
    .await
}

fn open_mobile_git_repository(root: &Path) -> HostResult<Repository> {
    let repo = Repository::open(root)
        .map_err(|_| HostError::state("Workspace is not a Git repository."))?;
    let actual_root = repo
        .workdir()
        .and_then(|directory| dunce::canonicalize(directory).ok())
        .ok_or_else(|| HostError::state("Workspace Git worktree is unavailable."))?;
    if actual_root != root {
        return Err(HostError::state(
            "Git root does not match the selected workspace.",
        ));
    }
    Ok(repo)
}

/// Never resolve a mobile-supplied branch name, tag or arbitrary Git object.
/// A valid SHA must identify a commit reachable from this workspace's HEAD.
fn checked_mobile_git_commit<'repo>(
    repo: &'repo Repository,
    raw_sha: &str,
) -> HostResult<git2::Commit<'repo>> {
    if raw_sha.len() != 40 || !raw_sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(HostError::format(
            "A full 40-character Git commit SHA is required.",
        ));
    }
    let oid =
        Oid::from_str(raw_sha).map_err(|_| HostError::format("Git commit SHA is invalid."))?;
    let head = repo
        .head()
        .and_then(|reference| reference.peel_to_commit())
        .map_err(|_| HostError::state("Workspace has no Git HEAD commit."))?;
    if oid != head.id() && !repo.graph_descendant_of(head.id(), oid).unwrap_or(false) {
        return Err(HostError::state(
            "Commit is not reachable from the selected workspace HEAD.",
        ));
    }
    repo.find_commit(oid)
        .map_err(|_| HostError::state("Commit is unavailable in the selected workspace."))
}

fn workspace_git_revision_patch(
    root: &Path,
    base_commit: Option<&str>,
    target_commit: &str,
    relative_path: &str,
) -> HostResult<Value> {
    let path = Path::new(relative_path);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || is_protected_workspace_path(path)
    {
        return Err(HostError::state("Git revision patch path is invalid."));
    }
    let repo = open_mobile_git_repository(root)?;
    let target = checked_mobile_git_commit(&repo, target_commit)?;
    let base = base_commit
        .map(|sha| checked_mobile_git_commit(&repo, sha))
        .transpose()?;
    let base_tree = base
        .as_ref()
        .map(|commit| commit.tree())
        .transpose()
        .map_err(|error| HostError::state(format!("Git revision patch failed: {error}")))?;
    let target_tree = target
        .tree()
        .map_err(|error| HostError::state(format!("Git revision patch failed: {error}")))?;
    let mut options = DiffOptions::new();
    options.pathspec(relative_path).disable_pathspec_match(true);
    let diff = repo
        .diff_tree_to_tree(base_tree.as_ref(), Some(&target_tree), Some(&mut options))
        .map_err(|error| HostError::state(format!("Git revision patch failed: {error}")))?;
    if !diff
        .deltas()
        .any(|delta| delta.old_file().path() == Some(path) || delta.new_file().path() == Some(path))
    {
        return Err(HostError::state(
            "File has no changes between selected commits.",
        ));
    }
    let mut patch = Vec::new();
    let mut truncated = false;
    diff.print(DiffFormat::Patch, |_delta, _hunk, line| {
        if patch.len() >= MAX_MOBILE_GIT_DIFF_BYTES {
            truncated = true;
            return true;
        }
        if matches!(line.origin(), '+' | '-' | ' ') {
            patch.push(line.origin() as u8);
        }
        let available = MAX_MOBILE_GIT_DIFF_BYTES.saturating_sub(patch.len());
        let content = line.content();
        patch.extend_from_slice(&content[..content.len().min(available)]);
        if content.len() > available {
            truncated = true;
            return true;
        }
        true
    })
    .map_err(|error| HostError::state(format!("Git revision patch failed: {error}")))?;
    Ok(json!({
        "baseCommit": base_commit,
        "targetCommit": target.id().to_string(),
        "relativePath": relative_path,
        "patch": String::from_utf8_lossy(&patch),
        "truncated": truncated,
    }))
}

fn workspace_git_compare(
    root: &Path,
    base_commit: Option<&str>,
    target_commit: &str,
) -> HostResult<Value> {
    let repo = open_mobile_git_repository(root)?;
    let target = checked_mobile_git_commit(&repo, target_commit)?;
    let base = base_commit
        .map(|sha| checked_mobile_git_commit(&repo, sha))
        .transpose()?;
    let base_tree = base
        .as_ref()
        .map(|commit| commit.tree())
        .transpose()
        .map_err(|error| HostError::state(format!("Git comparison failed: {error}")))?;
    let target_tree = target
        .tree()
        .map_err(|error| HostError::state(format!("Git comparison failed: {error}")))?;
    let diff = repo
        .diff_tree_to_tree(base_tree.as_ref(), Some(&target_tree), None)
        .map_err(|error| HostError::state(format!("Git comparison failed: {error}")))?;
    let mut items = Vec::new();
    for delta in diff.deltas() {
        let old_path = delta.old_file().path().and_then(Path::to_str);
        let new_path = delta.new_file().path().and_then(Path::to_str);
        let Some(relative_path) = new_path.or(old_path) else {
            continue;
        };
        if is_protected_workspace_path(Path::new(relative_path)) {
            continue;
        }
        let status = match delta.status() {
            Delta::Added => "added",
            Delta::Deleted => "deleted",
            Delta::Renamed => "renamed",
            Delta::Copied => "copied",
            Delta::Typechange => "typechange",
            _ => "modified",
        };
        items.push(json!({
            "relativePath": relative_path,
            "oldPath": old_path,
            "newPath": new_path,
            "status": status,
        }));
        if items.len() > MAX_MOBILE_GIT_COMPARE_ITEMS {
            break;
        }
    }
    let truncated = items.len() > MAX_MOBILE_GIT_COMPARE_ITEMS;
    items.truncate(MAX_MOBILE_GIT_COMPARE_ITEMS);
    Ok(json!({
        "baseCommit": base_commit,
        "targetCommit": target.id().to_string(),
        "items": items,
        "truncated": truncated,
    }))
}

async fn read_mobile_prompt_attachment(runtime_dir: PathBuf, payload: &Value) -> HostResult<Value> {
    let requested_path = require_string_key(payload, "path")?;
    let offset = payload.get("offset").and_then(Value::as_u64).unwrap_or(0);
    let length = payload
        .get("length")
        .and_then(Value::as_u64)
        .unwrap_or(alera_core::workspace_files::MAX_REMOTE_READ_BYTES);
    spawn_blocking_workspace("Prompt attachment read", move || {
        let canonical_path = dunce::canonicalize(&requested_path)
            .map_err(|_| HostError::state("Prompt attachment is unavailable."))?;
        let root = prompt_attachment_root(&runtime_dir, &canonical_path)?;
        let root =
            open_workspace_file_root(&root.to_string_lossy()).map_err(workspace_file_error)?;
        let relative_path = canonical_path
            .strip_prefix(root.canonical_path())
            .ok()
            .and_then(|path| path.to_str())
            .filter(|path| !path.is_empty())
            .ok_or_else(|| HostError::state("Prompt attachment path is invalid."))?
            .to_string();
        let range = read_workspace_file_range_from_root(&root, &relative_path, offset, length)
            .map_err(workspace_file_error)?;
        Ok(workspace_range_response(relative_path, range))
    })
    .await
}

fn workspace_range_response(
    relative_path: String,
    range: alera_core::workspace_files::WorkspaceFileRange,
) -> Value {
    json!({
        "relativePath": relative_path,
        "offset": range.offset,
        "nextOffset": range.next_offset,
        "totalBytes": range.total_bytes,
        "mimeType": range.mime_type,
        "isText": range.is_text,
        "dataBase64": STANDARD.encode(range.bytes),
    })
}

async fn workspace_for_mobile_file_request(
    runtime_store: &RuntimeStore,
    payload: &Value,
) -> HostResult<Workspace> {
    let workspace_id = require_string_key(payload, "workspaceId")?;
    runtime_store
        .find_workspace(&workspace_id)
        .await
        .map_err(|error| HostError::state(error.to_string()))?
        .ok_or_else(|| HostError::state(format!("Workspace not found: {workspace_id}")))
}

async fn mobile_workspace_file_root(
    runtime_store: &RuntimeStore,
    payload: &Value,
    workspace: &Workspace,
) -> HostResult<String> {
    let candidate = optional_string_key(payload, "cwd").unwrap_or_else(|| workspace.path.clone());
    let roots = known_workspace_paths(runtime_store).await?;
    let display_candidate = candidate.clone();
    spawn_blocking_workspace("Workspace root validation", move || {
        let candidate_root = dunce::canonicalize(&candidate).map_err(|error| {
            HostError::state(format!("Codex working directory is unavailable: {error}"))
        })?;
        let roots = roots
            .iter()
            .filter_map(|path| dunce::canonicalize(path).ok())
            .collect();
        if !candidate_root.is_dir() {
            return Err(HostError::state(format!(
                "Codex working directory is not a directory: {}",
                candidate_root.display()
            )));
        }
        validated_mobile_workspace_root(&candidate_root, roots).map_err(|error| {
            HostError::state(format!(
                "Codex working directory is unavailable: {} ({error})",
                Path::new(&display_candidate).display()
            ))
        })
    })
    .await
}

async fn known_workspace_paths(runtime_store: &RuntimeStore) -> HostResult<Vec<String>> {
    Ok(runtime_store
        .list_all_workspaces()
        .await
        .map_err(|error| HostError::state(error.to_string()))?
        .into_iter()
        .map(|workspace| workspace.path)
        .collect())
}

async fn spawn_blocking_workspace<T: Send + 'static>(
    operation: &'static str,
    task: impl FnOnce() -> HostResult<T> + Send + 'static,
) -> HostResult<T> {
    tokio::task::spawn_blocking(task)
        .await
        .map_err(|error| HostError::state(format!("{operation} failed: {error}")))?
}

fn absolute_workspace_file_target(
    requested: &str,
    roots: Vec<String>,
) -> HostResult<(WorkspaceFileRoot, String)> {
    let candidate = dunce::canonicalize(requested)
        .map_err(|_| HostError::state("Workspace file is unavailable."))?;
    let root = roots
        .into_iter()
        .filter_map(|root| open_workspace_file_root(&root).ok())
        .filter(|root| candidate.starts_with(root.canonical_path()))
        .max_by_key(|root| root.canonical_path().components().count())
        .ok_or_else(|| HostError::state("Workspace file is outside known workspaces."))?;
    let relative = candidate
        .strip_prefix(root.canonical_path())
        .ok()
        .and_then(|path| path.to_str())
        .filter(|path| !path.is_empty())
        .ok_or_else(|| HostError::state("Workspace file path is invalid."))?;
    let relative = relative.to_string();
    #[cfg(windows)]
    let relative = relative.replace('\\', "/");
    Ok((root, relative))
}

fn open_validated_mobile_workspace_root(
    candidate: &str,
    roots: Vec<String>,
) -> HostResult<WorkspaceFileRoot> {
    let candidate_root = open_workspace_file_root(candidate).map_err(workspace_file_error)?;
    let known_roots = roots
        .into_iter()
        .filter_map(|root| open_workspace_file_root(&root).ok())
        .map(|root| root.canonical_path().to_path_buf())
        .collect();
    validated_mobile_workspace_root(candidate_root.canonical_path(), known_roots).map_err(
        |error| {
            HostError::state(format!(
                "Codex working directory is unavailable: {} ({error})",
                Path::new(candidate).display()
            ))
        },
    )?;
    Ok(candidate_root)
}

fn validated_mobile_workspace_root(candidate: &Path, roots: Vec<PathBuf>) -> HostResult<String> {
    let root = roots
        .into_iter()
        .filter(|root| candidate.starts_with(root))
        .max_by_key(|root| root.components().count())
        .ok_or_else(|| HostError::state("outside known workspaces"))?;
    let relative = candidate
        .strip_prefix(&root)
        .map_err(|_| HostError::state("outside known workspaces"))?;
    if is_protected_workspace_path(relative) {
        return Err(HostError::state("protected workspace metadata"));
    }
    Ok(candidate.to_string_lossy().into_owned())
}

fn workspace_file_error(error: alera_core::workspace_files::WorkspaceFileError) -> HostError {
    HostError::state(error.to_string())
}

#[cfg(all(test, unix))]
#[path = "mobile_workspace_file_requests_platform_tests.rs"]
mod platform_tests;

#[cfg(test)]
mod tests {
    use super::super::actor_test_harness::test_actor;
    use super::super::request_route_policy::MobileWorkspaceFileOperation;
    use super::{
        absolute_workspace_file_target, mutate_mobile_git_stage, prompt_attachment_root,
        validated_mobile_workspace_root, workspace_git_compare, workspace_git_history,
        workspace_git_patch, workspace_git_revision_patch, workspace_git_status,
    };
    use serde_json::json;
    use std::collections::HashMap;
    use std::path::Path;

    #[test]
    fn mobile_git_compare_returns_changed_files_and_rejects_unreachable_commits() {
        let directory = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(directory.path()).unwrap();
        let signature = git2::Signature::now("Mobile Test", "mobile@test.invalid").unwrap();
        let file = Path::new("notes.txt");
        std::fs::write(directory.path().join(file), "before\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(file).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let first = repo
            .commit(Some("HEAD"), &signature, &signature, "first", &tree, &[])
            .unwrap();
        drop(tree);
        std::fs::write(directory.path().join(file), "after\n").unwrap();
        std::fs::write(directory.path().join("extra.txt"), "new\n").unwrap();
        index.add_path(file).unwrap();
        index.add_path(Path::new("extra.txt")).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let previous = repo.find_commit(first).unwrap();
        let second = repo
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                "second",
                &tree,
                &[&previous],
            )
            .unwrap();
        let root = dunce::canonicalize(directory.path()).unwrap();
        let changed =
            workspace_git_compare(&root, Some(&first.to_string()), &second.to_string()).unwrap();
        let files = changed["items"].as_array().unwrap();
        assert_eq!(files.len(), 2);
        assert!(files
            .iter()
            .any(|file| file["relativePath"] == "notes.txt" && file["status"] == "modified"));
        assert!(files
            .iter()
            .any(|file| file["relativePath"] == "extra.txt" && file["status"] == "added"));
        assert_eq!(changed["truncated"], false);
        let first_changes = workspace_git_compare(&root, None, &first.to_string()).unwrap();
        assert_eq!(first_changes["items"][0]["status"], "added");
        let reversed =
            workspace_git_compare(&root, Some(&second.to_string()), &first.to_string()).unwrap();
        assert!(reversed["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|file| file["status"] == "deleted"));

        let patch = workspace_git_revision_patch(
            &root,
            Some(&first.to_string()),
            &second.to_string(),
            "notes.txt",
        )
        .unwrap();
        assert!(patch["patch"].as_str().unwrap().contains("-before"));
        assert!(patch["patch"].as_str().unwrap().contains("+after"));
        assert_eq!(patch["truncated"], false);
        let root_patch =
            workspace_git_revision_patch(&root, None, &first.to_string(), "notes.txt").unwrap();
        assert!(root_patch["patch"].as_str().unwrap().contains("+before"));
        assert!(workspace_git_revision_patch(
            &root,
            Some(&first.to_string()),
            &second.to_string(),
            "../notes.txt"
        )
        .is_err());
        assert!(workspace_git_revision_patch(
            &root,
            Some(&first.to_string()),
            &second.to_string(),
            ".git/config"
        )
        .is_err());
        assert!(workspace_git_revision_patch(
            &root,
            Some(&first.to_string()),
            &second.to_string(),
            "notes*.txt"
        )
        .is_err());

        let unattached = repo
            .commit(None, &signature, &signature, "unreachable", &tree, &[])
            .unwrap();
        assert!(
            workspace_git_compare(&root, Some(&unattached.to_string()), &second.to_string())
                .is_err()
        );
        assert!(
            workspace_git_compare(&root, Some("refs/heads/main"), &second.to_string()).is_err()
        );
        assert!(workspace_git_compare(&root, Some(&first.to_string()), "deadbeef").is_err());
        assert!(workspace_git_revision_patch(
            &root,
            Some(&unattached.to_string()),
            &second.to_string(),
            "notes.txt"
        )
        .is_err());
    }

    #[test]
    fn mobile_git_history_lists_only_selected_worktree_head_and_parents() {
        let directory = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(directory.path()).unwrap();
        let tracked = Path::new("history.txt");
        let author = git2::Signature::now("Test User", "test@example.com").unwrap();
        std::fs::write(directory.path().join(tracked), "first\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(tracked).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let first = repo
            .commit(Some("HEAD"), &author, &author, "first commit", &tree, &[])
            .unwrap();
        drop(tree);
        std::fs::write(directory.path().join(tracked), "second\n").unwrap();
        index.add_path(tracked).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let parent = repo.find_commit(first).unwrap();
        let second = repo
            .commit(
                Some("HEAD"),
                &author,
                &author,
                "second commit",
                &tree,
                &[&parent],
            )
            .unwrap();
        let root = dunce::canonicalize(directory.path()).unwrap();
        let recent = workspace_git_history(&root, 1).unwrap();
        let list = recent["commits"].as_array().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["id"], second.to_string());
        assert_eq!(list[0]["summary"], "second commit");
        assert_eq!(list[0]["parents"][0], first.to_string());
        let complete = workspace_git_history(&root, 2).unwrap();
        assert_eq!(complete["commits"].as_array().unwrap().len(), 2);
        assert_eq!(complete["commits"][1]["id"], first.to_string());
    }

    #[test]
    fn mobile_git_stage_is_exact_file_and_unstage_preserves_working_copy() {
        let directory = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(directory.path()).unwrap();
        let path = Path::new("tracked.txt");
        std::fs::write(directory.path().join(path), b"old\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(path).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let author = git2::Signature::now("Mobile Test", "mobile@test.invalid").unwrap();
        repo.commit(Some("HEAD"), &author, &author, "init", &tree, &[])
            .unwrap();
        std::fs::write(directory.path().join(path), b"edited\n").unwrap();
        let root = dunce::canonicalize(directory.path()).unwrap();

        mutate_mobile_git_stage(&root, "tracked.txt", true).unwrap();
        let staged = git2::Repository::open(&root).unwrap();
        assert!(staged
            .status_file(path)
            .unwrap()
            .contains(git2::Status::INDEX_MODIFIED));
        mutate_mobile_git_stage(&root, "tracked.txt", false).unwrap();
        let unstaged = git2::Repository::open(&root).unwrap();
        let status = unstaged.status_file(path).unwrap();
        assert!(status.contains(git2::Status::WT_MODIFIED));
        assert!(!status.contains(git2::Status::INDEX_MODIFIED));
        assert_eq!(
            std::fs::read(directory.path().join(path)).unwrap(),
            b"edited\n"
        );

        std::fs::write(directory.path().join("new.txt"), b"new\n").unwrap();
        mutate_mobile_git_stage(&root, "new.txt", true).unwrap();
        mutate_mobile_git_stage(&root, "new.txt", false).unwrap();
        assert!(directory.path().join("new.txt").exists());
        assert!(git2::Repository::open(&root)
            .unwrap()
            .index()
            .unwrap()
            .get_path(Path::new("new.txt"), 0)
            .is_none());

        assert!(mutate_mobile_git_stage(&root, "../outside.txt", true).is_err());
        assert!(mutate_mobile_git_stage(&root, ".git/config", true).is_err());
        assert!(mutate_mobile_git_stage(&root, "untracked-directory", true).is_err());
    }

    #[test]
    fn workspace_git_status_reports_tracked_and_untracked_changes() {
        let directory = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(directory.path()).unwrap();
        let path = Path::new("tracked.txt");
        std::fs::write(directory.path().join(path), "old\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(path).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let signature = git2::Signature::now("Test", "test@example.com").unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, "init", &tree, &[])
            .unwrap();
        std::fs::write(directory.path().join(path), "new\n").unwrap();
        std::fs::write(directory.path().join("added.txt"), "new file\n").unwrap();
        let root = dunce::canonicalize(directory.path()).unwrap();
        let result = workspace_git_status(&root).unwrap();
        let items = result["items"].as_array().unwrap();
        assert!(items
            .iter()
            .any(|item| { item["relativePath"] == "tracked.txt" && item["status"] == "modified" }));
        assert!(items
            .iter()
            .any(|item| { item["relativePath"] == "added.txt" && item["status"] == "added" }));
        assert_eq!(result["truncated"], false);
    }

    #[test]
    fn workspace_git_patch_is_scoped_to_one_tracked_file() {
        let directory = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(directory.path()).unwrap();
        let relative = Path::new("change.txt");
        std::fs::write(directory.path().join(relative), "before\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(relative).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let signature = git2::Signature::now("Test", "test@example.com").unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
            .unwrap();
        std::fs::write(directory.path().join(relative), "after\n").unwrap();
        let root = dunce::canonicalize(directory.path()).unwrap();
        let result = workspace_git_patch(&root, "change.txt").unwrap();
        assert!(result["patch"].as_str().unwrap().contains("+after"));
        assert!(!result["truncated"].as_bool().unwrap());
        assert_eq!(result["relativePath"], "change.txt");
        assert!(workspace_git_patch(&root, "../outside.txt").is_err());
        assert!(workspace_git_patch(&root, ".git/config").is_err());
    }

    #[test]
    fn prompt_attachment_reads_are_limited_to_runtime_upload_stores() {
        let runtime = tempfile::tempdir().unwrap();
        let files = runtime.path().join("prompt-files");
        let outside = runtime.path().join("outside");
        std::fs::create_dir_all(&files).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let attachment = files.join("attachment.txt");
        let unrelated = outside.join("private.txt");
        std::fs::write(&attachment, b"attachment").unwrap();
        std::fs::write(&unrelated, b"private").unwrap();

        let attachment = dunce::canonicalize(attachment).unwrap();
        let unrelated = dunce::canonicalize(unrelated).unwrap();
        assert!(prompt_attachment_root(runtime.path(), &attachment).is_ok());
        assert!(prompt_attachment_root(runtime.path(), &unrelated).is_err());
    }

    #[test]
    fn mobile_roots_reject_protected_workspace_metadata() {
        let workspace = tempfile::tempdir().unwrap();
        let metadata = workspace.path().join(".git");
        std::fs::create_dir(&metadata).unwrap();

        let error =
            validated_mobile_workspace_root(&metadata, vec![workspace.path().to_path_buf()])
                .unwrap_err();

        assert!(error
            .wire_message()
            .contains("protected workspace metadata"));
    }

    #[test]
    fn absolute_files_use_the_most_specific_known_workspace() {
        let directory = tempfile::tempdir().unwrap();
        let parent = directory.path().join("workspace");
        let nested = parent.join("packages/app");
        let file = nested.join("lib/main.dart");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"void main() {}").unwrap();
        let nested_canonical = dunce::canonicalize(&nested).unwrap();

        let (root, relative) = absolute_workspace_file_target(
            &file.to_string_lossy(),
            vec![
                parent.to_string_lossy().into_owned(),
                nested.to_string_lossy().into_owned(),
            ],
        )
        .unwrap();

        assert_eq!(root.canonical_path(), nested_canonical);
        assert_eq!(relative, Path::new("lib/main.dart").to_string_lossy());
    }

    #[test]
    fn absolute_files_outside_known_workspaces_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let workspace = directory.path().join("workspace");
        let outside = directory.path().join("outside.txt");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(&outside, b"outside").unwrap();

        let result = absolute_workspace_file_target(
            &outside.to_string_lossy(),
            vec![workspace.to_string_lossy().into_owned()],
        );

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn filesystem_requests_are_parked_before_runtime_lookup() {
        let directory = tempfile::tempdir().unwrap();
        let actor = test_actor(&directory, HashMap::new(), HashMap::new()).await;

        let started = actor.start_mobile_workspace_file_request(
            1,
            1,
            MobileWorkspaceFileOperation::WorkspaceFileRead,
            "mobile.workspaceFile.read",
            &json!({
                "workspaceId": "missing",
                "relativePath": "README.md",
            }),
        );

        assert!(started.is_ok());
        tokio::task::yield_now().await;
    }
}
