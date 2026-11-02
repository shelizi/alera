use git2::{build::CheckoutBuilder, ErrorCode, MergeOptions, Repository, ResetType};

use super::branch_operations::{checkout_error, ensure_pending_changes_compatible};
use super::commit_operations::{current_head_commit, ensure_clean_state};
use super::{open_repo, GitError, GitErrorKind};

pub fn merge_ref(path: &str, reference: &str) -> Result<Option<String>, GitError> {
    let repo = open_repo(path)?;
    let reference = reference.trim();
    if reference.is_empty() || reference.starts_with('-') {
        return Err(GitError::new(
            GitErrorKind::InvalidBranchName,
            "merge ref cannot be empty or start with '-'",
        ));
    }
    ensure_clean_state(&repo, "merging a ref")?;
    let target = resolve_ref_commit(&repo, reference)?;
    let annotated = repo
        .find_annotated_commit(target.id())
        .map_err(GitError::from_git2)?;
    let (analysis, _) = repo
        .merge_analysis(&[&annotated])
        .map_err(GitError::from_git2)?;

    if analysis.is_up_to_date() {
        return Ok(None);
    }

    let head_reference = repo.find_reference("HEAD").map_err(GitError::from_git2)?;
    let head_symbolic = head_reference
        .symbolic_target()
        .map_err(GitError::from_git2)?
        .map(str::to_string);
    let head = match repo.head() {
        Ok(_) => Some(current_head_commit(&repo)?),
        Err(error) if error.code() == ErrorCode::UnbornBranch => None,
        Err(error) => return Err(GitError::from_git2(error)),
    };

    if analysis.is_fast_forward() || analysis.is_unborn() {
        if head.is_some() {
            ensure_pending_changes_compatible(&repo, &target)?;
        }
        fast_forward(&repo, head.as_ref(), head_symbolic.as_deref(), &target)?;
        return Ok(None);
    }
    if !analysis.is_normal() {
        return Err(GitError::new(
            GitErrorKind::Internal,
            format!("Git could not determine how to merge ref \"{reference}\""),
        ));
    }
    let head = head.ok_or_else(|| {
        GitError::new(
            GitErrorKind::BranchNotFound,
            "cannot merge into an unborn HEAD",
        )
    })?;
    let signature = default_signature(&repo)?;
    let mut merge_options = MergeOptions::new();
    let mut checkout_options = CheckoutBuilder::new();
    checkout_options.safe();
    if let Err(error) = repo.merge(
        &[&annotated],
        Some(&mut merge_options),
        Some(&mut checkout_options),
    ) {
        if error.code() == ErrorCode::Conflict {
            return Err(GitError::new(
                GitErrorKind::Conflict,
                format!(
                    "merging ref \"{reference}\" produced merge conflicts: {}",
                    error.message()
                ),
            ));
        }
        return Err(GitError::from_git2(error));
    }

    let mut index = repo
        .index()
        .map_err(|error| abort_merge(&repo, &head, GitError::from_git2(error)))?;
    if index.has_conflicts() {
        drop(index);
        return Err(GitError::new(
            GitErrorKind::Conflict,
            format!("merging ref \"{reference}\" produced merge conflicts"),
        ));
    }
    let tree_id = match index.write_tree() {
        Ok(tree_id) => tree_id,
        Err(error) => {
            drop(index);
            return Err(abort_merge(&repo, &head, GitError::from_git2(error)));
        }
    };
    drop(index);
    let tree = match repo.find_tree(tree_id) {
        Ok(tree) => tree,
        Err(error) => return Err(abort_merge(&repo, &head, GitError::from_git2(error))),
    };
    let message = repo
        .message()
        .ok()
        .filter(|message| !message.is_empty())
        .unwrap_or_else(|| format!("Merge {reference}"));
    let merge_oid = match repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        &message,
        &tree,
        &[&head, &target],
    ) {
        Ok(oid) => oid,
        Err(error) => {
            drop(tree);
            return Err(abort_merge(&repo, &head, GitError::from_git2(error)));
        }
    };
    drop(tree);
    repo.cleanup_state().map_err(GitError::from_git2)?;
    Ok(Some(merge_oid.to_string()))
}

fn resolve_ref_commit<'repo>(
    repo: &'repo Repository,
    reference: &str,
) -> Result<git2::Commit<'repo>, GitError> {
    if reference.trim().is_empty() || reference.trim().starts_with('-') {
        return Err(GitError::new(
            GitErrorKind::InvalidBranchName,
            format!("invalid ref \"{reference}\""),
        ));
    }
    let object = repo.revparse_single(reference).map_err(|_| {
        GitError::new(
            GitErrorKind::BranchNotFound,
            format!("unable to resolve ref \"{reference}\""),
        )
    })?;
    object.peel_to_commit().map_err(|_| {
        GitError::new(
            GitErrorKind::BranchNotFound,
            format!("ref \"{reference}\" does not name a commit"),
        )
    })
}

fn fast_forward(
    repo: &Repository,
    current: Option<&git2::Commit<'_>>,
    symbolic_head: Option<&str>,
    target: &git2::Commit<'_>,
) -> Result<(), GitError> {
    let mut preflight = CheckoutBuilder::new();
    preflight.safe().dry_run();
    repo.checkout_tree(target.as_object(), Some(&mut preflight))
        .map_err(checkout_error)?;
    let mut checkout = CheckoutBuilder::new();
    checkout.safe();
    repo.checkout_tree(target.as_object(), Some(&mut checkout))
        .map_err(checkout_error)?;

    let update_result = if let Some(symbolic_head) = symbolic_head {
        repo.find_reference(symbolic_head)
            .map_err(GitError::from_git2)?
            .set_target(target.id(), "fast-forward merge")
            .map(|_| ())
            .map_err(GitError::from_git2)
    } else {
        repo.set_head_detached(target.id())
            .map_err(GitError::from_git2)
    };
    if let Err(error) = update_result {
        if let Some(current) = current {
            let mut restore = CheckoutBuilder::new();
            restore.force();
            let _ = repo.checkout_tree(current.as_object(), Some(&mut restore));
        }
        return Err(error);
    }
    Ok(())
}

fn default_signature(repo: &Repository) -> Result<git2::Signature<'_>, GitError> {
    repo.signature().map_err(|_| {
        GitError::new(
            GitErrorKind::Internal,
            "Configure Git user.name and user.email before committing.",
        )
    })
}

fn abort_merge(repo: &Repository, head: &git2::Commit<'_>, original: GitError) -> GitError {
    let cleanup_result = repo.cleanup_state().map_err(GitError::from_git2);
    let reset_result = repo
        .reset(head.as_object(), ResetType::Hard, None)
        .map_err(GitError::from_git2);
    let kind = original.kind;
    let mut context = original.context;
    if let Err(error) = cleanup_result {
        context.push_str(&format!("; failed to clean up merge state: {error}"));
    }
    if let Err(error) = reset_result {
        context.push_str(&format!("; failed to restore HEAD after merge: {error}"));
    }
    GitError::new(kind, context)
}

/// Unresolved merge paths, returned from index stages rather than the status
/// snapshot so both add/add and delete/modify conflicts are visible.
pub fn merge_conflicted_paths(path: &str) -> Result<Vec<String>, GitError> {
    let repo = open_repo(path)?;
    if repo.state() != git2::RepositoryState::Merge {
        return Ok(Vec::new());
    }
    let index = repo.index().map_err(GitError::from_git2)?;
    let mut paths = Vec::new();
    for conflict in index.conflicts().map_err(GitError::from_git2)? {
        let conflict = conflict.map_err(GitError::from_git2)?;
        let entry = conflict
            .our
            .or(conflict.their)
            .or(conflict.ancestor)
            .ok_or_else(|| GitError::new(GitErrorKind::Internal, "empty conflict"))?;
        paths.push(String::from_utf8_lossy(&entry.path).to_string());
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

/// A text preview of the three index stages. Missing stages are represented
/// by None (for example when a side deleted the file). Binary/large files
/// require the normal file editor or an external merge tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitMergeConflictPreview {
    pub base: Option<String>,
    pub ours: Option<String>,
    pub theirs: Option<String>,
    pub is_binary: bool,
}

pub fn merge_in_progress(path: &str) -> Result<bool, GitError> {
    Ok(open_repo(path)?.state() == git2::RepositoryState::Merge)
}

fn checked_conflict_path(relative: &str) -> Result<&std::path::Path, GitError> {
    let relative = std::path::Path::new(relative);
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err(GitError::new(
            GitErrorKind::Conflict,
            "invalid merge conflict path",
        ));
    }
    Ok(relative)
}

fn conflict_stage_text(
    repo: &Repository,
    stage: Option<git2::IndexEntry>,
) -> Result<(Option<String>, bool), GitError> {
    let Some(stage) = stage else {
        return Ok((None, false));
    };
    // A preview is text-only. Symlinks, gitlinks, binary data and oversized
    // files must not become editable text in the app.
    if stage.mode != 0o100644 && stage.mode != 0o100755 {
        return Ok((None, true));
    }
    let blob = repo.find_blob(stage.id).map_err(GitError::from_git2)?;
    let bytes = blob.content();
    if bytes.len() > 128 * 1024 || bytes.contains(&0) {
        return Ok((None, true));
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => Ok((Some(text.to_string()), false)),
        Err(_) => Ok((None, true)),
    }
}

pub fn merge_conflict_preview(
    path: &str,
    file_path: &str,
) -> Result<GitMergeConflictPreview, GitError> {
    let repo = open_repo(path)?;
    if repo.state() != git2::RepositoryState::Merge {
        return Err(GitError::new(
            GitErrorKind::Conflict,
            "no merge is in progress",
        ));
    }
    let relative = checked_conflict_path(file_path)?;
    let index = repo.index().map_err(GitError::from_git2)?;
    let conflict = index
        .conflicts()
        .map_err(GitError::from_git2)?
        .find_map(|item| match item {
            Ok(conflict)
                if conflict
                    .ancestor
                    .as_ref()
                    .or(conflict.our.as_ref())
                    .or(conflict.their.as_ref())
                    .is_some_and(|entry| entry.path == relative.to_string_lossy().as_bytes()) =>
            {
                Some(Ok(conflict))
            }
            Err(error) => Some(Err(GitError::from_git2(error))),
            _ => None,
        })
        .ok_or_else(|| {
            GitError::new(GitErrorKind::Conflict, "file has no unresolved conflict")
        })??;
    let (base, binary_base) = conflict_stage_text(&repo, conflict.ancestor)?;
    let (ours, binary_ours) = conflict_stage_text(&repo, conflict.our)?;
    let (theirs, binary_theirs) = conflict_stage_text(&repo, conflict.their)?;
    Ok(GitMergeConflictPreview {
        base,
        ours,
        theirs,
        is_binary: binary_base || binary_ours || binary_theirs,
    })
}

/// Choose the *index* version for a single unresolved file, materialize it to
/// the isolated worktree and mark that file resolved. Nothing is committed.
pub fn resolve_merge_conflict(path: &str, file_path: &str, ours: bool) -> Result<(), GitError> {
    let repo = open_repo(path)?;
    if repo.state() != git2::RepositoryState::Merge {
        return Err(GitError::new(
            GitErrorKind::Conflict,
            "no merge is in progress",
        ));
    }
    let relative = checked_conflict_path(file_path)?;
    let mut index = repo.index().map_err(GitError::from_git2)?;
    let conflict = index
        .conflicts()
        .map_err(GitError::from_git2)?
        .find_map(|item| match item {
            Ok(conflict)
                if conflict
                    .ancestor
                    .as_ref()
                    .or(conflict.our.as_ref())
                    .or(conflict.their.as_ref())
                    .is_some_and(|entry| entry.path == relative.to_string_lossy().as_bytes()) =>
            {
                Some(Ok(conflict))
            }
            Err(error) => Some(Err(GitError::from_git2(error))),
            _ => None,
        })
        .ok_or_else(|| {
            GitError::new(GitErrorKind::Conflict, "file has no unresolved conflict")
        })??;
    let entry = if ours { conflict.our } else { conflict.their };
    let root = repo
        .workdir()
        .ok_or_else(|| GitError::new(GitErrorKind::Conflict, "bare repository"))?;
    // Do not traverse a symlink in either the file or any ancestor. A merge
    // index path must never be able to write outside the checkout.
    let mut current = root.to_path_buf();
    for part in relative.components() {
        current.push(part.as_os_str());
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(GitError::new(
                    GitErrorKind::Conflict,
                    "symlink conflict must be resolved manually",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(GitError::new(GitErrorKind::AccessDenied, error.to_string())),
        }
    }
    if let Some(entry) = entry {
        if entry.mode != 0o100644 && entry.mode != 0o100755 {
            return Err(GitError::new(
                GitErrorKind::Conflict,
                "symlink/submodule conflict requires manual resolution",
            ));
        }
        let blob = repo.find_blob(entry.id).map_err(GitError::from_git2)?;
        if let Some(parent) = current.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| GitError::new(GitErrorKind::AccessDenied, error.to_string()))?;
        }
        std::fs::write(&current, blob.content())
            .map_err(|error| GitError::new(GitErrorKind::AccessDenied, error.to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = if entry.mode == 0o100755 { 0o755 } else { 0o644 };
            std::fs::set_permissions(&current, std::fs::Permissions::from_mode(mode))
                .map_err(|error| GitError::new(GitErrorKind::AccessDenied, error.to_string()))?;
        }
        index
            .conflict_remove(relative)
            .map_err(GitError::from_git2)?;
        index.add_path(relative).map_err(GitError::from_git2)?;
    } else {
        match std::fs::remove_file(&current) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(GitError::new(GitErrorKind::AccessDenied, error.to_string())),
        }
        index
            .conflict_remove(relative)
            .map_err(GitError::from_git2)?;
        // A deleted file is resolved by having no stage-zero index entry.
        match index.remove_path(relative) {
            Ok(()) => {}
            Err(error) if error.code() == ErrorCode::NotFound => {}
            Err(error) => return Err(GitError::from_git2(error)),
        }
    }
    index.write().map_err(GitError::from_git2)
}

/// Delegates to Git's --abort (reset --merge semantics), instead of a hard
/// reset that could silently discard unrelated pre-merge local changes.
pub fn abort_merge_in_progress(path: &str) -> Result<(), GitError> {
    let repo = open_repo(path)?;
    if repo.state() != git2::RepositoryState::Merge {
        return Err(GitError::new(
            GitErrorKind::Conflict,
            "no merge is in progress",
        ));
    }
    let root = repo
        .workdir()
        .ok_or_else(|| GitError::new(GitErrorKind::Conflict, "bare repository"))?;
    crate::git_cli::git_in_dir(root, &["merge", "--abort"])
        .map(|_| ())
        .map_err(|error| GitError::new(GitErrorKind::GitCli, error.to_string()))
}
