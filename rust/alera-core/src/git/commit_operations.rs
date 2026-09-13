use std::collections::{HashMap, HashSet};

use git2::{
    build::CheckoutBuilder, ErrorCode, Repository, RepositoryState, ResetType, RevertOptions,
};

use super::branch_operations::{checkout_error, ensure_pending_changes_compatible};
use super::{open_repo, GitError, GitErrorKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitResetMode {
    Soft,
    Mixed,
    Hard,
}

pub fn revert_commit(
    path: &str,
    commit_id: &str,
    mainline_parent: Option<u32>,
) -> Result<String, GitError> {
    let repo = open_repo(path)?;
    ensure_clean_state(&repo, "reverting a commit")?;

    let commit = resolve_commit(&repo, commit_id)?;
    let mainline_parent = validate_mainline_parent(&commit, mainline_parent)?;
    let head = current_head_commit(&repo)?;
    let touched_paths = ensure_revert_changes_compatible(&repo, &commit, mainline_parent)?;
    let original_index = snapshot_index(&repo)?;
    let original_worktree = snapshot_worktree(&repo, &touched_paths)?;
    let signature = repo.signature().map_err(|_| {
        GitError::new(
            GitErrorKind::Internal,
            "Configure Git user.name and user.email before committing.",
        )
    })?;

    let commit_oid = commit.id().to_string();
    let subject = commit
        .summary()
        .map_err(GitError::from_git2)?
        .unwrap_or_default()
        .to_string();
    let mut options = RevertOptions::new();
    if let Some(mainline_parent) = mainline_parent {
        options.mainline(mainline_parent);
    }

    if let Err(error) = repo.revert(&commit, Some(&mut options)) {
        let revert_error = if error.code() == ErrorCode::Conflict {
            GitError::new(GitErrorKind::Conflict, error.message())
        } else {
            GitError::from_git2(error)
        };
        return Err(restore_after_revert_failure(
            &repo,
            &head,
            &touched_paths,
            &original_index,
            &original_worktree,
            revert_error,
        ));
    }

    let mut index = repo.index().map_err(GitError::from_git2)?;
    if index.has_conflicts() {
        drop(index);
        let conflict = GitError::new(
            GitErrorKind::Conflict,
            "reverting the commit produced merge conflicts",
        );
        return Err(restore_after_revert_failure(
            &repo,
            &head,
            &touched_paths,
            &original_index,
            &original_worktree,
            conflict,
        ));
    }
    let tree_id = match index.write_tree() {
        Ok(tree_id) => tree_id,
        Err(error) => {
            drop(index);
            let commit_error = GitError::from_git2(error);
            return Err(restore_after_revert_failure(
                &repo,
                &head,
                &touched_paths,
                &original_index,
                &original_worktree,
                commit_error,
            ));
        }
    };
    drop(index);

    let tree = match repo.find_tree(tree_id) {
        Ok(tree) => tree,
        Err(error) => {
            let commit_error = GitError::from_git2(error);
            return Err(restore_after_revert_failure(
                &repo,
                &head,
                &touched_paths,
                &original_index,
                &original_worktree,
                commit_error,
            ));
        }
    };
    let message = format!("Revert \"{subject}\"\n\nThis reverts commit {commit_oid}.");
    let new_oid = match repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        &message,
        &tree,
        &[&head],
    ) {
        Ok(oid) => oid,
        Err(error) => {
            drop(tree);
            let commit_error = GitError::from_git2(error);
            return Err(restore_after_revert_failure(
                &repo,
                &head,
                &touched_paths,
                &original_index,
                &original_worktree,
                commit_error,
            ));
        }
    };
    drop(tree);

    repo.cleanup_state().map_err(GitError::from_git2)?;
    Ok(new_oid.to_string())
}

/// Detaches HEAD at [commit_id], updating the index and working tree.
/// Pending changes that overlap the target commit abort the checkout, and a
/// checkout libgit2 cannot apply cleanly rolls HEAD back to its previous
/// symbolic or detached target.
pub fn checkout_commit(path: &str, commit_id: &str) -> Result<(), GitError> {
    let repo = open_repo(path)?;
    ensure_clean_state(&repo, "checking out a commit")?;
    let commit = resolve_commit(&repo, commit_id)?;

    let head = repo.head().map_err(GitError::from_git2)?;
    let already_detached_at_target = head
        .symbolic_target()
        .map_err(GitError::from_git2)?
        .is_none()
        && head.target() == Some(commit.id());
    drop(head);
    if already_detached_at_target {
        return Ok(());
    }

    ensure_pending_changes_compatible(&repo, &commit)?;

    // Apply the target tree before moving HEAD: a rejected safe checkout then
    // leaves HEAD untouched instead of sitting detached on an unapplied commit.
    let mut checkout = CheckoutBuilder::new();
    checkout.safe();
    repo.checkout_tree(commit.as_object(), Some(&mut checkout))
        .map_err(checkout_error)?;
    repo.set_head_detached(commit.id())
        .map_err(GitError::from_git2)
}

pub fn reset_to_commit(path: &str, commit_id: &str, mode: GitResetMode) -> Result<(), GitError> {
    let repo = open_repo(path)?;
    ensure_clean_state(&repo, "resetting to a commit")?;
    let commit = resolve_commit(&repo, commit_id)?;
    let reset_type = match mode {
        GitResetMode::Soft => ResetType::Soft,
        GitResetMode::Mixed => ResetType::Mixed,
        GitResetMode::Hard => ResetType::Hard,
    };
    repo.reset(commit.as_object(), reset_type, None)
        .map_err(GitError::from_git2)
}

pub(super) fn ensure_clean_state(repo: &Repository, operation: &str) -> Result<(), GitError> {
    if repo.state() != RepositoryState::Clean {
        return Err(GitError::new(
            GitErrorKind::Conflict,
            format!("finish or abort the in-progress git operation before {operation}"),
        ));
    }
    Ok(())
}

pub(super) fn resolve_commit<'repo>(
    repo: &'repo Repository,
    commit_id: &str,
) -> Result<git2::Commit<'repo>, GitError> {
    let object = repo
        .revparse_single(commit_id)
        .map_err(|_| invalid_commit(commit_id))?;
    object
        .peel_to_commit()
        .map_err(|_| invalid_commit(commit_id))
}

fn invalid_commit(commit_id: &str) -> GitError {
    GitError::new(
        GitErrorKind::Internal,
        format!("unable to resolve commit id \"{commit_id}\""),
    )
}

pub(super) fn current_head_commit(repo: &Repository) -> Result<git2::Commit<'_>, GitError> {
    repo.head()
        .map_err(GitError::from_git2)?
        .peel_to_commit()
        .map_err(GitError::from_git2)
}

pub(super) fn validate_mainline_parent(
    commit: &git2::Commit<'_>,
    mainline_parent: Option<u32>,
) -> Result<Option<u32>, GitError> {
    let parent_count = commit.parent_count() as u32;
    if parent_count > 1 {
        match mainline_parent {
            Some(parent) if (1..=parent_count).contains(&parent) => Ok(Some(parent)),
            Some(parent) => Err(GitError::new(
                GitErrorKind::Internal,
                format!(
                    "mainline parent {parent} is out of range; merge commit requires a mainline parent between 1 and {parent_count}"
                ),
            )),
            None => Err(GitError::new(
                GitErrorKind::Internal,
                format!(
                    "merge commit requires a mainline parent between 1 and {parent_count}"
                ),
            )),
        }
    } else if mainline_parent.is_some() {
        Err(GitError::new(
            GitErrorKind::Internal,
            "mainline parent is only valid for merge commits",
        ))
    } else {
        Ok(None)
    }
}

fn ensure_revert_changes_compatible(
    repo: &Repository,
    commit: &git2::Commit<'_>,
    mainline_parent: Option<u32>,
) -> Result<HashSet<String>, GitError> {
    let commit_tree = commit.tree().map_err(GitError::from_git2)?;
    let parent = if commit.parent_count() == 0 {
        None
    } else {
        Some(
            commit
                .parent(mainline_parent.unwrap_or(1) as usize - 1)
                .map_err(GitError::from_git2)?,
        )
    };
    let parent_tree = parent
        .as_ref()
        .map(|parent| parent.tree())
        .transpose()
        .map_err(GitError::from_git2)?;
    let diff = repo
        .diff_tree_to_tree(parent_tree.as_ref(), Some(&commit_tree), None)
        .map_err(GitError::from_git2)?;
    let touched_paths = revert_touched_paths(&diff);
    if touched_paths.is_empty() {
        return Ok(touched_paths);
    }

    let mut options = git2::StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .update_index(true);
    let statuses = repo
        .statuses(Some(&mut options))
        .map_err(GitError::from_git2)?;
    for entry in statuses.iter() {
        let path = entry.path().map_err(GitError::from_git2)?;
        if touched_paths
            .iter()
            .any(|touched| paths_overlap(path, touched))
        {
            return Err(GitError::new(
                GitErrorKind::Conflict,
                format!("local changes to \"{path}\" overlap the reverted commit"),
            ));
        }
    }
    Ok(touched_paths)
}

fn revert_touched_paths(diff: &git2::Diff<'_>) -> HashSet<String> {
    let mut paths = HashSet::new();
    for delta in diff.deltas() {
        for file in [delta.old_file(), delta.new_file()] {
            if let Some(path) = file.path() {
                paths.insert(normalize_path(path));
            }
        }
    }
    paths
}

fn normalize_path(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn paths_overlap(left: &str, right: &str) -> bool {
    left == right
        || left
            .strip_prefix(right)
            .is_some_and(|remainder| remainder.starts_with('/'))
        || right
            .strip_prefix(left)
            .is_some_and(|remainder| remainder.starts_with('/'))
}

fn snapshot_index(repo: &Repository) -> Result<Vec<git2::IndexEntry>, GitError> {
    Ok(repo.index().map_err(GitError::from_git2)?.iter().collect())
}

fn snapshot_worktree(
    repo: &Repository,
    touched_paths: &HashSet<String>,
) -> Result<HashMap<String, Option<Vec<u8>>>, GitError> {
    let workdir = repo
        .workdir()
        .ok_or_else(|| GitError::new(GitErrorKind::Internal, "repository has no working tree"))?;
    touched_paths
        .iter()
        .map(|path| {
            let absolute_path = workdir.join(path);
            let contents = match std::fs::read(&absolute_path) {
                Ok(contents) => Some(contents),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => {
                    return Err(GitError::new(
                        GitErrorKind::Internal,
                        format!(
                            "failed to snapshot worktree path \"{path}\" before reverting: {error}"
                        ),
                    ));
                }
            };
            Ok((path.clone(), contents))
        })
        .collect()
}

fn restore_after_revert_failure(
    repo: &Repository,
    head: &git2::Commit<'_>,
    touched_paths: &HashSet<String>,
    original_index: &[git2::IndexEntry],
    original_worktree: &HashMap<String, Option<Vec<u8>>>,
    original_error: GitError,
) -> GitError {
    let restore_result = (|| {
        repo.cleanup_state().map_err(GitError::from_git2)?;
        repo.reset(head.as_object(), ResetType::Mixed, None)
            .map_err(GitError::from_git2)?;
        if !touched_paths.is_empty() {
            let mut checkout = CheckoutBuilder::new();
            checkout.force();
            for path in touched_paths {
                checkout.path(path);
            }
            repo.checkout_head(Some(&mut checkout))
                .map_err(GitError::from_git2)?;
            let workdir = repo.workdir().ok_or_else(|| {
                GitError::new(GitErrorKind::Internal, "repository has no working tree")
            })?;
            for (path, contents) in original_worktree {
                let absolute_path = workdir.join(path);
                match contents {
                    Some(contents) => std::fs::write(&absolute_path, contents).map_err(|error| {
                        GitError::new(
                            GitErrorKind::Internal,
                            format!(
                                "failed to restore worktree path \"{path}\" after reverting: {error}"
                            ),
                        )
                    })?,
                    None => match std::fs::symlink_metadata(&absolute_path) {
                        Ok(_) => std::fs::remove_file(&absolute_path).map_err(|error| {
                            GitError::new(
                                GitErrorKind::Internal,
                                format!(
                                    "failed to remove worktree path \"{path}\" after reverting: {error}"
                                ),
                            )
                        })?,
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(error) => {
                            return Err(GitError::new(
                                GitErrorKind::Internal,
                                format!(
                                    "failed to inspect worktree path \"{path}\" after reverting: {error}"
                                ),
                            ));
                        }
                    },
                }
            }
        }
        let mut index = repo.index().map_err(GitError::from_git2)?;
        index.clear().map_err(GitError::from_git2)?;
        for entry in original_index {
            index.add(entry).map_err(GitError::from_git2)?;
        }
        index.write().map_err(GitError::from_git2)?;
        Ok::<(), GitError>(())
    })();
    if let Err(restore_error) = restore_result {
        GitError::new(
            original_error.kind,
            format!("{original_error}; failed to restore the pre-revert state: {restore_error}"),
        )
    } else {
        original_error
    }
}
