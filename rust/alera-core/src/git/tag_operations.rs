use git2::{ErrorCode, Repository};

use super::commit_operations::{ensure_clean_state, resolve_commit};
use super::{open_repo, GitError, GitErrorKind};

pub fn create_tag(
    path: &str,
    commit_id: &str,
    name: &str,
    message: Option<&str>,
) -> Result<(), GitError> {
    let repo = open_repo(path)?;
    ensure_clean_state(&repo, "creating a tag")?;
    let name = name.trim();
    if name.is_empty() {
        return Err(GitError::new(
            GitErrorKind::InvalidBranchName,
            "tag name cannot be empty",
        ));
    }
    let commit = resolve_commit(&repo, commit_id)?;
    let signature = repo.signature().map_err(|_| {
        GitError::new(
            GitErrorKind::Internal,
            "Configure Git user.name and user.email before creating an annotated tag.",
        )
    })?;
    let result = match message {
        Some(message) => repo.tag(name, commit.as_object(), &signature, message, false),
        None => repo.tag_lightweight(name, commit.as_object(), false),
    };
    result.map(|_| ()).map_err(|error| match error.code() {
        ErrorCode::Exists => GitError::new(
            GitErrorKind::BranchAlreadyExists,
            format!("tag \"{name}\" already exists"),
        ),
        ErrorCode::InvalidSpec => GitError::new(GitErrorKind::InvalidBranchName, name),
        _ => GitError::from_git2(error),
    })
}

pub fn delete_tag(path: &str, name: &str) -> Result<(), GitError> {
    let repo = open_repo(path)?;
    ensure_clean_state(&repo, "deleting a tag")?;
    let name = name.trim();
    if name.is_empty() {
        return Err(GitError::new(
            GitErrorKind::InvalidBranchName,
            "tag name cannot be empty",
        ));
    }
    repo.tag_delete(name).map_err(|error| match error.code() {
        ErrorCode::NotFound => GitError::new(GitErrorKind::BranchNotFound, name),
        ErrorCode::InvalidSpec => GitError::new(GitErrorKind::InvalidBranchName, name),
        _ => GitError::from_git2(error),
    })
}

pub(super) fn tag_reference_exists(repo: &Repository, name: &str) -> Result<(), GitError> {
    let reference_name = format!("refs/tags/{name}");
    repo.find_reference(&reference_name)
        .map(|_| ())
        .map_err(|error| match error.code() {
            ErrorCode::NotFound => GitError::new(
                GitErrorKind::BranchNotFound,
                format!("tag \"{name}\" was not found"),
            ),
            _ => GitError::from_git2(error),
        })
}
