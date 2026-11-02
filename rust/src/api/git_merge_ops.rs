use alera_core::git as core_git;

use super::GitError;

pub fn merge_ref(path: String, reference: String) -> Result<Option<String>, GitError> {
    core_git::merge_ref(&path, &reference).map_err(Into::into)
}

pub fn merge_conflicted_paths(path: String) -> Result<Vec<String>, GitError> {
    core_git::merge_conflicted_paths(&path).map_err(Into::into)
}
