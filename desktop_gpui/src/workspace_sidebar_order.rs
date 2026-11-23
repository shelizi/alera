//! Read-only ordering for native project > section > worktree descendants.
//! Classic RuntimeStore owns relationships; GPUI never duplicates persistence.
use crate::runtime_catalog::RuntimeCatalog;
use crate::worktree_manager::WorktreeEntry;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct SidebarWorktreeRow {
    pub entry: WorktreeEntry,
    pub depth: usize,
    /// Printed once at the beginning of each top-level section.
    pub section_header: Option<String>,
}

pub fn visible_worktree_rows(
    rows: Vec<SidebarWorktreeRow>,
    collapsed: &HashSet<PathBuf>,
) -> impl Iterator<Item = (SidebarWorktreeRow, bool)> + '_ {
    let mut rows = rows.into_iter().peekable();
    let mut hidden_below = None;
    std::iter::from_fn(move || {
        while let Some(row) = rows.next() {
            if hidden_below.is_some_and(|depth| row.depth > depth) {
                continue;
            }
            let has_children = rows.peek().is_some_and(|next| next.depth > row.depth);
            hidden_below = if has_children && collapsed.contains(&row.entry.path) {
                Some(row.depth)
            } else {
                None
            };
            return Some((row, has_children));
        }
        None
    })
}

/// A cycle or stale parent cannot hide or infinitely recurse any checkout.
/// Roots are ordered: main, pinned, section name, ungrouped, archived.
pub fn order_worktree_rows(
    entries: &[WorktreeEntry],
    catalog: Option<&RuntimeCatalog>,
) -> Vec<SidebarWorktreeRow> {
    order_worktree_rows_with_manual(entries, catalog, None)
}

/// Custom ordering applies within one section/pin/archive group and retains
/// parent-child hierarchy. IDs resolve through the Host RuntimeCatalog.
pub fn order_worktree_rows_with_manual(
    entries: &[WorktreeEntry],
    catalog: Option<&RuntimeCatalog>,
    manual_order: Option<&[String]>,
) -> Vec<SidebarWorktreeRow> {
    const LIMIT: usize = 96;
    let entries: Vec<WorktreeEntry> = entries.iter().take(LIMIT).cloned().collect();
    let mut nodes: Vec<(
        WorktreeEntry,
        Option<String>,
        Option<String>,
        String,
        bool,
        bool,
    )> = Vec::with_capacity(entries.len());
    let id_by_path: HashMap<_, _> = catalog
        .into_iter()
        .flat_map(|catalog| catalog.workspaces.iter())
        .map(|workspace| (workspace.path.clone(), workspace.id.clone()))
        .collect();
    let known: HashSet<_> = entries
        .iter()
        .filter_map(|entry| id_by_path.get(&entry.path).cloned())
        .collect();

    for entry in entries {
        let workspace = catalog.and_then(|catalog| {
            catalog
                .workspaces
                .iter()
                .find(|workspace| workspace.path == entry.path)
        });
        let section = workspace
            .and_then(|workspace| workspace.section_id.as_ref())
            .and_then(|id| {
                catalog.and_then(|cat| cat.sections.iter().find(|section| &section.id == id))
            })
            .map(|section| section.name.clone());
        let parent = workspace
            .and_then(|workspace| workspace.parent_workspace_id.as_ref())
            .filter(|id| known.contains(*id))
            .cloned();
        let name = workspace
            .map(|workspace| workspace.name.clone())
            .or_else(|| entry.branch.clone())
            .unwrap_or_else(|| entry.path.to_string_lossy().to_string());
        let pinned = workspace.is_some_and(|workspace| workspace.is_pinned);
        let archived = workspace.is_some_and(|workspace| workspace.is_archived);
        nodes.push((entry, parent, section, name, pinned, archived));
    }

    let mut indices: Vec<usize> = (0..nodes.len()).collect();
    let positions: HashMap<&str, usize> = manual_order
        .into_iter()
        .flat_map(|ids| {
            ids.iter()
                .enumerate()
                .map(|(index, id)| (id.as_str(), index))
        })
        .collect();
    indices.sort_by(|&a, &b| {
        let x = &nodes[a];
        let y = &nodes[b];
        if manual_order.is_some() {
            // A drag changes order only, not Project, section, pin, archive,
            // or parent membership; keep those grouping boundaries stable.
            let group_x = (x.5, !x.4, x.2.as_deref().unwrap_or("~"));
            let group_y = (y.5, !y.4, y.2.as_deref().unwrap_or("~"));
            let by_group = group_x.cmp(&group_y);
            if !by_group.is_eq() {
                return by_group;
            }
            let rank = |entry: &WorktreeEntry| {
                id_by_path
                    .get(&entry.path)
                    .and_then(|id| positions.get(id.as_str()).copied())
                    .unwrap_or(usize::MAX)
            };
            let by_manual = rank(&x.0).cmp(&rank(&y.0));
            if !by_manual.is_eq() {
                return by_manual;
            }
            return x.3.to_lowercase().cmp(&y.3.to_lowercase());
        }
        let sort_key = |node: &(
            WorktreeEntry,
            Option<String>,
            Option<String>,
            String,
            bool,
            bool,
        )| {
            (
                !node.0.is_primary,
                node.5,
                !node.4,
                node.2.as_deref().unwrap_or("~").to_owned(),
                node.3.to_lowercase(),
            )
        };
        sort_key(x).cmp(&sort_key(y))
    });
    let mut rows = Vec::with_capacity(nodes.len());
    let mut visited = HashSet::<usize>::new();
    let mut last_section = None::<Option<String>>;
    fn push_descendants(
        index: usize,
        depth: usize,
        nodes: &[(
            WorktreeEntry,
            Option<String>,
            Option<String>,
            String,
            bool,
            bool,
        )],
        id_by_path: &HashMap<std::path::PathBuf, String>,
        order: &[usize],
        visited: &mut HashSet<usize>,
        rows: &mut Vec<SidebarWorktreeRow>,
        header: Option<String>,
    ) {
        if depth > 8 || !visited.insert(index) {
            return;
        }
        rows.push(SidebarWorktreeRow {
            entry: nodes[index].0.clone(),
            depth,
            section_header: header,
        });
        let Some(id) = id_by_path.get(&nodes[index].0.path) else {
            return;
        };
        for &child in order {
            if nodes[child].1.as_ref() == Some(id) && !visited.contains(&child) {
                push_descendants(
                    child,
                    depth + 1,
                    nodes,
                    id_by_path,
                    order,
                    visited,
                    rows,
                    None,
                );
            }
        }
    }

    for &index in &indices {
        if nodes[index].1.is_some() || visited.contains(&index) {
            continue;
        }
        let section = nodes[index].2.clone();
        let header = if last_section.as_ref() == Some(&section) {
            None
        } else {
            last_section = Some(section.clone());
            Some(section.unwrap_or_else(|| "Other workspaces".into()))
        };
        push_descendants(
            index,
            0,
            &nodes,
            &id_by_path,
            &indices,
            &mut visited,
            &mut rows,
            header,
        );
    }
    // Stale relationships and cycles are retained as root-level entries.
    for index in indices {
        if !visited.contains(&index) {
            push_descendants(
                index,
                0,
                &nodes,
                &id_by_path,
                &[],
                &mut visited,
                &mut rows,
                None,
            );
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_catalog::{RuntimeSection, RuntimeWorkspace};
    use std::path::PathBuf;

    #[test]
    fn worktree_disclosure_hides_only_descendants_and_preserves_nested_state() {
        let rows: Vec<_> = [("main", 0), ("child", 1), ("grandchild", 2), ("sibling", 0)]
            .into_iter()
            .map(|(name, depth)| SidebarWorktreeRow {
                entry: WorktreeEntry {
                    path: PathBuf::from(name),
                    branch: None,
                    detached: false,
                    locked: false,
                    prunable: false,
                    is_primary: depth == 0 && name == "main",
                },
                depth,
                section_header: None,
            })
            .collect();
        let visible = |collapsed: &HashSet<PathBuf>| {
            visible_worktree_rows(rows.clone(), collapsed)
                .map(|(row, has_children)| (row.entry.path, has_children))
                .collect::<Vec<_>>()
        };
        let mut collapsed = HashSet::from([PathBuf::from("main"), PathBuf::from("child")]);
        assert_eq!(
            visible(&collapsed),
            [
                (PathBuf::from("main"), true),
                (PathBuf::from("sibling"), false)
            ]
        );
        collapsed.remove(&PathBuf::from("main"));
        assert_eq!(
            visible(&collapsed),
            [
                (PathBuf::from("main"), true),
                (PathBuf::from("child"), true),
                (PathBuf::from("sibling"), false),
            ]
        );
        assert_eq!(visible(&HashSet::new()).len(), rows.len());
    }

    fn git_entry(path: &str, primary: bool) -> WorktreeEntry {
        WorktreeEntry {
            path: PathBuf::from(path),
            branch: Some(path.into()),
            detached: false,
            locked: false,
            prunable: false,
            is_primary: primary,
        }
    }

    fn workspace(path: &str, id: &str, parent: Option<&str>) -> RuntimeWorkspace {
        RuntimeWorkspace {
            id: id.into(),
            project_id: "p".into(),
            name: id.into(),
            path: PathBuf::from(path),
            branch: None,
            is_pinned: false,
            tag_names: vec![],
            tag_ids: vec![],
            section_id: None,
            parent_workspace_id: parent.map(str::to_owned),
            is_archived: false,
        }
    }

    #[test]
    fn children_follow_parent_instead_of_git_list_order() {
        let mut catalog = RuntimeCatalog::default();
        catalog
            .workspaces
            .push(workspace("C", "child", Some("parent")));
        catalog.workspaces.push(workspace("P", "parent", None));
        catalog.workspaces.push(workspace("M", "main", None));
        let rows = order_worktree_rows(
            &[
                git_entry("C", false),
                git_entry("M", true),
                git_entry("P", false),
            ],
            Some(&catalog),
        );
        let paths: Vec<_> = rows
            .iter()
            .map(|row| row.entry.path.to_string_lossy().to_string())
            .collect();
        let parent_index = paths.iter().position(|p| p == "P").unwrap();
        assert_eq!(paths[parent_index + 1], "C");
        assert_eq!(rows[parent_index + 1].depth, 1);
        assert_eq!(paths[0], "M");
    }

    #[test]
    fn orphaned_parent_and_cycles_remain_visible() {
        let mut catalog = RuntimeCatalog::default();
        catalog.workspaces.push(workspace("A", "a", Some("b")));
        catalog.workspaces.push(workspace("B", "b", Some("a")));
        catalog.workspaces.push(workspace("C", "c", Some("gone")));
        let entries = [
            git_entry("A", false),
            git_entry("B", false),
            git_entry("C", false),
        ];
        let rows = order_worktree_rows(&entries, Some(&catalog));
        assert_eq!(rows.len(), 3);
        let unique: HashSet<_> = rows.iter().map(|row| row.entry.path.clone()).collect();
        assert_eq!(unique.len(), 3);
    }

    #[test]
    fn section_and_pin_order_are_derived_from_runtime_not_native_storage() {
        let mut catalog = RuntimeCatalog::default();
        catalog.sections.push(RuntimeSection {
            id: "s".into(),
            name: "Feature".into(),
        });
        let mut pinned = workspace("P", "p", None);
        pinned.is_pinned = true;
        pinned.section_id = Some("s".into());
        catalog.workspaces.push(pinned);
        catalog.workspaces.push(workspace("U", "u", None));
        let rows = order_worktree_rows(
            &[git_entry("U", false), git_entry("P", false)],
            Some(&catalog),
        );
        assert_eq!(rows[0].entry.path, PathBuf::from("P"));
        assert_eq!(rows[0].section_header.as_deref(), Some("Feature"));
        assert_eq!(rows[1].entry.path, PathBuf::from("U"));
        assert_eq!(rows[1].section_header.as_deref(), Some("Other workspaces"));
    }

    #[test]
    fn manual_order_repositions_main_and_children_without_losing_hierarchy() {
        let mut catalog = RuntimeCatalog::default();
        catalog.workspaces.push(workspace("M", "main", None));
        catalog.workspaces.push(workspace("A", "a", None));
        catalog.workspaces.push(workspace("B", "b", None));
        catalog.workspaces.push(workspace("C", "child", Some("a")));
        let order = vec!["b".into(), "a".into(), "child".into(), "main".into()];
        let rows = order_worktree_rows_with_manual(
            &[
                git_entry("M", true),
                git_entry("C", false),
                git_entry("A", false),
                git_entry("B", false),
            ],
            Some(&catalog),
            Some(&order),
        );
        assert_eq!(
            rows.iter()
                .map(|row| row.entry.path.to_string_lossy().to_string())
                .collect::<Vec<_>>(),
            vec!["B", "A", "C", "M"]
        );
        assert_eq!(rows[2].depth, 1);
    }
}
