use alera_core::git as core_git;

use super::GitError;

pub fn create_tag(
    path: String,
    commit_id: String,
    name: String,
    message: Option<String>,
) -> Result<(), GitError> {
    core_git::create_tag(&path, &commit_id, &name, message.as_deref()).map_err(Into::into)
}

pub fn delete_tag(path: String, name: String) -> Result<(), GitError> {
    core_git::delete_tag(&path, &name).map_err(Into::into)
}
