use alera_core::git as core_git;

use super::GitError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitArchiveFormat {
    Zip,
    Tar,
}

pub fn create_archive(
    path: String,
    reference: String,
    output_path: String,
    format: GitArchiveFormat,
) -> Result<(), GitError> {
    let format = match format {
        GitArchiveFormat::Zip => core_git::GitArchiveFormat::Zip,
        GitArchiveFormat::Tar => core_git::GitArchiveFormat::Tar,
    };
    core_git::create_archive(&path, &reference, &output_path, format).map_err(Into::into)
}
