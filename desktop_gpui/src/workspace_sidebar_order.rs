//! Read-only ordering for native project > section > worktree descendants.
//! Classic RuntimeStore owns relationships; GPUI never duplicates persistence.
use crate::runtime_catalog::{RuntimeCatalog, SidebarCatalogIndex};
use crate::sidebar_manual_order::{SidebarActivityRank, SidebarSortContext, NO_ACTIVITY};
use crate::worktree_manager::WorktreeEntry;
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
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
    order_worktree_rows_sorted(
        entries,
        catalog,
        if manual_order.is_some() {
            "manual"
        } else {
            "name"
        },
        manual_order,
        &SidebarSortContext::default(),
    )
}

/// Keep the native section, archive, pin, and parent boundaries while
/// applying Classic's four sort modes within the available sibling groups.
pub fn order_worktree_rows_sorted(
    entries: &[WorktreeEntry],
    catalog: Option<&RuntimeCatalog>,
    mode: &str,
    manual_order: Option<&[String]>,
    sort: &SidebarSortContext,
) -> Vec<SidebarWorktreeRow> {
    order_worktree_rows_indexed(
        entries,
        &SidebarCatalogIndex::new(catalog),
        mode,
        manual_order,
        sort,
    )
}

pub fn order_worktree_rows_indexed(
    entries: &[WorktreeEntry],
    catalog: &SidebarCatalogIndex<'_>,
    mode: &str,
    manual_order: Option<&[String]>,
    sort: &SidebarSortContext,
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
    // Only selected paths can participate in the visible hierarchy.
    // Keep the original last-ID-wins rule for duplicate catalog paths.
    let id_by_path: HashMap<_, _> = entries
        .iter()
        .filter_map(|entry| {
            catalog
                .workspaces_at(&entry.path)
                .last()
                .map(|workspace| (entry.path.clone(), workspace.id.clone()))
        })
        .collect();
    let known: HashSet<_> = entries
        .iter()
        .filter_map(|entry| id_by_path.get(&entry.path).cloned())
        .collect();

    for entry in entries {
        let workspace = catalog.workspaces_at(&entry.path).first().copied();
        let section = workspace
            .and_then(|workspace| workspace.section_id.as_ref())
            .and_then(|id| catalog.sections_with_id(id).first().copied())
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
        // Sort comparators use the normalized name without reallocating it.
        nodes.push((
            entry,
            parent,
            section,
            name.to_lowercase(),
            pinned,
            archived,
        ));
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
    // A child requiring input promotes its visible parent, just as Classic.
    let mut ranks: Vec<SidebarActivityRank> = nodes
        .iter()
        .map(|node| sort.activity_for_path(&node.0.path))
        .collect();
    if mode == "activity" {
        let by_id: HashMap<_, _> = nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| id_by_path.get(&node.0.path).map(|id| (id.as_str(), index)))
            .collect();
        for index in 0..nodes.len() {
            let rank = ranks[index];
            if rank == NO_ACTIVITY {
                continue;
            }
            let mut parent = nodes[index].1.as_deref();
            let mut seen = HashSet::from([index]);
            while let Some(parent_index) = parent.and_then(|id| by_id.get(id).copied()) {
                if !seen.insert(parent_index) {
                    break;
                }
                ranks[parent_index] = ranks[parent_index].min(rank);
                parent = nodes[parent_index].1.as_deref();
            }
        }
    }
    indices.sort_by(|&a, &b| {
        let x = &nodes[a];
        let y = &nodes[b];
        if mode == "manual" {
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
            return x.3.cmp(&y.3);
        }
        if mode == "recent" || mode == "activity" {
            let key = |i: usize| {
                let n = &nodes[i];
                let main_first = mode == "recent" && !n.0.is_primary;
                let rank = if mode == "activity" {
                    ranks[i]
                } else {
                    (0, Reverse(sort.recent_for_path(&n.0.path)))
                };
                (
                    main_first,
                    n.5,
                    !n.4,
                    n.2.as_deref().unwrap_or("~"),
                    rank,
                    n.3.as_str(),
                )
            };
            return key(a).cmp(&key(b));
        }
        (
            !x.0.is_primary,
            x.5,
            !x.4,
            x.2.as_deref().unwrap_or("~"),
            x.3.as_str(),
        )
            .cmp(&(
                !y.0.is_primary,
                y.5,
                !y.4,
                y.2.as_deref().unwrap_or("~"),
                y.3.as_str(),
            ))
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

    // Test oracle: the previous per-group catalog map and comparator.
    fn legacy_order_worktree_rows_sorted(
        entries: &[WorktreeEntry],
        catalog: Option<&RuntimeCatalog>,
        mode: &str,
        manual_order: Option<&[String]>,
        sort: &SidebarSortContext,
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
        // A child requiring input promotes its visible parent, just as Classic.
        let mut ranks: Vec<SidebarActivityRank> = nodes
            .iter()
            .map(|node| sort.activity_for_path(&node.0.path))
            .collect();
        if mode == "activity" {
            let by_id: HashMap<_, _> = nodes
                .iter()
                .enumerate()
                .filter_map(|(index, node)| {
                    id_by_path.get(&node.0.path).map(|id| (id.as_str(), index))
                })
                .collect();
            for index in 0..nodes.len() {
                let rank = ranks[index];
                if rank == NO_ACTIVITY {
                    continue;
                }
                let mut parent = nodes[index].1.as_deref();
                let mut seen = HashSet::from([index]);
                while let Some(parent_index) = parent.and_then(|id| by_id.get(id).copied()) {
                    if !seen.insert(parent_index) {
                        break;
                    }
                    ranks[parent_index] = ranks[parent_index].min(rank);
                    parent = nodes[parent_index].1.as_deref();
                }
            }
        }
        indices.sort_by(|&a, &b| {
            let x = &nodes[a];
            let y = &nodes[b];
            if mode == "manual" {
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
            if mode == "recent" || mode == "activity" {
                let key = |i: usize| {
                    let n = &nodes[i];
                    let main_first = mode == "recent" && !n.0.is_primary;
                    let rank = if mode == "activity" {
                        ranks[i]
                    } else {
                        (0, Reverse(sort.recent_for_path(&n.0.path)))
                    };
                    (
                        main_first,
                        n.5,
                        !n.4,
                        n.2.as_deref().unwrap_or("~"),
                        rank,
                        n.3.to_lowercase(),
                    )
                };
                return key(a).cmp(&key(b));
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

    fn projection_fixture(
        project_count: usize,
        workspace_count: usize,
    ) -> (RuntimeCatalog, Vec<crate::project_tree::ProjectTreeGroup>) {
        use crate::project_tree::ProjectTreeGroup;
        use crate::runtime_catalog::RuntimeProject;
        let mut catalog = RuntimeCatalog::default();
        for index in 0..4 {
            catalog.sections.push(RuntimeSection {
                id: format!("section-{index}"),
                name: format!("Section {index}"),
            });
        }
        let mut groups = Vec::new();
        for project in 0..project_count {
            let root = format!("repo-{project:02}");
            let project_id = format!("project-id-{project}");
            catalog.projects.push(RuntimeProject {
                id: project_id.clone(),
                root: PathBuf::from(&root),
                name: format!("Project {project}"),
            });
            let mut entries = Vec::new();
            for index in 0..workspace_count {
                let path = format!("{root}/checkout-{index:02}");
                let id = format!("workspace-{project}-{index}");
                let parent = (index > 1 && index % 4 == 0)
                    .then(|| format!("workspace-{project}-{}", index - 1));
                let mut ws = workspace(&path, &id, parent.as_deref());
                ws.project_id = project_id.clone();
                ws.name = match index % 4 {
                    0 => "Alpha",
                    1 => "alpha",
                    2 => "Review 中文",
                    _ => "Beta",
                }
                .into();
                ws.branch = Some(format!("feature/{index}"));
                ws.tag_names = vec![format!("Tag {}", index % 3)];
                ws.section_id = Some(format!("section-{}", index % 4));
                ws.is_pinned = index % 7 == 1;
                ws.is_archived = index % 11 == 3;
                catalog
                    .workspace_updated_ms
                    .insert(id, (project * 100 + index) as i64);
                catalog.workspaces.push(ws);
                entries.push(git_entry(&path, index == 0));
            }
            entries.reverse();
            groups.push(ProjectTreeGroup {
                root: PathBuf::from(&root),
                label: format!("Repo {project}"),
                is_saved: true,
                is_active: project == 0,
                worktrees: entries,
                error: None,
            });
        }
        (catalog, groups)
    }

    #[test]
    fn indexed_sidebar_search_and_order_match_previous_projection() {
        use crate::project_tree::{
            project_title_matches_search, worktree_matches_search, SidebarSearch,
        };
        let (mut catalog, groups) = projection_fixture(4, 24);
        // Preserve first display metadata, last hierarchy ID, and any
        // duplicate's search metadata (including duplicate section IDs).
        let mut duplicate = catalog.workspaces[0].clone();
        duplicate.id = "duplicate-id".into();
        duplicate.name = "Duplicate Name".into();
        duplicate.section_id = Some("section-0".into());
        duplicate.tag_names = vec!["Special Tag".into()];
        catalog.workspaces.push(duplicate);
        catalog.sections.push(RuntimeSection {
            id: "section-0".into(),
            name: "Duplicate Section".into(),
        });
        let mut duplicate_project = catalog.projects[0].clone();
        duplicate_project.id = "duplicate-project-id".into();
        duplicate_project.name = "Second Project Name".into();
        catalog.projects.push(duplicate_project);
        catalog.workspaces[1].parent_workspace_id = Some("duplicate-id".into());
        catalog.workspaces[2].parent_workspace_id = Some(catalog.workspaces[3].id.clone());
        catalog.workspaces[3].parent_workspace_id = Some(catalog.workspaces[2].id.clone());
        let manual: Vec<_> = catalog
            .workspaces
            .iter()
            .rev()
            .map(|ws| ws.id.clone())
            .collect();
        for source in [None, Some(&catalog)] {
            let indexed = SidebarCatalogIndex::new(source);
            let sort = SidebarSortContext::new(source, None, &[], true, 1000);
            for query in [
                "",
                "  ALPHA ",
                "中文",
                "FEATURE/5",
                "tag 1",
                "duplicate section",
                "special tag",
                "second project name",
                "duplicate-id",
                "not-found",
            ] {
                let search = SidebarSearch::new(query, &indexed);
                for group in &groups {
                    assert_eq!(
                        search.project_matches(group),
                        project_title_matches_search(group, source, query)
                    );
                    for entry in &group.worktrees {
                        assert_eq!(
                            search.worktree_matches(entry),
                            worktree_matches_search(entry, source, query)
                        );
                    }
                }
            }
            for mode in ["name", "manual", "recent", "activity", "unknown"] {
                for group in &groups {
                    let order = (mode == "manual").then_some(manual.as_slice());
                    assert_eq!(
                        order_worktree_rows_indexed(&group.worktrees, &indexed, mode, order, &sort),
                        legacy_order_worktree_rows_sorted(
                            &group.worktrees,
                            source,
                            mode,
                            order,
                            &sort
                        ),
                        "{mode}: {}",
                        group.label
                    );
                }
            }
        }
    }

    #[test]
    #[ignore = "synthetic sidebar projection performance benchmark"]
    fn benchmark_sidebar_catalog_index() {
        use crate::project_tree::{
            project_title_matches_search, worktree_matches_search, SidebarSearch,
        };
        use std::time::{Duration, Instant};
        let (catalog, groups) = projection_fixture(64, 8);
        let sort = SidebarSortContext::new(Some(&catalog), None, &[], true, 1000);
        let manual: Vec<_> = catalog
            .workspaces
            .iter()
            .rev()
            .map(|ws| ws.id.clone())
            .collect();
        let queries = [
            "",
            "alpha",
            "中文",
            "section 2",
            "tag 1",
            "feature/5",
            "missing",
        ];
        // A projection batch includes index construction (when enabled),
        // filtering, and ordering every matching group's rows.
        let run = |query: &str, mode: &str, indexed: bool| {
            let index = indexed.then(|| SidebarCatalogIndex::new(Some(&catalog)));
            let search = index.as_ref().map(|idx| SidebarSearch::new(query, idx));
            let manual_order = (mode == "manual").then_some(manual.as_slice());
            groups
                .iter()
                .filter_map(|group| {
                    let show_all = search.as_ref().map_or_else(
                        || project_title_matches_search(group, Some(&catalog), query),
                        |search| search.project_matches(group),
                    );
                    let entries: Vec<_> = group
                        .worktrees
                        .iter()
                        .filter(|entry| {
                            show_all
                                || search.as_ref().map_or_else(
                                    || worktree_matches_search(entry, Some(&catalog), query),
                                    |search| search.worktree_matches(entry),
                                )
                        })
                        .cloned()
                        .collect();
                    if entries.is_empty() && !show_all {
                        return None;
                    }
                    let rows = if let Some(index) = index.as_ref() {
                        order_worktree_rows_indexed(&entries, index, mode, manual_order, &sort)
                    } else {
                        legacy_order_worktree_rows_sorted(
                            &entries,
                            Some(&catalog),
                            mode,
                            manual_order,
                            &sort,
                        )
                    };
                    Some((group.root.clone(), rows))
                })
                .collect::<Vec<_>>()
        };
        let workloads: Vec<_> = queries
            .iter()
            .flat_map(|query| ["name", "manual", "recent", "activity"].map(|mode| (*query, mode)))
            .collect();
        let expected: Vec<_> = workloads
            .iter()
            .map(|(query, mode)| run(query, mode, false))
            .collect();
        for (i, (query, mode)) in workloads.iter().enumerate() {
            assert_eq!(run(query, mode, true), expected[i]);
        }
        let mut old = Vec::new();
        let mut new = Vec::new();
        for sample in 0..7 {
            for optimized in if sample % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            } {
                let start = Instant::now();
                let actual: Vec<_> = workloads
                    .iter()
                    .map(|(query, mode)| run(query, mode, optimized))
                    .collect();
                let elapsed = start.elapsed();
                assert_eq!(actual, expected);
                if optimized {
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
        println!("GPUI sidebar projection projects={} workspaces={} workloads={} old_ms={old:.3} new_ms={new:.3} speedup={:.2}x", groups.len(), catalog.workspaces.len(), workloads.len(), old/new);
    }

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
    fn recent_and_agent_activity_sort_preserve_main_and_visible_descendants() {
        use crate::terminal::{AgentActivityState, TerminalSession};
        let mut catalog = RuntimeCatalog::default();
        catalog.workspaces.extend([
            workspace("M", "main", None),
            workspace("A", "a", None),
            workspace("B", "b", None),
            workspace("C", "child", Some("b")),
        ]);
        catalog.workspace_updated_ms =
            HashMap::from([("a".into(), 100), ("b".into(), 200), ("main".into(), 50)]);
        let entries = [
            git_entry("M", true),
            git_entry("A", false),
            git_entry("B", false),
            git_entry("C", false),
        ];
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-24T12:00:00Z")
            .unwrap()
            .timestamp_millis();
        let recency = SidebarSortContext::new(Some(&catalog), None, &[], true, now);
        let rows = order_worktree_rows_sorted(&entries, Some(&catalog), "recent", None, &recency);
        assert_eq!(
            rows.iter()
                .map(|r| r.entry.path.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            ["M", "B", "C", "A"]
        );
        let session = |id: &str, state| TerminalSession {
            handle: id.into(),
            workspace_id: id.into(),
            tab_id: id.into(),
            running: true,
            agent_type: Some("agy".into()),
            agent_state: Some(state),
            interrupted: false,
            agent_state_started_at: Some("2026-11-24T11:59:00Z".into()),
            agent_updated_at: Some("2026-11-24T11:59:00Z".into()),
            agent_context: None,
            builtin_mcp_status: None,
        };
        let sessions = [
            session("main", AgentActivityState::Working),
            session("a", AgentActivityState::Done),
            session("child", AgentActivityState::Waiting),
        ];
        let activity = SidebarSortContext::new(Some(&catalog), None, &sessions, true, now);
        let rows =
            order_worktree_rows_sorted(&entries, Some(&catalog), "activity", None, &activity);
        assert_eq!(
            rows.iter()
                .map(|r| r.entry.path.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            ["B", "C", "A", "M"]
        );
        assert_eq!(rows[1].depth, 1);
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
