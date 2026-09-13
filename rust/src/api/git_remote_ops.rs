use alera_core::git as core_git;

use super::GitError;

pub fn push_tag(path: String, name: String, remote: Option<String>) -> Result<(), GitError> {
    core_git::push_tag(&path, &name, remote.as_deref()).map_err(Into::into)
}

pub fn delete_remote_branch(path: String, remote: String, branch: String) -> Result<(), GitError> {
    core_git::delete_remote_branch(&path, &remote, &branch).map_err(Into::into)
}
