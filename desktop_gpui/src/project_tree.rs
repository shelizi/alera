//! Left-sidebar project -> worktree grouping for the GPUI preview.
//!
//! Collects the saved projects, recent workspaces and the active workspace
//! into one deduplicated [`ProjectTreeGroup`] per repository. Git checkouts
//! group under their *primary* worktree root, so a linked worktree appearing
//! in any input list still lands in the same group as the main checkout.
//! Non-Git directories become plain roots without worktrees. Collection is
//! read-only: it canonicalizes paths and asks [`WorktreeManager`] for
//! `git worktree list`; it never writes, moves or deletes anything.
use crate::worktree_manager::{WorktreeEntry, WorktreeManager};
use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// Mirrors the persisted caps in `session::PreviewSessionStore`; callers may
/// pass longer slices and scanning still stays bounded.
const MAX_SAVED_PROJECTS: usize = 32;
const MAX_RECENT_WORKSPACES: usize = 12;

/// One left-sidebar project row: a repository (or plain directory) plus all
/// of its registered Git worktrees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectTreeGroup {
    /// Canonical primary worktree root for Git repositories, or the
    /// canonical directory itself for non-Git projects.
    pub root: PathBuf,
    /// Directory display name of [`Self::root`].
    pub label: String,
    /// True when a saved project resolves into this group.
    pub is_saved: bool,
    /// True when the active workspace resolves into this group. At most one
    /// group can be active, even when the active path is a linked worktree.
    pub is_active: bool,
    /// Every worktree `git worktree list` reports, primary included. Empty
    /// for non-Git roots and when listing failed.
    pub worktrees: Vec<WorktreeEntry>,
    /// Why worktree discovery failed, when it did. `None` for healthy repos
    /// and plain non-Git directories.
    pub error: Option<String>,
}

/// Group saved projects, recent workspaces and the active workspace by Git
/// primary root. Groups are ordered by first appearance: saved projects in
/// saved order, then recent-only workspaces in recent order, then the active
/// workspace last when it is not already represented. Inputs that cannot be
/// canonicalized (for example a missing active path) are skipped.
pub fn collect_project_tree(
    saved: &[PathBuf],
    recent: &[PathBuf],
    active: &Path,
) -> Vec<ProjectTreeGroup> {
    let mut state = State::default();
    for path in saved.iter().take(MAX_SAVED_PROJECTS) {
        collect_candidate(path, true, false, &mut state);
    }
    for path in recent.iter().take(MAX_RECENT_WORKSPACES) {
        collect_candidate(path, false, false, &mut state);
    }
    collect_candidate(active, false, true, &mut state);
    state.groups
}

#[derive(Default)]
struct State {
    groups: Vec<ProjectTreeGroup>,
    /// Group root -> index into `groups`.
    positions: HashMap<PathBuf, usize>,
    /// Any canonical path inside a repository -> its group root, learned from
    /// successful `git worktree list` output so later candidates skip Git.
    owners: HashMap<PathBuf, PathBuf>,
}

struct Resolution {
    root: PathBuf,
    worktrees: Vec<WorktreeEntry>,
    error: Option<String>,
}

fn collect_candidate(path: &Path, is_saved: bool, is_active: bool, state: &mut State) {
    // Inputs are expected canonical already, but recanonicalizing keeps the
    // grouping correct for aliases and keeps every `root` canonical.
    let Ok(canonical) = dunce::canonicalize(path) else {
        return;
    };
    if let Some(root) = state.owners.get(&canonical) {
        if let Some(&index) = state.positions.get(root) {
            let group = &mut state.groups[index];
            group.is_saved |= is_saved;
            group.is_active |= is_active;
            return;
        }
    }
    let resolution = resolve(&canonical, &mut state.owners);
    let index = match state.positions.get(&resolution.root) {
        Some(&index) => index,
        None => {
            let index = state.groups.len();
            state.positions.insert(resolution.root.clone(), index);
            state.groups.push(ProjectTreeGroup {
                root: resolution.root.clone(),
                label: label_for(&resolution.root),
                is_saved,
                is_active,
                worktrees: resolution.worktrees,
                error: resolution.error,
            });
            index
        }
    };
    let group = &mut state.groups[index];
    group.is_saved |= is_saved;
    group.is_active |= is_active;
}

/// Resolve one canonical path to its group root. Git checkouts resolve to
/// the primary worktree reported by `git worktree list` — `manager.root()`
/// alone is not enough because discovering from a linked worktree yields the
/// linked path, not the primary. Everything else resolves to the path
/// itself. Successful listings also teach `owners` every worktree path (and
/// the input path itself) so later candidates of the same repository skip
/// Git entirely.
fn resolve(path: &Path, owners: &mut HashMap<PathBuf, PathBuf>) -> Resolution {
    match WorktreeManager::discover(path) {
        Ok(manager) => {
            let worktree_root = manager.root().to_path_buf();
            match manager.list() {
                Ok(entries) => {
                    let root = entries
                        .iter()
                        .find(|entry| entry.is_primary)
                        .map(|entry| entry.path.clone())
                        .unwrap_or_else(|| worktree_root.clone());
                    for entry in &entries {
                        owners.insert(entry.path.clone(), root.clone());
                    }
                    owners.insert(worktree_root, root.clone());
                    owners.insert(path.to_path_buf(), root.clone());
                    Resolution {
                        root,
                        worktrees: entries,
                        error: None,
                    }
                }
                Err(error) => {
                    owners.insert(path.to_path_buf(), worktree_root.clone());
                    owners.insert(worktree_root.clone(), worktree_root.clone());
                    Resolution {
                        root: worktree_root,
                        worktrees: Vec::new(),
                        error: Some(error.to_string()),
                    }
                }
            }
        }
        Err(error) => {
            let root = path.to_path_buf();
            owners.insert(root.clone(), root.clone());
            // git2 discovery failures surface as NotFound whether the path
            // simply is not a repository or the repository is unreadable;
            // only other kinds (e.g. bare repositories) are reported.
            let error = (error.kind() != ErrorKind::NotFound).then(|| error.to_string());
            Resolution {
                root,
                worktrees: Vec::new(),
                error,
            }
        }
    }
}

fn label_for(root: &Path) -> String {
    root.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::{Repository, Signature};
    use std::fs;

    fn canonical(path: &Path) -> PathBuf {
        dunce::canonicalize(path).unwrap()
    }

    fn init_repo(dir: &Path) -> WorktreeManager {
        fs::create_dir_all(dir).unwrap();
        let repo = Repository::init(dir).unwrap();
        fs::write(dir.join("README"), "initial").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("README")).unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let signature = Signature::now("GPUI Test", "gpui@example.invalid").unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
            .unwrap();
        drop(tree);
        drop(repo);
        WorktreeManager::discover(dir).unwrap()
    }

    fn plain_dir(dir: &Path) -> PathBuf {
        fs::create_dir_all(dir).unwrap();
        canonical(dir)
    }

    #[test]
    fn saved_linked_worktree_and_recent_primary_share_one_group() {
        let temp = tempfile::tempdir().unwrap();
        let base = canonical(temp.path());
        let manager = init_repo(&base.join("alpha"));
        let linked = canonical(&manager.create("feature/linked").unwrap());
        let primary = canonical(manager.root());

        let groups = collect_project_tree(&[linked.clone()], &[primary.clone()], &primary);

        assert_eq!(groups.len(), 1);
        let group = &groups[0];
        assert_eq!(group.root, primary);
        assert_eq!(group.label, "alpha");
        assert!(group.is_saved);
        assert!(group.is_active);
        assert!(group.error.is_none());
        assert_eq!(group.worktrees.len(), 2);
        assert_eq!(
            group
                .worktrees
                .iter()
                .filter(|entry| entry.is_primary)
                .count(),
            1
        );
        assert!(group.worktrees.iter().any(|entry| entry.path == linked));
        assert!(group.worktrees.iter().any(|entry| entry.path == primary));
    }

    #[test]
    fn duplicate_worktree_roots_and_non_git_dirs_dedupe() {
        let temp = tempfile::tempdir().unwrap();
        let base = canonical(temp.path());
        let manager = init_repo(&base.join("alpha"));
        let linked = canonical(&manager.create("feature/linked").unwrap());
        let primary = canonical(manager.root());
        let plain = plain_dir(&base.join("plain"));

        let groups = collect_project_tree(
            &[
                primary.clone(),
                linked.clone(),
                plain.clone(),
                primary.clone(),
            ],
            &[linked.clone(), plain.clone()],
            &linked,
        );

        assert_eq!(groups.len(), 2);
        let repo_group = groups.iter().find(|group| group.root == primary).unwrap();
        assert!(repo_group.is_saved);
        // The active path is a linked worktree: it marks the primary-rooted
        // group and never becomes an ambiguous second group of its own.
        assert!(repo_group.is_active);
        assert_eq!(repo_group.worktrees.len(), 2);
        let plain_group = groups.iter().find(|group| group.root == plain).unwrap();
        assert!(plain_group.is_saved);
        assert!(!plain_group.is_active);
        assert!(plain_group.worktrees.is_empty());
        assert!(plain_group.error.is_none());
        assert_eq!(plain_group.label, "plain");
        assert!(!groups.iter().any(|group| group.root == linked));
    }

    #[test]
    fn groups_follow_saved_recent_active_order() {
        let temp = tempfile::tempdir().unwrap();
        let base = canonical(temp.path());
        let manager_a = init_repo(&base.join("aaa-saved"));
        let primary_a = canonical(manager_a.root());
        let linked_a = canonical(&manager_a.create("feature/x").unwrap());
        let manager_b = init_repo(&base.join("bbb-recent"));
        let primary_b = canonical(manager_b.root());
        let plain_c = plain_dir(&base.join("ccc-active"));

        let groups = collect_project_tree(
            &[primary_a.clone()],
            &[primary_b.clone(), linked_a.clone()],
            &plain_c,
        );

        let roots: Vec<&Path> = groups.iter().map(|group| group.root.as_path()).collect();
        assert_eq!(
            roots,
            vec![primary_a.as_path(), primary_b.as_path(), plain_c.as_path()]
        );
        assert!(groups[0].is_saved && !groups[0].is_active);
        assert!(!groups[1].is_saved && !groups[1].is_active);
        assert!(!groups[2].is_saved && groups[2].is_active);
        // The recent linked worktree merged into the saved group instead of
        // creating its own entry.
        assert_eq!(groups[0].worktrees.len(), 2);
    }

    #[test]
    fn non_git_dirs_and_bare_repos_become_plain_or_errored_roots() {
        let temp = tempfile::tempdir().unwrap();
        let base = canonical(temp.path());
        let plain = plain_dir(&base.join("plain"));
        let bare = base.join("bare");
        fs::create_dir_all(&bare).unwrap();
        Repository::init_bare(&bare).unwrap();
        let bare = canonical(&bare);

        let groups = collect_project_tree(&[plain.clone(), bare.clone()], &[], &plain);

        assert_eq!(groups.len(), 2);
        let plain_group = groups.iter().find(|group| group.root == plain).unwrap();
        assert!(plain_group.worktrees.is_empty());
        assert!(plain_group.error.is_none());
        assert!(plain_group.is_saved && plain_group.is_active);
        let bare_group = groups.iter().find(|group| group.root == bare).unwrap();
        assert!(bare_group.worktrees.is_empty());
        assert!(bare_group.error.is_some());
    }

    #[test]
    fn scanning_is_bounded_to_store_limits() {
        let temp = tempfile::tempdir().unwrap();
        let base = canonical(temp.path());
        let saved: Vec<PathBuf> = (0..40)
            .map(|index| plain_dir(&base.join(format!("saved-{index:02}"))))
            .collect();
        let recent: Vec<PathBuf> = (0..15)
            .map(|index| plain_dir(&base.join(format!("recent-{index:02}"))))
            .collect();
        let active = plain_dir(&base.join("active"));

        let groups = collect_project_tree(&saved, &recent, &active);

        assert_eq!(groups.len(), MAX_SAVED_PROJECTS + MAX_RECENT_WORKSPACES + 1);
        assert_eq!(groups[0].root, saved[0]);
        assert_eq!(
            groups[MAX_SAVED_PROJECTS - 1].root,
            saved[MAX_SAVED_PROJECTS - 1]
        );
        assert!(!groups
            .iter()
            .any(|group| group.root == saved[MAX_SAVED_PROJECTS]));
        assert_eq!(groups[MAX_SAVED_PROJECTS].root, recent[0]);
        assert!(!groups
            .iter()
            .any(|group| group.root == recent[MAX_RECENT_WORKSPACES]));
        assert_eq!(groups.last().unwrap().root, active);
        assert!(groups.last().unwrap().is_active);
    }
}
