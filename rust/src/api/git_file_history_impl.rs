use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use git2::{Oid, Repository};

use super::GitError;

pub(super) fn nearest_file_ancestors(
    repo: &Repository,
    path: &str,
    start: Oid,
    cache: &mut HashMap<Oid, bool>,
) -> Result<Vec<Oid>, GitError> {
    let mut queue = VecDeque::from([start]);
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    while let Some(oid) = queue.pop_front() {
        if !seen.insert(oid) {
            continue;
        }
        let commit = repo.find_commit(oid).map_err(GitError::from_git2)?;
        let touches = match cache.get(&oid) {
            Some(value) => *value,
            None => {
                let value = commit_touches_file(repo, &commit, path)?;
                cache.insert(oid, value);
                value
            }
        };
        if touches {
            result.push(oid);
        } else {
            queue.extend(commit.parent_ids());
        }
    }
    Ok(result)
}

pub(super) fn commit_touches_file(
    repo: &Repository,
    commit: &git2::Commit<'_>,
    path: &str,
) -> Result<bool, GitError> {
    let tree = commit.tree().map_err(GitError::from_git2)?;
    // Compare against every parent so merge resolutions and branch contributions remain visible.
    if commit.parent_count() == 0 {
        return Ok(tree.get_path(Path::new(path)).is_ok());
    }
    for parent in commit.parents() {
        let parent_tree = parent.tree().map_err(GitError::from_git2)?;
        let mut options = git2::DiffOptions::new();
        options.pathspec(path).disable_pathspec_match(true);
        let diff = repo
            .diff_tree_to_tree(Some(&parent_tree), Some(&tree), Some(&mut options))
            .map_err(GitError::from_git2)?;
        if diff.deltas().len() > 0 {
            return Ok(true);
        }
    }
    Ok(false)
}
