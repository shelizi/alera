use git2::{build::CheckoutBuilder, ErrorCode, MergeOptions, Repository, ResetType};

use super::branch_operations::{checkout_error, ensure_pending_changes_compatible};
use super::commit_operations::{current_head_commit, ensure_clean_state};
use super::{open_repo, GitError, GitErrorKind};

pub fn merge_ref(path: &str, reference: &str) -> Result<Option<String>, GitError> {
    let repo = open_repo(path)?;
    let reference = reference.trim();
    if reference.is_empty() || reference.starts_with('-') {
        return Err(GitError::new(
            GitErrorKind::InvalidBranchName,
            "merge ref cannot be empty or start with '-'",
        ));
    }
    ensure_clean_state(&repo, "merging a ref")?;
    let target = resolve_ref_commit(&repo, reference)?;
    let annotated = repo
        .find_annotated_commit(target.id())
        .map_err(GitError::from_git2)?;
    let (analysis, _) = repo
        .merge_analysis(&[&annotated])
        .map_err(GitError::from_git2)?;

    if analysis.is_up_to_date() {
        return Ok(None);
    }

    let head_reference = repo.find_reference("HEAD").map_err(GitError::from_git2)?;
    let head_symbolic = head_reference
        .symbolic_target()
        .map_err(GitError::from_git2)?
        .map(str::to_string);
    let head = match repo.head() {
        Ok(_) => Some(current_head_commit(&repo)?),
        Err(error) if error.code() == ErrorCode::UnbornBranch => None,
        Err(error) => return Err(GitError::from_git2(error)),
    };

    if analysis.is_fast_forward() || analysis.is_unborn() {
        if head.is_some() {
            ensure_pending_changes_compatible(&repo, &target)?;
        }
        fast_forward(&repo, head.as_ref(), head_symbolic.as_deref(), &target)?;
        return Ok(None);
    }
    if !analysis.is_normal() {
        return Err(GitError::new(
            GitErrorKind::Internal,
            format!("Git could not determine how to merge ref \"{reference}\""),
        ));
    }
    let head = head.ok_or_else(|| {
        GitError::new(
            GitErrorKind::BranchNotFound,
            "cannot merge into an unborn HEAD",
        )
    })?;
    let signature = default_signature(&repo)?;
    let mut merge_options = MergeOptions::new();
    let mut checkout_options = CheckoutBuilder::new();
    checkout_options.safe();
    if let Err(error) = repo.merge(
        &[&annotated],
        Some(&mut merge_options),
        Some(&mut checkout_options),
    ) {
        if error.code() == ErrorCode::Conflict {
            return Err(GitError::new(
                GitErrorKind::Conflict,
                format!(
                    "merging ref \"{reference}\" produced merge conflicts: {}",
                    error.message()
                ),
            ));
        }
        return Err(GitError::from_git2(error));
    }

    let mut index = repo
        .index()
        .map_err(|error| abort_merge(&repo, &head, GitError::from_git2(error)))?;
    if index.has_conflicts() {
        drop(index);
        return Err(GitError::new(
            GitErrorKind::Conflict,
            format!("merging ref \"{reference}\" produced merge conflicts"),
        ));
    }
    let tree_id = match index.write_tree() {
        Ok(tree_id) => tree_id,
        Err(error) => {
            drop(index);
            return Err(abort_merge(&repo, &head, GitError::from_git2(error)));
        }
    };
    drop(index);
    let tree = match repo.find_tree(tree_id) {
        Ok(tree) => tree,
        Err(error) => return Err(abort_merge(&repo, &head, GitError::from_git2(error))),
    };
    let message = repo
        .message()
        .ok()
        .filter(|message| !message.is_empty())
        .unwrap_or_else(|| format!("Merge {reference}"));
    let merge_oid = match repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        &message,
        &tree,
        &[&head, &target],
    ) {
        Ok(oid) => oid,
        Err(error) => {
            drop(tree);
            return Err(abort_merge(&repo, &head, GitError::from_git2(error)));
        }
    };
    drop(tree);
    repo.cleanup_state().map_err(GitError::from_git2)?;
    Ok(Some(merge_oid.to_string()))
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

fn fast_forward(
    repo: &Repository,
    current: Option<&git2::Commit<'_>>,
    symbolic_head: Option<&str>,
    target: &git2::Commit<'_>,
) -> Result<(), GitError> {
    let mut preflight = CheckoutBuilder::new();
    preflight.safe().dry_run();
    repo.checkout_tree(target.as_object(), Some(&mut preflight))
        .map_err(checkout_error)?;
    let mut checkout = CheckoutBuilder::new();
    checkout.safe();
    repo.checkout_tree(target.as_object(), Some(&mut checkout))
        .map_err(checkout_error)?;

    let update_result = if let Some(symbolic_head) = symbolic_head {
        repo.find_reference(symbolic_head)
            .map_err(GitError::from_git2)?
            .set_target(target.id(), "fast-forward merge")
            .map(|_| ())
            .map_err(GitError::from_git2)
    } else {
        repo.set_head_detached(target.id())
            .map_err(GitError::from_git2)
    };
    if let Err(error) = update_result {
        if let Some(current) = current {
            let mut restore = CheckoutBuilder::new();
            restore.force();
            let _ = repo.checkout_tree(current.as_object(), Some(&mut restore));
        }
        return Err(error);
    }
    Ok(())
}

fn default_signature(repo: &Repository) -> Result<git2::Signature<'_>, GitError> {
    repo.signature().map_err(|_| {
        GitError::new(
            GitErrorKind::Internal,
            "Configure Git user.name and user.email before committing.",
        )
    })
}

fn abort_merge(repo: &Repository, head: &git2::Commit<'_>, original: GitError) -> GitError {
    let cleanup_result = repo.cleanup_state().map_err(GitError::from_git2);
    let reset_result = repo
        .reset(head.as_object(), ResetType::Hard, None)
        .map_err(GitError::from_git2);
    let kind = original.kind;
    let mut context = original.context;
    if let Err(error) = cleanup_result {
        context.push_str(&format!("; failed to clean up merge state: {error}"));
    }
    if let Err(error) = reset_result {
        context.push_str(&format!("; failed to restore HEAD after merge: {error}"));
    }
    GitError::new(kind, context)
}
