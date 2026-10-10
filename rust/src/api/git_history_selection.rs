use std::collections::HashSet;

use git2::{Repository, Revwalk};

use super::{GitError, GitHistoryItemRef, GitHistoryRefCategory};

pub(super) fn push_selected_refs(
    repo: &Repository,
    revwalk: &mut Revwalk<'_>,
    refs: &[String],
) -> Result<(), GitError> {
    let mut pushed = HashSet::new();
    for name in refs {
        let oid = repo
            .revparse_single(name)
            .and_then(|object| object.peel_to_commit())
            .map_err(GitError::from_git2)?
            .id();
        if pushed.insert(oid) {
            revwalk.push(oid).map_err(GitError::from_git2)?;
        }
    }
    Ok(())
}

pub(super) fn includes_ref(refs: &[String], item_ref: &GitHistoryItemRef) -> bool {
    refs.iter()
        .any(|name| name == &item_ref.id || name == &item_ref.name)
}

pub(super) fn keep_selected_branch_badges(
    refs: &[String],
    references: &mut Vec<GitHistoryItemRef>,
) {
    references.retain(|item_ref| {
        !matches!(
            item_ref.category,
            Some(GitHistoryRefCategory::Branches | GitHistoryRefCategory::RemoteBranches)
        ) || includes_ref(refs, item_ref)
    });
}
