use std::path::Path;

use git2::{ErrorCode, Repository};

use crate::git_cli::git_in_dir;

use super::{open_repo, GitError, GitErrorKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitArchiveFormat {
    Zip,
    Tar,
}

pub fn create_archive(
    path: &str,
    reference: &str,
    output_path: &str,
    format: GitArchiveFormat,
) -> Result<(), GitError> {
    let repo = open_repo(path)?;
    let reference = reference.trim();
    if reference.is_empty() || reference.starts_with('-') {
        return Err(GitError::new(
            GitErrorKind::InvalidBranchName,
            format!("invalid archive ref '{reference}'"),
        ));
    }
    resolve_archive_ref(&repo, reference)?;
    drop(repo);

    // libgit2 has no archive API, so export through the credential-safe git CLI boundary.
    let format_arg = match format {
        GitArchiveFormat::Zip => "--format=zip",
        GitArchiveFormat::Tar => "--format=tar",
    };
    git_in_dir(
        Path::new(path),
        &["archive", format_arg, "-o", output_path, reference],
    )
    .map(|_| ())
    .map_err(|error| GitError::new(GitErrorKind::GitCli, error.message))
}

fn resolve_archive_ref(repo: &Repository, reference: &str) -> Result<(), GitError> {
    repo.revparse_single(reference)
        .map(|_| ())
        .map_err(|error| match error.code() {
            ErrorCode::NotFound => GitError::new(
                GitErrorKind::BranchNotFound,
                format!("unable to resolve ref \"{reference}\""),
            ),
            ErrorCode::Ambiguous | ErrorCode::InvalidSpec => GitError::new(
                GitErrorKind::InvalidBranchName,
                format!("invalid archive ref \"{reference}\""),
            ),
            _ => GitError::new(
                GitErrorKind::BranchNotFound,
                format!("unable to resolve ref \"{reference}\": {}", error.message()),
            ),
        })
}
