use std::path::Path;

use git2::Branch;

use crate::git_cli::git_in_dir;

use super::tag_operations::tag_reference_exists;
use super::{open_repo, GitError, GitErrorKind};

pub fn push_tag(path: &str, name: &str, remote: Option<&str>) -> Result<(), GitError> {
    let repo = open_repo(path)?;
    let name = name.trim();
    if name.is_empty() {
        return Err(GitError::new(
            GitErrorKind::InvalidBranchName,
            "tag name cannot be empty",
        ));
    }
    tag_reference_exists(&repo, name)?;
    let remote = remote.map(str::trim).unwrap_or("origin");
    ensure_remote_exists(&repo, remote)?;
    drop(repo);

    let tag_reference = format!("refs/tags/{name}");
    git_in_dir(Path::new(path), &["push", remote, tag_reference.as_str()])
        .map(|_| ())
        .map_err(|error| GitError::new(GitErrorKind::GitCli, error.message))
}

pub fn delete_remote_branch(path: &str, remote: &str, branch: &str) -> Result<(), GitError> {
    let repo = open_repo(path)?;
    let remote = remote.trim();
    let branch = branch.trim();
    if remote.is_empty() {
        return Err(GitError::new(
            GitErrorKind::RemoteNotFound,
            "remote name cannot be empty",
        ));
    }
    if branch.is_empty() {
        return Err(GitError::new(
            GitErrorKind::InvalidBranchName,
            "remote branch name cannot be empty",
        ));
    }
    if !Branch::name_is_valid(branch).map_err(GitError::from_git2)? {
        return Err(GitError::new(GitErrorKind::InvalidBranchName, branch));
    }
    ensure_remote_exists(&repo, remote)?;
    drop(repo);

    git_in_dir(Path::new(path), &["push", remote, "--delete", branch])
        .map(|_| ())
        .map_err(|error| GitError::new(GitErrorKind::GitCli, error.message))
}

fn ensure_remote_exists(repo: &git2::Repository, remote: &str) -> Result<(), GitError> {
    repo.find_remote(remote)
        .map(|_| ())
        .map_err(|error| match error.code() {
            git2::ErrorCode::NotFound => GitError::new(
                GitErrorKind::RemoteNotFound,
                format!("remote \"{remote}\" was not found"),
            ),
            _ => GitError::from_git2(error),
        })
}
