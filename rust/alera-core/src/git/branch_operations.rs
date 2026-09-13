use std::collections::HashSet;

use git2::{build::CheckoutBuilder, Branch, BranchType, ErrorCode, RepositoryState, StatusOptions};

use super::{canonical, checkout_path_for_branch, open_repo, GitError, GitErrorKind};

pub fn list_branches(path: &str) -> Result<Vec<String>, GitError> {
    let repo = open_repo(path)?;
    let mut names = Vec::new();
    let branches = repo.branches(None).map_err(GitError::from_git2)?;
    for entry in branches {
        let (branch, _) = entry.map_err(GitError::from_git2)?;
        if let Some(name) = branch.name().map_err(GitError::from_git2)? {
            if name.ends_with("/HEAD") {
                continue;
            }
            names.push(name.to_string());
        }
    }
    names.sort();
    names.dedup();
    Ok(names)
}

pub fn branch_exists(repo_path: &str, branch: &str) -> Result<bool, GitError> {
    let repo = open_repo(repo_path)?;
    let result = match repo.find_branch(branch, BranchType::Local) {
        Ok(_) => Ok(true),
        Err(error) if error.code() == ErrorCode::NotFound => Ok(false),
        Err(error) => Err(GitError::from_git2(error)),
    };
    result
}

/// Creates a local branch at the current HEAD and makes it active without
/// modifying the index or working tree.
pub fn create_and_checkout_branch(path: &str, branch: &str) -> Result<(), GitError> {
    let repo = open_repo(path)?;
    let branch = branch.trim();
    if branch.is_empty() || !is_valid_branch_name(branch)? {
        return Err(GitError::new(GitErrorKind::InvalidBranchName, branch));
    }
    if repo.state() != RepositoryState::Clean {
        return Err(GitError::new(
            GitErrorKind::Conflict,
            "finish or abort the in-progress git operation before creating a branch",
        ));
    }
    if repo.head_detached().map_err(GitError::from_git2)? {
        return Err(GitError::new(
            GitErrorKind::DetachedHead,
            "cannot create a branch from detached HEAD",
        ));
    }
    let head = repo.head().map_err(|error| match error.code() {
        ErrorCode::UnbornBranch => GitError::new(
            GitErrorKind::BranchNotFound,
            "create the first commit before creating another branch",
        ),
        _ => GitError::from_git2(error),
    })?;
    let commit = head.peel_to_commit().map_err(GitError::from_git2)?;
    let mut created = repo
        .branch(branch, &commit, false)
        .map_err(|error| match error.code() {
            ErrorCode::Exists => GitError::new(GitErrorKind::BranchAlreadyExists, branch),
            ErrorCode::InvalidSpec => GitError::new(GitErrorKind::InvalidBranchName, branch),
            _ => GitError::from_git2(error),
        })?;
    let reference_name = format!("refs/heads/{branch}");
    if let Err(error) = repo.set_head(&reference_name) {
        let _ = created.delete();
        return Err(GitError::from_git2(error));
    }
    // Both branches point at the same commit, so changing symbolic HEAD is
    // sufficient and intentionally preserves pending staged/unstaged changes.
    Ok(())
}

/// Makes an existing local branch active in this checkout while preserving
/// pending changes whenever libgit2 can do so safely. A branch already checked
/// out by another worktree is rejected instead of stealing it from that
/// workspace.
pub fn checkout_branch(path: &str, branch: &str) -> Result<(), GitError> {
    let repo = open_repo(path)?;
    let branch = branch.trim();
    if branch.is_empty() || !is_valid_branch_name(branch)? {
        return Err(GitError::new(GitErrorKind::InvalidBranchName, branch));
    }
    if repo.state() != RepositoryState::Clean {
        return Err(GitError::new(
            GitErrorKind::Conflict,
            "finish or abort the in-progress git operation before switching branches",
        ));
    }

    let target = repo
        .find_branch(branch, BranchType::Local)
        .map_err(|error| match error.code() {
            ErrorCode::NotFound => GitError::new(GitErrorKind::BranchNotFound, branch),
            _ => GitError::from_git2(error),
        })?;
    let current_workdir = repo
        .workdir()
        .ok_or_else(|| GitError::new(GitErrorKind::Internal, "repository has no working tree"))?;
    if let Some(checked_out_path) = checkout_path_for_branch(&repo, branch)? {
        if canonical(&checked_out_path) == canonical(&current_workdir.to_string_lossy()) {
            return Ok(());
        }
        return Err(GitError::new(
            GitErrorKind::WorktreeAlreadyExists,
            format!("branch \"{branch}\" is already checked out at \"{checked_out_path}\""),
        ));
    }

    let target_reference = target
        .get()
        .name()
        .map_err(GitError::from_git2)?
        .to_string();
    let target_commit = target.get().peel_to_commit().map_err(GitError::from_git2)?;
    ensure_pending_changes_compatible(&repo, &target_commit)?;

    // Preflight the checkout before moving HEAD. This prevents a rejected safe
    // checkout from leaving symbolic HEAD on a branch whose files were never
    // applied.
    let mut preflight = CheckoutBuilder::new();
    preflight.safe().dry_run();
    repo.checkout_tree(target_commit.as_object(), Some(&mut preflight))
        .map_err(checkout_error)?;

    let head = repo.find_reference("HEAD").map_err(GitError::from_git2)?;
    let previous_symbolic = head
        .symbolic_target()
        .map_err(GitError::from_git2)?
        .map(str::to_string);
    let previous_detached = if previous_symbolic.is_none() {
        head.target()
    } else {
        None
    };
    drop(head);

    repo.set_head(&target_reference)
        .map_err(GitError::from_git2)?;
    let mut checkout = CheckoutBuilder::new();
    checkout.safe();
    if let Err(error) = repo.checkout_head(Some(&mut checkout)) {
        if let Some(reference) = previous_symbolic.as_deref() {
            let _ = repo.set_head(reference);
        } else if let Some(oid) = previous_detached {
            let _ = repo.set_head_detached(oid);
        }
        return Err(checkout_error(error));
    }
    Ok(())
}

pub(super) fn ensure_pending_changes_compatible(
    repo: &git2::Repository,
    target_commit: &git2::Commit<'_>,
) -> Result<(), GitError> {
    let head_commit = repo
        .head()
        .map_err(GitError::from_git2)?
        .peel_to_commit()
        .map_err(GitError::from_git2)?;
    if head_commit.id() == target_commit.id() {
        return Ok(());
    }
    let head_tree = head_commit.tree().map_err(GitError::from_git2)?;
    let target_tree = target_commit.tree().map_err(GitError::from_git2)?;
    let diff = repo
        .diff_tree_to_tree(Some(&head_tree), Some(&target_tree), None)
        .map_err(GitError::from_git2)?;
    let mut target_paths = HashSet::<String>::new();
    for delta in diff.deltas() {
        for file in [delta.old_file(), delta.new_file()] {
            if let Some(path) = file.path() {
                target_paths.insert(path.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    if target_paths.is_empty() {
        return Ok(());
    }

    let mut options = StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .update_index(true);
    let statuses = repo
        .statuses(Some(&mut options))
        .map_err(GitError::from_git2)?;
    for entry in statuses.iter() {
        let path = entry.path().map_err(GitError::from_git2)?;
        if target_paths.contains(&path.replace('\\', "/")) {
            return Err(GitError::new(
                GitErrorKind::Conflict,
                format!("local changes to \"{path}\" would overlap changes on the target branch"),
            ));
        }
    }
    Ok(())
}

pub(super) fn checkout_error(error: git2::Error) -> GitError {
    match error.code() {
        ErrorCode::Conflict => GitError::new(GitErrorKind::Conflict, error.message()),
        _ => GitError::from_git2(error),
    }
}

pub fn is_valid_branch_name(name: &str) -> Result<bool, GitError> {
    Branch::name_is_valid(name).map_err(GitError::from_git2)
}

pub fn delete_branch(repo_path: &str, branch: &str, _force: bool) -> Result<(), GitError> {
    let repo = open_repo(repo_path)?;
    let mut target = repo
        .find_branch(branch, BranchType::Local)
        .map_err(|error| match error.code() {
            ErrorCode::NotFound => GitError::new(GitErrorKind::BranchNotFound, branch),
            _ => GitError::from_git2(error),
        })?;
    target.delete().map_err(GitError::from_git2)?;
    Ok(())
}
