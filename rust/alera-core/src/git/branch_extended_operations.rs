use git2::{build::CheckoutBuilder, Branch, BranchType, ErrorCode, Repository};

use super::branch_operations::{checkout_error, ensure_pending_changes_compatible};
use super::commit_operations::{ensure_clean_state, resolve_commit};
use super::{head_branch_name, open_repo, GitError, GitErrorKind};

pub fn create_branch_at_commit(
    path: &str,
    commit_id: &str,
    branch_name: &str,
    checkout: bool,
) -> Result<(), GitError> {
    let repo = open_repo(path)?;
    let branch_name = branch_name.trim();
    if branch_name.is_empty() || !Branch::name_is_valid(branch_name).map_err(GitError::from_git2)? {
        return Err(GitError::new(GitErrorKind::InvalidBranchName, branch_name));
    }
    ensure_clean_state(&repo, "creating a branch")?;
    let commit = resolve_commit(&repo, commit_id)?;
    let mut created =
        repo.branch(branch_name, &commit, false)
            .map_err(|error| match error.code() {
                ErrorCode::Exists => GitError::new(GitErrorKind::BranchAlreadyExists, branch_name),
                ErrorCode::InvalidSpec => {
                    GitError::new(GitErrorKind::InvalidBranchName, branch_name)
                }
                _ => GitError::from_git2(error),
            })?;

    if !checkout {
        return Ok(());
    }

    match repo.head() {
        Ok(_) => ensure_pending_changes_compatible(&repo, &commit)?,
        Err(error) if error.code() == ErrorCode::UnbornBranch => {}
        Err(error) => {
            let _ = created.delete();
            return Err(GitError::from_git2(error));
        }
    }

    let mut preflight = CheckoutBuilder::new();
    preflight.safe().dry_run();
    if let Err(error) = repo.checkout_tree(commit.as_object(), Some(&mut preflight)) {
        let _ = created.delete();
        return Err(checkout_error(error));
    }

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

    let reference_name = format!("refs/heads/{branch_name}");
    if let Err(error) = repo.set_head(&reference_name) {
        let _ = created.delete();
        return Err(GitError::from_git2(error));
    }
    let mut checkout_options = CheckoutBuilder::new();
    checkout_options.safe();
    if let Err(error) = repo.checkout_head(Some(&mut checkout_options)) {
        if let Some(reference) = previous_symbolic.as_deref() {
            let _ = repo.set_head(reference);
        } else if let Some(oid) = previous_detached {
            let _ = repo.set_head_detached(oid);
        }
        let _ = created.delete();
        return Err(checkout_error(error));
    }
    Ok(())
}

pub fn checkout_remote_branch(path: &str, remote_branch: &str) -> Result<String, GitError> {
    let repo = open_repo(path)?;
    ensure_clean_state(&repo, "checking out a remote branch")?;
    let remote_branch = remote_branch.trim();
    let Some((remote_name, local_name)) = remote_branch.split_once('/') else {
        return Err(GitError::new(
            GitErrorKind::InvalidBranchName,
            format!("remote branch must use the form remote/branch: {remote_branch}"),
        ));
    };
    if remote_name.is_empty()
        || local_name.is_empty()
        || !Branch::name_is_valid(local_name).map_err(GitError::from_git2)?
    {
        return Err(GitError::new(
            GitErrorKind::InvalidBranchName,
            format!("invalid local branch name derived from {remote_branch}"),
        ));
    }

    let remote = repo
        .find_branch(remote_branch, BranchType::Remote)
        .map_err(|error| match error.code() {
            ErrorCode::NotFound => GitError::new(GitErrorKind::BranchNotFound, remote_branch),
            _ => GitError::from_git2(error),
        })?;
    let tracking_name = remote
        .name()
        .map_err(GitError::from_git2)?
        .map(ToString::to_string)
        .unwrap_or_else(|| remote_branch.to_string());
    let target_commit = remote.get().peel_to_commit().map_err(GitError::from_git2)?;

    match repo.find_branch(local_name, BranchType::Local) {
        Ok(_) => {
            return Err(GitError::new(GitErrorKind::BranchAlreadyExists, local_name));
        }
        Err(error) if error.code() == ErrorCode::NotFound => {}
        Err(error) => return Err(GitError::from_git2(error)),
    }

    let mut local = repo
        .branch(local_name, &target_commit, false)
        .map_err(|error| match error.code() {
            ErrorCode::Exists => GitError::new(GitErrorKind::BranchAlreadyExists, local_name),
            ErrorCode::InvalidSpec => GitError::new(GitErrorKind::InvalidBranchName, local_name),
            _ => GitError::from_git2(error),
        })?;
    if let Err(error) = local.set_upstream(Some(&tracking_name)) {
        let _ = local.delete();
        return Err(GitError::from_git2(error));
    }
    drop(local);
    drop(target_commit);
    drop(remote);
    drop(repo);

    match super::branch_operations::checkout_branch(path, local_name) {
        Ok(()) => Ok(local_name.to_string()),
        Err(error) => {
            delete_local_branch(path, local_name);
            Err(error)
        }
    }
}

pub fn rename_branch(path: &str, old_name: &str, new_name: &str) -> Result<(), GitError> {
    let repo = open_repo(path)?;
    ensure_clean_state(&repo, "renaming a branch")?;
    let old_name = old_name.trim();
    let new_name = new_name.trim();
    if old_name.is_empty() || !Branch::name_is_valid(old_name).map_err(GitError::from_git2)? {
        return Err(GitError::new(GitErrorKind::InvalidBranchName, old_name));
    }
    if new_name.is_empty() || !Branch::name_is_valid(new_name).map_err(GitError::from_git2)? {
        return Err(GitError::new(GitErrorKind::InvalidBranchName, new_name));
    }

    let mut branch = repo
        .find_branch(old_name, BranchType::Local)
        .map_err(|error| match error.code() {
            ErrorCode::NotFound => GitError::new(GitErrorKind::BranchNotFound, old_name),
            _ => GitError::from_git2(error),
        })?;
    if old_name == new_name {
        return Ok(());
    }

    if let Some(worktree_path) = linked_worktree_for_branch(&repo, old_name)? {
        return Err(GitError::new(
            GitErrorKind::WorktreeAlreadyExists,
            format!("branch \"{old_name}\" is checked out in linked worktree \"{worktree_path}\""),
        ));
    }

    branch
        .rename(new_name, false)
        .map(|_| ())
        .map_err(|error| match error.code() {
            ErrorCode::Exists => GitError::new(GitErrorKind::BranchAlreadyExists, new_name),
            ErrorCode::InvalidSpec => GitError::new(GitErrorKind::InvalidBranchName, new_name),
            ErrorCode::NotFound => GitError::new(GitErrorKind::BranchNotFound, old_name),
            _ => GitError::from_git2(error),
        })
}

fn linked_worktree_for_branch(
    repo: &Repository,
    branch_name: &str,
) -> Result<Option<String>, GitError> {
    let names = repo.worktrees().map_err(GitError::from_git2)?;
    for entry in names.iter() {
        let Ok(Some(name)) = entry else {
            continue;
        };
        let worktree = repo.find_worktree(name).map_err(GitError::from_git2)?;
        let Ok(worktree_repo) = Repository::open(worktree.path()) else {
            continue;
        };
        if head_branch_name(&worktree_repo) == branch_name {
            return Ok(Some(worktree.path().to_string_lossy().to_string()));
        }
    }
    Ok(None)
}

fn delete_local_branch(path: &str, branch_name: &str) {
    let Ok(repo) = open_repo(path) else {
        return;
    };
    if let Ok(mut branch) = repo.find_branch(branch_name, BranchType::Local) {
        let _ = branch.delete();
    };
}
