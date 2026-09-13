use git2::{
    CherrypickOptions, ErrorCode, Rebase, RebaseOperationType, RebaseOptions, Repository,
    ResetType, Signature,
};

use super::commit_operations::{
    current_head_commit, ensure_clean_state, resolve_commit, validate_mainline_parent,
};
use super::{open_repo, GitError, GitErrorKind};

pub fn cherry_pick_commit(
    path: &str,
    commit_id: &str,
    mainline_parent: Option<u32>,
) -> Result<String, GitError> {
    let repo = open_repo(path)?;
    ensure_clean_state(&repo, "cherry-picking a commit")?;
    let commit = resolve_commit(&repo, commit_id)?;
    let mainline_parent = validate_mainline_parent(&commit, mainline_parent)?;
    let head = current_head_commit(&repo)?;
    let signature = default_signature(&repo)?;
    let mut options = CherrypickOptions::new();
    if let Some(mainline_parent) = mainline_parent {
        options.mainline(mainline_parent);
    }

    if let Err(error) = repo.cherrypick(&commit, Some(&mut options)) {
        let pick_error = replay_error(
            error,
            GitErrorKind::Conflict,
            format!("cherry-picking commit {commit_id} produced conflicts"),
        );
        return Err(abort_cherry_pick(&repo, &head, pick_error));
    }

    let mut index = repo
        .index()
        .map_err(|error| abort_cherry_pick(&repo, &head, GitError::from_git2(error)))?;
    if index.has_conflicts() {
        drop(index);
        return Err(abort_cherry_pick(
            &repo,
            &head,
            GitError::new(
                GitErrorKind::Conflict,
                format!("cherry-picking commit {commit_id} produced merge conflicts"),
            ),
        ));
    }
    let tree_id = match index.write_tree() {
        Ok(tree_id) => tree_id,
        Err(error) => {
            drop(index);
            return Err(abort_cherry_pick(&repo, &head, GitError::from_git2(error)));
        }
    };
    drop(index);
    let tree = match repo.find_tree(tree_id) {
        Ok(tree) => tree,
        Err(error) => return Err(abort_cherry_pick(&repo, &head, GitError::from_git2(error))),
    };
    let message = match commit.message() {
        Ok(message) => message,
        Err(error) => {
            drop(tree);
            return Err(abort_cherry_pick(&repo, &head, GitError::from_git2(error)));
        }
    };
    let new_oid = match repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &[&head],
    ) {
        Ok(oid) => oid,
        Err(error) => {
            drop(tree);
            return Err(abort_cherry_pick(&repo, &head, GitError::from_git2(error)));
        }
    };
    drop(tree);
    repo.cleanup_state().map_err(GitError::from_git2)?;
    Ok(new_oid.to_string())
}

pub fn drop_commit(path: &str, commit_id: &str) -> Result<(), GitError> {
    let repo = open_repo(path)?;
    ensure_clean_state(&repo, "dropping a commit")?;
    let target = resolve_commit(&repo, commit_id)?;
    if target.parent_count() == 0 {
        return Err(GitError::new(
            GitErrorKind::Internal,
            format!("cannot drop root commit {commit_id}"),
        ));
    }

    let head_reference = repo.find_reference("HEAD").map_err(GitError::from_git2)?;
    let branch_reference_name = head_reference
        .symbolic_target()
        .map_err(GitError::from_git2)?
        .map(str::to_string);
    let Some(branch_reference_name) = branch_reference_name else {
        return Err(GitError::new(
            GitErrorKind::DetachedHead,
            "cannot drop a commit from detached HEAD",
        ));
    };
    let resolved_head = repo.head().map_err(|error| match error.code() {
        ErrorCode::UnbornBranch => GitError::new(
            GitErrorKind::BranchNotFound,
            "cannot drop a commit from an unborn branch",
        ),
        _ => GitError::from_git2(error),
    })?;
    let head = resolved_head
        .peel_to_commit()
        .map_err(GitError::from_git2)?;
    let target_is_ancestor = target.id() == head.id()
        || repo
            .graph_descendant_of(head.id(), target.id())
            .map_err(GitError::from_git2)?;
    if !target_is_ancestor {
        return Err(GitError::new(
            GitErrorKind::Conflict,
            format!("commit {commit_id} is not an ancestor of the current HEAD"),
        ));
    }

    let parent = target.parent(0).map_err(GitError::from_git2)?;
    let branch_reference = repo
        .find_reference(&branch_reference_name)
        .map_err(GitError::from_git2)?;
    let branch = repo
        .reference_to_annotated_commit(&branch_reference)
        .map_err(GitError::from_git2)?;
    let upstream = repo
        .find_annotated_commit(target.id())
        .map_err(GitError::from_git2)?;
    let onto = repo
        .find_annotated_commit(parent.id())
        .map_err(GitError::from_git2)?;
    let signature = default_signature(&repo)?;
    let mut options = RebaseOptions::new();
    let rebase = repo
        .rebase(
            Some(&branch),
            Some(&upstream),
            Some(&onto),
            Some(&mut options),
        )
        .map_err(|error| replay_error(error, GitErrorKind::Conflict, "dropping the commit"))?;
    finish_rebase(&repo, rebase, &signature, "dropping the commit")
}

pub fn rebase_onto(path: &str, onto_ref: &str) -> Result<(), GitError> {
    let repo = open_repo(path)?;
    let onto_ref = onto_ref.trim();
    if onto_ref.is_empty() || onto_ref.starts_with('-') {
        return Err(GitError::new(
            GitErrorKind::InvalidBranchName,
            "rebase target ref cannot be empty or start with '-'",
        ));
    }
    ensure_clean_state(&repo, "rebasing the current branch")?;
    let head_reference = repo.find_reference("HEAD").map_err(GitError::from_git2)?;
    let branch_reference_name = head_reference
        .symbolic_target()
        .map_err(GitError::from_git2)?
        .map(str::to_string);
    let resolved_head = repo.head().map_err(|error| match error.code() {
        ErrorCode::UnbornBranch => {
            GitError::new(GitErrorKind::BranchNotFound, "cannot rebase an unborn HEAD")
        }
        _ => GitError::from_git2(error),
    })?;
    let onto_commit = resolve_ref_commit(&repo, onto_ref)?;
    let onto = repo
        .find_annotated_commit(onto_commit.id())
        .map_err(GitError::from_git2)?;
    let branch = if let Some(branch_reference_name) = branch_reference_name {
        let branch_reference = repo
            .find_reference(&branch_reference_name)
            .map_err(GitError::from_git2)?;
        Some(
            repo.reference_to_annotated_commit(&branch_reference)
                .map_err(GitError::from_git2)?,
        )
    } else {
        None
    };
    drop(resolved_head);
    let signature = default_signature(&repo)?;
    let mut options = RebaseOptions::new();
    let rebase = repo
        .rebase(branch.as_ref(), Some(&onto), None, Some(&mut options))
        .map_err(|error| {
            replay_error(error, GitErrorKind::Conflict, "rebasing the current branch")
        })?;
    finish_rebase(&repo, rebase, &signature, "rebasing the current branch")
}

fn resolve_ref_commit<'repo>(
    repo: &'repo Repository,
    reference: &str,
) -> Result<git2::Commit<'repo>, GitError> {
    if reference.trim().is_empty() || reference.trim().starts_with('-') {
        return Err(GitError::new(
            GitErrorKind::InvalidBranchName,
            format!("invalid ref \"{reference}\""),
        ));
    }
    let object = repo.revparse_single(reference).map_err(|_| {
        GitError::new(
            GitErrorKind::BranchNotFound,
            format!("unable to resolve ref \"{reference}\""),
        )
    })?;
    object.peel_to_commit().map_err(|_| {
        GitError::new(
            GitErrorKind::BranchNotFound,
            format!("ref \"{reference}\" does not name a commit"),
        )
    })
}

fn default_signature(repo: &Repository) -> Result<Signature<'_>, GitError> {
    repo.signature().map_err(|_| {
        GitError::new(
            GitErrorKind::Internal,
            "Configure Git user.name and user.email before committing.",
        )
    })
}

fn abort_cherry_pick(repo: &Repository, head: &git2::Commit<'_>, original: GitError) -> GitError {
    let cleanup_result = repo.cleanup_state().map_err(GitError::from_git2);
    let reset_result = repo
        .reset(head.as_object(), ResetType::Hard, None)
        .map_err(GitError::from_git2);
    append_cleanup_errors(original, cleanup_result, reset_result, "cherry-pick")
}

fn finish_rebase<'repo>(
    repo: &'repo Repository,
    mut rebase: Rebase<'repo>,
    signature: &Signature<'_>,
    operation: &str,
) -> Result<(), GitError> {
    loop {
        let Some(next) = rebase.next() else {
            break;
        };
        let should_commit = match next {
            Ok(operation_entry) => {
                let has_conflicts = match repo.index() {
                    Ok(index) => index.has_conflicts(),
                    Err(error) => {
                        return Err(abort_rebase(&mut rebase, GitError::from_git2(error)));
                    }
                };
                if has_conflicts {
                    return Err(abort_rebase(
                        &mut rebase,
                        GitError::new(
                            GitErrorKind::Conflict,
                            format!("{operation} produced merge conflicts"),
                        ),
                    ));
                }
                !matches!(operation_entry.kind(), Some(RebaseOperationType::Exec))
            }
            Err(error) => {
                return Err(abort_rebase(
                    &mut rebase,
                    replay_error(error, GitErrorKind::Conflict, operation),
                ));
            }
        };
        if should_commit {
            if let Err(error) = rebase.commit(None, signature, None) {
                return Err(abort_rebase(
                    &mut rebase,
                    replay_error(error, GitErrorKind::Conflict, operation),
                ));
            }
        }
    }
    if let Err(error) = rebase.finish(Some(signature)) {
        return Err(abort_rebase(
            &mut rebase,
            replay_error(error, GitErrorKind::Conflict, operation),
        ));
    }
    Ok(())
}

fn abort_rebase(rebase: &mut Rebase<'_>, original: GitError) -> GitError {
    match rebase.abort() {
        Ok(()) => original,
        Err(error) => GitError::new(
            original.kind,
            format!(
                "{original}; failed to abort the rebase: {}",
                error.message()
            ),
        ),
    }
}

fn replay_error(
    error: git2::Error,
    conflict_kind: GitErrorKind,
    operation: impl Into<String>,
) -> GitError {
    let operation = operation.into();
    if error.code() == ErrorCode::Conflict {
        GitError::new(conflict_kind, format!("{operation}: {}", error.message()))
    } else {
        GitError::from_git2(error)
    }
}

fn append_cleanup_errors(
    original: GitError,
    cleanup_result: Result<(), GitError>,
    reset_result: Result<(), GitError>,
    operation: &str,
) -> GitError {
    let kind = original.kind;
    let mut context = original.context;
    if let Err(error) = cleanup_result {
        context.push_str(&format!("; failed to clean up {operation} state: {error}"));
    }
    if let Err(error) = reset_result {
        context.push_str(&format!(
            "; failed to restore HEAD after {operation}: {error}"
        ));
    }
    GitError::new(kind, context)
}
