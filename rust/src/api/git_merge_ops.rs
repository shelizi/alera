use alera_core::git as core_git;

use super::GitError;

pub fn merge_ref(path: String, reference: String) -> Result<Option<String>, GitError> {
    core_git::merge_ref(&path, &reference).map_err(Into::into)
}

pub fn merge_conflicted_paths(path: String) -> Result<Vec<String>, GitError> {
    core_git::merge_conflicted_paths(&path).map_err(Into::into)
}

pub struct GitMergeConflictPreview {
    pub base: Option<String>,
    pub ours: Option<String>,
    pub theirs: Option<String>,
    pub is_binary: bool,
}

pub fn merge_in_progress(path: String) -> Result<bool, GitError> {
    core_git::merge_in_progress(&path).map_err(Into::into)
}

pub fn merge_conflict_preview(
    path: String,
    file_path: String,
) -> Result<GitMergeConflictPreview, GitError> {
    let preview = core_git::merge_conflict_preview(&path, &file_path)?;
    Ok(GitMergeConflictPreview {
        base: preview.base,
        ours: preview.ours,
        theirs: preview.theirs,
        is_binary: preview.is_binary,
    })
}

pub fn resolve_merge_conflict(path: String, file_path: String, ours: bool) -> Result<(), GitError> {
    core_git::resolve_merge_conflict(&path, &file_path, ours).map_err(Into::into)
}

pub fn abort_merge_in_progress(path: String) -> Result<(), GitError> {
    core_git::abort_merge_in_progress(&path).map_err(Into::into)
}
