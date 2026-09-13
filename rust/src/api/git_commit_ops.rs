use alera_core::git as core_git;

use super::GitError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitResetMode {
    Soft,
    Mixed,
    Hard,
}

pub fn git_checkout_commit(path: String, commit_id: String) -> Result<(), GitError> {
    core_git::checkout_commit(&path, &commit_id).map_err(Into::into)
}

pub fn git_revert_commit(
    path: String,
    commit_id: String,
    mainline_parent: Option<u32>,
) -> Result<String, GitError> {
    core_git::revert_commit(&path, &commit_id, mainline_parent).map_err(Into::into)
}

pub fn git_reset_to_commit(
    path: String,
    commit_id: String,
    mode: GitResetMode,
) -> Result<(), GitError> {
    let mode = match mode {
        GitResetMode::Soft => core_git::GitResetMode::Soft,
        GitResetMode::Mixed => core_git::GitResetMode::Mixed,
        GitResetMode::Hard => core_git::GitResetMode::Hard,
    };
    core_git::reset_to_commit(&path, &commit_id, mode).map_err(Into::into)
}

pub fn git_cherry_pick_commit(
    path: String,
    commit_id: String,
    mainline_parent: Option<u32>,
) -> Result<String, GitError> {
    core_git::cherry_pick_commit(&path, &commit_id, mainline_parent).map_err(Into::into)
}

pub fn git_drop_commit(path: String, commit_id: String) -> Result<(), GitError> {
    core_git::drop_commit(&path, &commit_id).map_err(Into::into)
}

pub fn git_rebase_onto(path: String, onto_ref: String) -> Result<(), GitError> {
    core_git::rebase_onto(&path, &onto_ref).map_err(Into::into)
}
