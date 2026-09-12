use super::*;
use std::collections::HashMap;

use git2::{BranchType, ErrorCode};

pub(super) fn resolve_current_ref(
    repo: &Repository,
    branch_name: &str,
    head_oid: Oid,
) -> Result<GitHistoryItemRef, GitError> {
    if branch_name != "HEAD" {
        return Ok(GitHistoryItemRef {
            id: format!("refs/heads/{branch_name}"),
            name: branch_name.to_string(),
            revision: Some(head_oid.to_string()),
            category: Some(GitHistoryRefCategory::Branches),
        });
    }
    let _ = repo;
    Ok(GitHistoryItemRef {
        id: head_oid.to_string(),
        name: short_oid(head_oid),
        revision: Some(head_oid.to_string()),
        category: Some(GitHistoryRefCategory::Commits),
    })
}

pub(super) fn resolve_upstream_ref(
    repo: &Repository,
    branch_name: &str,
) -> Result<Option<GitHistoryItemRef>, GitError> {
    if branch_name == "HEAD" {
        return Ok(None);
    }
    let Ok(local) = repo.find_branch(branch_name, BranchType::Local) else {
        return Ok(None);
    };
    let Ok(upstream) = local.upstream() else {
        return Ok(None);
    };
    let Some(oid) = upstream.get().target() else {
        return Ok(None);
    };
    let full_name = upstream.get().name().unwrap_or_default();
    Ok(Some(history_ref_from_full_name(
        full_name,
        upstream
            .name()
            .map_err(GitError::from_git2)?
            .unwrap_or(full_name),
        oid,
    )))
}

pub(super) fn resolve_named_ref(
    repo: &Repository,
    name: Option<&str>,
) -> Result<Option<GitHistoryItemRef>, GitError> {
    let Some(name) = name.map(str::trim).filter(|name| !name.is_empty()) else {
        return Ok(None);
    };
    if name.starts_with('-') {
        return Ok(None);
    }
    let object = match repo.revparse_single(name) {
        Ok(object) => object,
        Err(error)
            if matches!(
                error.code(),
                ErrorCode::NotFound | ErrorCode::Ambiguous | ErrorCode::InvalidSpec
            ) =>
        {
            return Ok(None);
        }
        Err(error) => return Err(GitError::from_git2(error)),
    };
    let commit = match object.peel_to_commit() {
        Ok(commit) => commit,
        Err(_) => return Ok(None),
    };
    let full_name = repo
        .find_reference(name)
        .ok()
        .and_then(|reference| reference.name().ok().map(ToString::to_string));
    Ok(Some(history_ref_from_full_name(
        full_name.as_deref().unwrap_or(name),
        name,
        commit.id(),
    )))
}

pub(super) fn refs_by_oid(
    repo: &Repository,
) -> Result<HashMap<Oid, Vec<GitHistoryItemRef>>, GitError> {
    let mut output: HashMap<Oid, Vec<GitHistoryItemRef>> = HashMap::new();
    let references = repo.references().map_err(GitError::from_git2)?;
    for reference in references {
        let reference = reference.map_err(GitError::from_git2)?;
        let Ok(full_name) = reference.name() else {
            continue;
        };
        if full_name == "HEAD" || full_name.ends_with("/HEAD") {
            continue;
        }
        let category = category_for_ref(full_name);
        if category.is_none() {
            continue;
        }
        let oid = if category == Some(GitHistoryRefCategory::Tags) {
            reference
                .peel_to_commit()
                .ok()
                .map(|commit| commit.id())
                .or_else(|| reference.target())
        } else {
            reference
                .target()
                .or_else(|| reference.peel_to_commit().ok().map(|commit| commit.id()))
        };
        let Some(oid) = oid else {
            continue;
        };
        output
            .entry(oid)
            .or_default()
            .push(history_ref_from_full_name(
                full_name,
                short_name_for_ref(full_name),
                oid,
            ));
    }
    for refs in output.values_mut() {
        refs.sort_by(compare_refs);
    }
    Ok(output)
}

pub(super) fn history_ref_from_full_name(
    full_name: &str,
    fallback_name: &str,
    oid: Oid,
) -> GitHistoryItemRef {
    let category = category_for_ref(full_name).unwrap_or(GitHistoryRefCategory::Commits);
    GitHistoryItemRef {
        id: full_name.to_string(),
        name: short_name_for_ref(full_name)
            .strip_prefix("tag: ")
            .unwrap_or_else(|| short_name_for_ref(fallback_name))
            .to_string(),
        revision: Some(oid.to_string()),
        category: Some(category),
    }
}

pub(super) fn category_for_ref(full_name: &str) -> Option<GitHistoryRefCategory> {
    if full_name.starts_with("refs/heads/") {
        Some(GitHistoryRefCategory::Branches)
    } else if full_name.starts_with("refs/remotes/") {
        Some(GitHistoryRefCategory::RemoteBranches)
    } else if full_name.starts_with("refs/tags/") {
        Some(GitHistoryRefCategory::Tags)
    } else {
        None
    }
}

pub(super) fn short_name_for_ref(full_name: &str) -> &str {
    full_name
        .strip_prefix("refs/heads/")
        .or_else(|| full_name.strip_prefix("refs/remotes/"))
        .or_else(|| full_name.strip_prefix("refs/tags/"))
        .unwrap_or(full_name)
}

pub(super) fn compare_refs(a: &GitHistoryItemRef, b: &GitHistoryItemRef) -> std::cmp::Ordering {
    ref_order(a)
        .cmp(&ref_order(b))
        .then_with(|| a.name.cmp(&b.name))
}

pub(super) fn ref_order(reference: &GitHistoryItemRef) -> u8 {
    match reference.category {
        Some(GitHistoryRefCategory::Branches) => 1,
        Some(GitHistoryRefCategory::RemoteBranches) => 2,
        Some(GitHistoryRefCategory::Tags) => 3,
        _ => 99,
    }
}
