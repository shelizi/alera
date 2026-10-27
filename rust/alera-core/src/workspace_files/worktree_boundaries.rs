use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Return only linked worktrees registered in this repository. An unrelated
/// nested Git repository or submodule must remain within the search scope.
/// Callers pass a canonical workspace root; returned paths are canonical.
pub fn nested_git_worktree_paths(root: &Path) -> HashSet<PathBuf> {
    let Ok(repo) = git2::Repository::discover(root) else {
        return HashSet::new();
    };
    let Ok(names) = repo.worktrees() else {
        return HashSet::new();
    };
    names
        .iter()
        .filter_map(|entry| entry.ok().flatten())
        .filter_map(|name| repo.find_worktree(name).ok())
        .filter_map(|worktree| dunce::canonicalize(worktree.path()).ok())
        .filter(|path| path != root && path.starts_with(root))
        .collect()
}
