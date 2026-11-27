//! Left-sidebar project -> worktree grouping for the GPUI preview.
//!
//! Collects the saved projects, recent workspaces and the active workspace
//! into one deduplicated [`ProjectTreeGroup`] per repository. Git checkouts
//! group under their *primary* worktree root, so a linked worktree appearing
//! in any input list still lands in the same group as the main checkout.
//! Non-Git directories become plain roots without worktrees. Collection is
//! read-only: it canonicalizes paths and asks [`WorktreeManager`] for
//! `git worktree list`; it never writes, moves or deletes anything.
use crate::runtime_catalog::SidebarCatalogIndex;
use crate::worktree_manager::{WorktreeEntry, WorktreeManager, WorktreeReadSnapshot};
use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// Mirrors the persisted caps in `session::PreviewSessionStore`; callers may
/// pass longer slices and scanning still stays bounded.
const MAX_SAVED_PROJECTS: usize = 32;
const MAX_RECENT_WORKSPACES: usize = 12;
const MAX_REGISTERED_PROJECTS: usize = 64;

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
    collect_project_tree_with_registered(saved, recent, &[], active)
}

/// Merge the canonical RuntimeStore registry without treating all registered
/// projects as pinned; the native preview's saved set remains independent.
pub fn collect_project_tree_with_registered(
    saved: &[PathBuf],
    recent: &[PathBuf],
    registered: &[PathBuf],
    active: &Path,
) -> Vec<ProjectTreeGroup> {
    collect_project_tree_with_snapshot(
        saved,
        recent,
        registered,
        active,
        &mut WorktreeReadSnapshot::default(),
    )
}

/// Reuse successful catalog validation reads during the same sidebar refresh.
pub fn collect_project_tree_with_snapshot(
    saved: &[PathBuf],
    recent: &[PathBuf],
    registered: &[PathBuf],
    active: &Path,
    snapshot: &mut WorktreeReadSnapshot,
) -> Vec<ProjectTreeGroup> {
    let mut state = State::default();
    for path in saved.iter().take(MAX_SAVED_PROJECTS) {
        collect_candidate(path, true, false, &mut state, snapshot);
    }
    for path in registered.iter().take(MAX_REGISTERED_PROJECTS) {
        collect_candidate(path, false, false, &mut state, snapshot);
    }
    for path in recent.iter().take(MAX_RECENT_WORKSPACES) {
        collect_candidate(path, false, false, &mut state, snapshot);
    }
    collect_candidate(active, false, true, &mut state, snapshot);
    state.groups
}

/// Folder projects do not have Git worktree metadata, but their registered
/// primary workspace still needs Classic workspace actions in Native.
/// This view-only synthetic row can never be removed as a Git checkout.
pub fn sidebar_workspaces_for_group(
    group: &ProjectTreeGroup,
    catalog: Option<&crate::runtime_catalog::RuntimeCatalog>,
) -> Vec<WorktreeEntry> {
    if !group.worktrees.is_empty() {
        return group.worktrees.clone();
    }
    let has_registered_primary = catalog.is_some_and(|catalog| {
        catalog
            .workspaces
            .iter()
            .any(|workspace| workspace.path == group.root)
    });
    if !has_registered_primary {
        return Vec::new();
    }
    vec![WorktreeEntry {
        path: group.root.clone(),
        branch: None,
        detached: false,
        locked: false,
        prunable: false,
        is_primary: true,
    }]
}

/// Retain the global project tree while the replacement snapshot loads.
/// Selection follows the target immediately; the separate Git Worktrees list
/// is only retained when the target is a known checkout of that repository.
/// Call with canonical paths after a successful workspace switch.
pub fn reconcile_sidebar_on_workspace_switch(
    groups: &mut [ProjectTreeGroup],
    worktrees: &mut Vec<WorktreeEntry>,
    active: &Path,
) {
    for group in groups {
        group.is_active =
            group.root == active || group.worktrees.iter().any(|entry| entry.path == active);
    }
    if !worktrees.iter().any(|entry| entry.path == active) {
        worktrees.clear();
    }
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

fn collect_candidate(
    path: &Path,
    is_saved: bool,
    is_active: bool,
    state: &mut State,
    snapshot: &mut WorktreeReadSnapshot,
) {
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
    let resolution = resolve(&canonical, &mut state.owners, snapshot);
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
fn resolve(
    path: &Path,
    owners: &mut HashMap<PathBuf, PathBuf>,
    snapshot: &mut WorktreeReadSnapshot,
) -> Resolution {
    match WorktreeManager::discover_canonical(path) {
        Ok(manager) => {
            let worktree_root = manager.root().to_path_buf();
            match snapshot.list(&manager) {
                Ok(entries) => {
                    let root = entries
                        .iter()
                        .find(|entry| entry.is_primary)
                        .map(|entry| entry.path.clone())
                        .unwrap_or_else(|| worktree_root.clone());
                    for entry in entries {
                        owners.insert(entry.path.clone(), root.clone());
                    }
                    owners.insert(worktree_root, root.clone());
                    owners.insert(path.to_path_buf(), root.clone());
                    Resolution {
                        root,
                        worktrees: entries.to_vec(),
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

/// Project-list search uses Classic display names in addition to Git roots.
/// Keep filtering entirely read-only: searching must never enumerate or create
/// worktrees or mutate the RuntimeStore registry.
pub fn project_title_matches_search(
    group: &ProjectTreeGroup,
    catalog: Option<&crate::runtime_catalog::RuntimeCatalog>,
    query: &str,
) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    group.label.to_lowercase().contains(&query)
        || group.root.to_string_lossy().to_lowercase().contains(&query)
        || catalog.is_some_and(|catalog| {
            catalog.projects.iter().any(|project| {
                project.root == group.root && project.name.to_lowercase().contains(&query)
            })
        })
}

/// Search worktree branch, registered display name, tags, section and path.
/// Do not search internal IDs or hidden metadata not shown in the UI.
pub fn worktree_matches_search(
    entry: &WorktreeEntry,
    catalog: Option<&crate::runtime_catalog::RuntimeCatalog>,
    query: &str,
) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    entry.path.to_string_lossy().to_lowercase().contains(&query)
        || entry
            .branch
            .as_deref()
            .is_some_and(|branch| branch.to_lowercase().contains(&query))
        || catalog.is_some_and(|catalog| {
            catalog.workspaces.iter().any(|workspace| {
                workspace.path == entry.path
                    && (workspace.name.to_lowercase().contains(&query)
                        || workspace
                            .branch
                            .as_deref()
                            .is_some_and(|branch| branch.to_lowercase().contains(&query))
                        || workspace
                            .tag_names
                            .iter()
                            .any(|tag| tag.to_lowercase().contains(&query))
                        || workspace.section_id.as_deref().is_some_and(|section_id| {
                            catalog.sections.iter().any(|section| {
                                section.id == section_id
                                    && section.name.to_lowercase().contains(&query)
                            })
                        }))
            })
        })
}

/// Search using the same normalized query and catalog index across every
/// project and worktree in a sidebar projection.
pub struct SidebarSearch<'a, 'catalog> {
    query: String,
    catalog: &'a SidebarCatalogIndex<'catalog>,
}

impl<'a, 'catalog> SidebarSearch<'a, 'catalog> {
    pub fn new(query: &str, catalog: &'a SidebarCatalogIndex<'catalog>) -> Self {
        Self {
            query: query.trim().to_lowercase(),
            catalog,
        }
    }

    pub fn project_matches(&self, group: &ProjectTreeGroup) -> bool {
        let query = &self.query;
        query.is_empty()
            || group.label.to_lowercase().contains(query)
            || group.root.to_string_lossy().to_lowercase().contains(query)
            || self
                .catalog
                .projects_at(&group.root)
                .iter()
                .any(|project| project.name.to_lowercase().contains(query))
    }

    pub fn worktree_matches(&self, entry: &WorktreeEntry) -> bool {
        let query = &self.query;
        query.is_empty()
            || entry.path.to_string_lossy().to_lowercase().contains(query)
            || entry
                .branch
                .as_deref()
                .is_some_and(|branch| branch.to_lowercase().contains(query))
            || self
                .catalog
                .workspaces_at(&entry.path)
                .iter()
                .any(|workspace| {
                    workspace.name.to_lowercase().contains(query)
                        || workspace
                            .branch
                            .as_deref()
                            .is_some_and(|branch| branch.to_lowercase().contains(query))
                        || workspace
                            .tag_names
                            .iter()
                            .any(|tag| tag.to_lowercase().contains(query))
                        || workspace.section_id.as_deref().is_some_and(|id| {
                            self.catalog
                                .sections_with_id(id)
                                .iter()
                                .any(|section| section.name.to_lowercase().contains(query))
                        })
                })
    }
}

/// A matching branch must keep its parent project visible even when its
/// project name does not match, as in Classic's sidebar workspace search.
pub fn project_group_matches_search(
    group: &ProjectTreeGroup,
    catalog: Option<&crate::runtime_catalog::RuntimeCatalog>,
    query: &str,
) -> bool {
    project_title_matches_search(group, catalog, query)
        || sidebar_workspaces_for_group(group, catalog)
            .iter()
            .any(|entry| worktree_matches_search(entry, catalog, query))
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::{Repository, Signature};
    use std::fs;

    #[test]
    fn switching_known_worktrees_keeps_both_sidebar_lists_and_updates_selection() {
        let primary = PathBuf::from("C:/repo/primary");
        let linked = PathBuf::from("C:/repo/.worktrees/feature");
        let entries = vec![
            WorktreeEntry {
                path: primary.clone(),
                branch: Some("main".into()),
                detached: false,
                locked: false,
                prunable: false,
                is_primary: true,
            },
            WorktreeEntry {
                path: linked.clone(),
                branch: Some("feature".into()),
                detached: false,
                locked: false,
                prunable: false,
                is_primary: false,
            },
        ];
        let mut groups = vec![ProjectTreeGroup {
            root: primary.clone(),
            label: "repo".into(),
            is_saved: true,
            is_active: true,
            worktrees: entries.clone(),
            error: None,
        }];
        let mut visible_worktrees = entries.clone();

        reconcile_sidebar_on_workspace_switch(&mut groups, &mut visible_worktrees, &linked);
        assert_eq!(
            groups.len(),
            1,
            "project tree must not disappear during refresh"
        );
        assert!(groups[0].is_saved);
        assert!(groups[0].is_active);
        assert_eq!(
            visible_worktrees, entries,
            "same-repo worktree rows stay visible"
        );

        reconcile_sidebar_on_workspace_switch(&mut groups, &mut visible_worktrees, &primary);
        assert!(groups[0].is_active);
        assert_eq!(visible_worktrees, entries);
    }

    #[test]
    fn switching_projects_preserves_global_tree_but_clears_unrelated_git_rows() {
        let first = PathBuf::from("C:/repo/first");
        let second = PathBuf::from("C:/repo/second");
        let entry = WorktreeEntry {
            path: first.clone(),
            branch: Some("main".into()),
            detached: false,
            locked: false,
            prunable: false,
            is_primary: true,
        };
        let mut groups = vec![
            ProjectTreeGroup {
                root: first.clone(),
                label: "first".into(),
                is_saved: true,
                is_active: true,
                worktrees: vec![entry.clone()],
                error: None,
            },
            ProjectTreeGroup {
                root: second.clone(),
                label: "second".into(),
                is_saved: true,
                is_active: false,
                worktrees: vec![],
                error: None,
            },
        ];
        let mut visible_worktrees = vec![entry];

        reconcile_sidebar_on_workspace_switch(&mut groups, &mut visible_worktrees, &second);
        assert_eq!(
            groups.len(),
            2,
            "global project navigation must remain visible"
        );
        assert!(!groups[0].is_active);
        assert!(groups[1].is_active);
        assert!(
            visible_worktrees.is_empty(),
            "do not display a different repository's Git list"
        );
    }

    fn canonical(path: &Path) -> PathBuf {
        dunce::canonicalize(path).unwrap()
    }

    #[test]
    fn sidebar_search_finds_classic_project_workspace_branch_tag_and_section() {
        use crate::runtime_catalog::{
            RuntimeCatalog, RuntimeProject, RuntimeSection, RuntimeWorkspace,
        };
        let root = PathBuf::from("C:/repo/main");
        let linked = PathBuf::from("C:/worktrees/feature");
        let group = ProjectTreeGroup {
            root: root.clone(),
            label: "main".into(),
            is_saved: true,
            is_active: false,
            worktrees: vec![WorktreeEntry {
                path: linked.clone(),
                branch: Some("feature/sidebar-ui".into()),
                detached: false,
                locked: false,
                prunable: false,
                is_primary: false,
            }],
            error: None,
        };
        let catalog = RuntimeCatalog {
            projects: vec![RuntimeProject {
                id: "p1".into(),
                name: "使用者專案".into(),
                root,
            }],
            workspaces: vec![RuntimeWorkspace {
                id: "w1".into(),
                project_id: "p1".into(),
                name: "設計工作區".into(),
                path: linked.clone(),
                branch: Some("feature/sidebar-ui".into()),
                is_pinned: true,
                tag_names: vec!["重要".into()],
                tag_ids: vec!["t1".into()],
                section_id: Some("s1".into()),
                parent_workspace_id: None,
                is_archived: false,
            }],
            sections: vec![RuntimeSection {
                id: "s1".into(),
                name: "設計分類".into(),
            }],
            ..Default::default()
        };
        assert!(project_title_matches_search(
            &group,
            Some(&catalog),
            "使用者"
        ));
        for term in ["sidebar-UI", "設計工作", "重要", "設計分類", "WORKTREES"] {
            assert!(
                project_group_matches_search(&group, Some(&catalog), term),
                "{term}"
            );
        }
        assert!(!project_title_matches_search(
            &group,
            Some(&catalog),
            "sidebar-UI"
        ));
        assert!(worktree_matches_search(
            &group.worktrees[0],
            Some(&catalog),
            "重要"
        ));
        assert!(project_group_matches_search(&group, Some(&catalog), "  "));
        assert!(!project_group_matches_search(
            &group,
            Some(&catalog),
            "no-such-name"
        ));
        assert!(!worktree_matches_search(&group.worktrees[0], None, "重要"));
    }

    #[test]
    fn registered_plain_folder_has_primary_workspace_row_without_git() {
        use crate::runtime_catalog::{RuntimeCatalog, RuntimeWorkspace};
        let root = PathBuf::from("X:/plain-folder");
        let group = ProjectTreeGroup {
            root: root.clone(),
            label: "plain-folder".into(),
            is_saved: false,
            is_active: false,
            worktrees: Vec::new(),
            error: None,
        };
        assert!(sidebar_workspaces_for_group(&group, None).is_empty());
        let catalog = RuntimeCatalog {
            workspaces: vec![RuntimeWorkspace {
                id: "ws1".into(),
                project_id: "p".into(),
                name: "Folder".into(),
                path: root.clone(),
                branch: None,
                is_pinned: false,
                tag_names: vec![],
                tag_ids: vec![],
                section_id: None,
                parent_workspace_id: None,
                is_archived: false,
            }],
            ..Default::default()
        };
        let rows = sidebar_workspaces_for_group(&group, Some(&catalog));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, root);
        assert!(rows[0].is_primary);
        assert!(!rows[0].locked);
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

    // A real RuntimeStore-shaped fixture: registered primary and linked
    // checkouts, each with separate IDs and display metadata.
    fn sidebar_fixture(
        count: usize,
    ) -> (
        tempfile::TempDir,
        serde_json::Value,
        serde_json::Value,
        Vec<PathBuf>,
        Vec<PathBuf>,
    ) {
        use serde_json::json;
        let temp = tempfile::tempdir().unwrap();
        let mut projects = Vec::new();
        let mut workspaces = Vec::new();
        let mut roots = Vec::new();
        let mut linked_paths = Vec::new();
        for index in 0..count {
            let manager = init_repo(&temp.path().join(format!("repo-{index:02}")));
            let root = canonical(manager.root());
            // Set up through libgit2 to avoid timing fixture subprocesses.
            let repo = Repository::open(&root).unwrap();
            let commit = repo.head().unwrap().peel_to_commit().unwrap();
            let reference = repo
                .branch("feature", &commit, false)
                .unwrap()
                .into_reference();
            let mut options = git2::WorktreeAddOptions::new();
            options.reference(Some(&reference));
            let linked = temp.path().join(format!("linked-{index:02}"));
            repo.worktree("feature", &linked, Some(&options)).unwrap();
            let linked = canonical(&linked);
            let id = format!("project-{index}");
            projects.push(json!({"id":id,"name":id,"repoPath":root}));
            workspaces.push(json!({"id":format!("main-{index}"),"projectId":id,"name":"Main","path":root,"isPinned":true}));
            workspaces.push(json!({"id":format!("feature-{index}"),"projectId":id,"name":"Feature","path":linked,"branch":"feature","tagNames":["Review"]}));
            // A different project's existing checkout must not be accepted.
            if index > 0 {
                workspaces.push(json!({"id":format!("foreign-{index}"),"projectId":id,"name":"Foreign","path":linked_paths[0]}));
            }
            roots.push(root);
            linked_paths.push(linked);
        }
        (
            temp,
            json!({"kind":"projects","items":projects}),
            json!({"kind":"workspaces","items":workspaces}),
            roots,
            linked_paths,
        )
    }

    fn collect_sidebar_fixture(
        projects: &serde_json::Value,
        workspaces: &serde_json::Value,
        roots: &[PathBuf],
        linked: &[PathBuf],
        reuse: bool,
    ) -> (
        crate::runtime_catalog::RuntimeCatalog,
        Vec<ProjectTreeGroup>,
        usize,
    ) {
        let mut snapshot = WorktreeReadSnapshot::default();
        snapshot.bypass_cache = !reuse;
        let catalog = crate::runtime_catalog::parse_runtime_catalog_with_snapshot(
            projects,
            workspaces,
            &mut snapshot,
        )
        .unwrap();
        let registered: Vec<_> = catalog
            .projects
            .iter()
            .map(|project| project.root.clone())
            .collect();
        let groups = collect_project_tree_with_snapshot(
            roots,
            linked,
            &registered,
            &linked[0],
            &mut snapshot,
        );
        (catalog, groups, snapshot.list_reads)
    }

    #[test]
    fn catalog_and_project_tree_share_only_exact_verified_worktree_listings() {
        let (_temp, projects, workspaces, roots, linked) = sidebar_fixture(4);
        let old = collect_sidebar_fixture(&projects, &workspaces, &roots, &linked, false);
        let new = collect_sidebar_fixture(&projects, &workspaces, &roots, &linked, true);
        assert_eq!(new.0, old.0);
        assert_eq!(new.1, old.1);
        assert_eq!(new.0.workspaces.len(), 8);
        assert_eq!(new.1.len(), 4);
        assert_eq!(old.2, 8);
        assert_eq!(new.2, 4);
        assert!(new.1[0].is_active);
        assert!(new.1.iter().all(|group| group.worktrees.len() == 2));
        // Next refresh must see newly created worktrees, never a stale cache.
        let manager = WorktreeManager::discover(&roots[0]).unwrap();
        let created = manager.create("next-refresh").unwrap();
        let next = collect_sidebar_fixture(&projects, &workspaces, &roots, &linked, true);
        assert_eq!(next.2, 4);
        assert!(next.1[0]
            .worktrees
            .iter()
            .any(|entry| entry.path == canonical(&created)));
    }

    #[test]
    fn nested_paths_reuse_discovered_root_but_nested_repository_stays_separate() {
        let temp = tempfile::tempdir().unwrap();
        let primary = init_repo(&temp.path().join("outer"));
        let root = canonical(primary.root());
        let first = plain_dir(&root.join("first"));
        let second = plain_dir(&root.join("second"));
        let nested = init_repo(&root.join("independent"));
        let nested_root = canonical(nested.root());
        let nested_subdir = plain_dir(&nested_root.join("subdir"));
        let saved = vec![root.clone(), first, second, nested_subdir];
        let mut old = WorktreeReadSnapshot::default();
        old.bypass_cache = true;
        let expected = collect_project_tree_with_snapshot(&saved, &[], &[], &nested_root, &mut old);
        let mut snapshot = WorktreeReadSnapshot::default();
        let actual =
            collect_project_tree_with_snapshot(&saved, &[], &[], &nested_root, &mut snapshot);
        assert_eq!(actual, expected);
        assert_eq!(old.list_reads, 4);
        assert_eq!(snapshot.list_reads, 2);
        assert_eq!(actual.len(), 2);
        assert_eq!(actual[0].root, root);
        assert_eq!(actual[1].root, nested_root);
        assert!(actual[1].is_active);
    }

    #[test]
    #[ignore = "real repository sidebar refresh benchmark"]
    fn benchmark_shared_sidebar_worktree_snapshot() {
        use std::time::{Duration, Instant};
        let (_temp, projects, workspaces, roots, linked) = sidebar_fixture(12);
        let expected = collect_sidebar_fixture(&projects, &workspaces, &roots, &linked, false);
        let warm = collect_sidebar_fixture(&projects, &workspaces, &roots, &linked, true);
        assert_eq!(warm.0, expected.0);
        assert_eq!(warm.1, expected.1);
        assert_eq!(expected.2, 24);
        assert_eq!(warm.2, 12);
        let mut old = Vec::new();
        let mut new = Vec::new();
        for sample in 0..7 {
            for reuse in if sample % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            } {
                let start = Instant::now();
                let actual =
                    collect_sidebar_fixture(&projects, &workspaces, &roots, &linked, reuse);
                let elapsed = start.elapsed();
                assert_eq!(actual.0, expected.0);
                assert_eq!(actual.1, expected.1);
                assert_eq!(actual.2, if reuse { 12 } else { 24 });
                if reuse {
                    new.push(elapsed);
                } else {
                    old.push(elapsed);
                }
            }
        }
        fn median(samples: &mut [Duration]) -> f64 {
            samples.sort();
            samples[samples.len() / 2].as_secs_f64() * 1000.0
        }
        let old = median(&mut old);
        let new = median(&mut new);
        println!("GPUI sidebar refresh repositories={} registered_workspaces={} groups={} git_list_old={} git_list_new={} old_ms={old:.3} new_ms={new:.3} speedup={:.2}x", roots.len(), expected.0.workspaces.len(), expected.1.len(), expected.2, warm.2, old/new);
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
    fn runtime_registered_projects_show_without_becoming_pinned() {
        let temp = tempfile::tempdir().unwrap();
        let root = plain_dir(&temp.path().join("registered"));
        let active = plain_dir(&temp.path().join("active"));
        let groups = collect_project_tree_with_registered(&[], &[], &[root.clone()], &active);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].root, root);
        assert!(!groups[0].is_saved);
        assert!(!groups[0].is_active);
        assert!(groups[1].is_active);
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
