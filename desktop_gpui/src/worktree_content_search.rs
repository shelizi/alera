//! Read-only content search over the selected Git repository's real worktrees.
//! Each result carries its owning checkout. Never reinterpret a relative hit
//! against the currently active workspace (two worktrees may have same paths).
use std::collections::HashSet;
use std::io::{self, ErrorKind};
use std::path::{Component, Path, PathBuf};

use crate::project_search::{ContentMatch, ProjectSearch, SearchLimits};
use crate::worktree_manager::WorktreeManager;

const MAX_SEARCH_WORKTREES: usize = 24;
const MAX_COMBINED_MATCHES: usize = 1_000;
const MAX_MATCHES_PER_WORKTREE: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorktreeContentScope {
    Current,
    AllWorktrees,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeContentMatch {
    pub root: PathBuf,
    pub branch: Option<String>,
    pub hit: ContentMatch,
}

/// A linked checkout may physically live inside its primary checkout's
/// .worktrees/ directory. Containment is NOT ownership: route by the exact
/// registered Worktree root from the search result.
pub fn worktree_hit_requires_switch(active_root: &Path, hit: &WorktreeContentMatch) -> bool {
    active_root != hit.root
}

impl WorktreeContentMatch {
    /// The file must still resolve to this exact checkout at click time.
    /// Never join the result to the *currently active* workspace root.
    pub fn checked_path(&self) -> io::Result<PathBuf> {
        if self.hit.path.as_os_str().is_empty()
            || !self
                .hit
                .path
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
        {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "Search hit has an invalid workspace-relative path",
            ));
        }
        let root = dunce::canonicalize(&self.root)?;
        let candidate = dunce::canonicalize(root.join(&self.hit.path))?;
        if !candidate.starts_with(&root) || !candidate.is_file() {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "Search hit no longer belongs to its original worktree",
            ));
        }
        Ok(candidate)
    }

    pub fn label(&self) -> String {
        let display = self.branch.clone().unwrap_or_else(|| {
            self.root
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "detached worktree".to_owned())
        });
        format!("{display} · {}", self.root.display())
    }
}

#[derive(Debug, Default)]
pub struct WorktreeContentResults {
    pub matches: Vec<WorktreeContentMatch>,
    pub truncated: bool,
    pub searched_worktrees: usize,
    pub skipped_worktrees: usize,
}

/// Called only from the GPUI background worker. All-worktree mode discovers
/// Git's actual registered checkouts rather than crawling arbitrary folders.
/// A single unreadable linked checkout causes a partial result, not a silent
/// result from the wrong workspace.
pub fn search_worktree_contents(
    active_root: &Path,
    query: &str,
    scope: WorktreeContentScope,
) -> io::Result<WorktreeContentResults> {
    let active_root = dunce::canonicalize(active_root)?;
    let mut roots = vec![(active_root.clone(), None)];
    let mut out = WorktreeContentResults::default();
    if scope == WorktreeContentScope::AllWorktrees {
        let entries = WorktreeManager::discover(&active_root)?.list()?;
        let mut seen = HashSet::from([active_root.clone()]);
        for entry in entries {
            if entry.prunable {
                out.skipped_worktrees += 1;
                continue;
            }
            let Ok(root) = dunce::canonicalize(&entry.path) else {
                out.skipped_worktrees += 1;
                continue;
            };
            if root == active_root {
                roots[0].1 = entry.branch;
            } else if seen.insert(root.clone()) {
                roots.push((root, entry.branch));
            }
        }
        if roots.len() > MAX_SEARCH_WORKTREES {
            roots.truncate(MAX_SEARCH_WORKTREES);
            out.truncated = true;
        }
    }

    for (root, branch) in roots {
        let scan = ProjectSearch::new(&root).and_then(|search| {
            if scope == WorktreeContentScope::Current {
                search.search(query)
            } else {
                search.search_with_limits(
                    query,
                    SearchLimits {
                        max_results: MAX_MATCHES_PER_WORKTREE,
                        ..SearchLimits::default()
                    },
                )
            }
        });
        let result = match scan {
            Ok(result) => result,
            Err(error) if scope == WorktreeContentScope::AllWorktrees => {
                let _ = error;
                out.skipped_worktrees += 1;
                continue;
            }
            Err(error) => return Err(error),
        };
        out.searched_worktrees += 1;
        out.truncated |= result.truncated;
        for hit in result.matches {
            if out.matches.len() == MAX_COMBINED_MATCHES {
                out.truncated = true;
                break;
            }
            out.matches.push(WorktreeContentMatch {
                root: root.clone(),
                branch: branch.clone(),
                hit,
            });
        }
    }
    if out.skipped_worktrees != 0 {
        out.truncated = true;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::{Repository, Signature};
    use std::fs;

    fn two_worktrees_with_same_relative_file() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("primary");
        fs::create_dir_all(&root).unwrap();
        let repo = Repository::init(&root).unwrap();
        fs::write(root.join("README"), "initial").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("README")).unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let identity = Signature::now("E05 Test", "test@example.invalid").unwrap();
        repo.commit(Some("HEAD"), &identity, &identity, "first", &tree, &[])
            .unwrap();
        drop(tree);
        drop(repo);
        let linked = WorktreeManager::discover(&root)
            .unwrap()
            .create("feature/e05-second")
            .unwrap();
        for tree in [&root, &linked] {
            fs::create_dir_all(tree.join("src")).unwrap();
        }
        fs::write(root.join("src/same.txt"), "needle primary text").unwrap();
        fs::write(linked.join("src/same.txt"), "needle linked text").unwrap();
        (
            tmp,
            dunce::canonicalize(root).unwrap(),
            dunce::canonicalize(linked).unwrap(),
        )
    }

    #[test]
    fn e05_scopes_results_to_exact_worktree_with_duplicate_relative_paths() {
        let (_fixture, main, linked) = two_worktrees_with_same_relative_file();
        let current =
            search_worktree_contents(&main, "needle", WorktreeContentScope::Current).unwrap();
        assert_eq!(current.searched_worktrees, 1);
        assert_eq!(current.matches.len(), 1);
        assert_eq!(
            current.matches[0].checked_path().unwrap(),
            main.join("src/same.txt")
        );

        let all =
            search_worktree_contents(&main, "needle", WorktreeContentScope::AllWorktrees).unwrap();
        assert_eq!(all.searched_worktrees, 2);
        assert_eq!(all.matches.len(), 2);
        assert!(!all.truncated);
        let first = all.matches.iter().find(|hit| hit.root == main).unwrap();
        let second = all.matches.iter().find(|hit| hit.root == linked).unwrap();
        assert_eq!(
            first.hit.path, second.hit.path,
            "deliberately same relative file"
        );
        assert_eq!(first.checked_path().unwrap(), main.join("src/same.txt"));
        assert_eq!(second.checked_path().unwrap(), linked.join("src/same.txt"));
        assert_eq!(
            fs::read_to_string(second.checked_path().unwrap()).unwrap(),
            "needle linked text"
        );
        assert!(second.label().contains("feature/e05-second"));

        let from_linked =
            search_worktree_contents(&linked, "needle", WorktreeContentScope::AllWorktrees)
                .unwrap();
        assert_eq!(from_linked.matches.len(), 2, "search from either checkout");
    }

    #[test]
    fn e05_same_relative_file_opens_owner_after_explicit_worktree_selection() {
        let (_fixture, main, linked) = two_worktrees_with_same_relative_file();
        let found =
            search_worktree_contents(&main, "needle", WorktreeContentScope::AllWorktrees).unwrap();
        let linked_hit = found.matches.iter().find(|hit| hit.root == linked).unwrap();
        let destination = linked_hit.checked_path().unwrap();
        assert!(worktree_hit_requires_switch(&main, linked_hit));
        assert!(!worktree_hit_requires_switch(&linked, linked_hit));

        // The primary and linked checkout deliberately share one *relative*
        // file name. Never reconstruct a linked hit under the old root.
        let wrong = main.join(&linked_hit.hit.path);
        let mut original = crate::model::WorkbenchModel::new(&main).unwrap();
        assert!(original.open_path(&wrong));
        assert_eq!(original.content(), "needle primary text");
        assert_ne!(wrong, destination);

        // The handler must select the owning checkout BEFORE opening the
        // resolved absolute path, even if .worktrees physically nests under
        // the primary checkout directory.
        let mut selected = crate::model::WorkbenchModel::new(&linked).unwrap();
        assert!(selected.open_path(&destination));
        assert_eq!(selected.selected_path(), Some(destination.as_path()));
        assert_eq!(selected.content(), "needle linked text");
    }

    #[test]
    fn e05_deleted_or_escaped_hit_is_never_opened_from_another_workspace() {
        let (_fixture, main, linked) = two_worktrees_with_same_relative_file();
        let mut results =
            search_worktree_contents(&main, "needle", WorktreeContentScope::AllWorktrees).unwrap();
        let linked_hit = results
            .matches
            .iter()
            .find(|hit| hit.root == linked)
            .unwrap();
        fs::remove_file(linked_hit.checked_path().unwrap()).unwrap();
        assert!(
            linked_hit.checked_path().is_err(),
            "stale destination refused"
        );
        let escaped = results.matches.first_mut().unwrap();
        escaped.hit.path = PathBuf::from("../src/same.txt");
        assert!(
            escaped.checked_path().is_err(),
            "relative path escape refused"
        );
    }
}
