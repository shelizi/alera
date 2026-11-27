use std::collections::{HashMap, HashSet};

use super::{
    WorkspaceExplorerDirectoryChildren, WorkspaceExplorerEntryBinding, WorkspaceExplorerTreeNode,
    WorkspaceExplorerTreeNodeKind, WorkspaceExplorerTreeProjection, WorkspaceFileEntry,
    WorkspaceFileKind,
};

const ROOT_ID: &str = "workspace-root";
const PLACEHOLDER_PREFIX: &str = "__alera_placeholder__:";

pub(super) fn project_tree(
    workspace_name: String,
    workspace_path: String,
    directories: Vec<WorkspaceExplorerDirectoryChildren>,
    replacement: Option<WorkspaceExplorerDirectoryChildren>,
) -> WorkspaceExplorerTreeProjection {
    let mut directories_by_path = directories
        .into_iter()
        .map(|directory| {
            (
                normalize_relative_path(&directory.relative_path),
                directory.children,
            )
        })
        .collect::<HashMap<_, _>>();

    if let Some(replacement) = replacement {
        replace_directory_children(
            &mut directories_by_path,
            normalize_relative_path(&replacement.relative_path),
            replacement.children,
        );
    }
    prune_unreachable_directories(&mut directories_by_path);

    let (nodes, entry_bindings) =
        build_nodes(&workspace_name, &workspace_path, &directories_by_path);
    let directories = sorted_directories(directories_by_path);

    WorkspaceExplorerTreeProjection {
        directories,
        nodes,
        entry_bindings,
    }
}

fn replace_directory_children(
    directories_by_path: &mut HashMap<String, Vec<WorkspaceFileEntry>>,
    relative_path: String,
    children: Vec<WorkspaceFileEntry>,
) {
    if let Some(previous) = directories_by_path.get(&relative_path) {
        let next_paths = children
            .iter()
            .map(|entry| entry.relative_path.as_str())
            .collect::<HashSet<_>>();
        let next_directories = children
            .iter()
            .filter(|entry| matches!(entry.kind, WorkspaceFileKind::Directory))
            .map(|entry| entry.relative_path.as_str())
            .collect::<HashSet<_>>();
        let removed_paths = previous
            .iter()
            .filter(|entry| {
                !next_paths.contains(entry.relative_path.as_str())
                    || (matches!(entry.kind, WorkspaceFileKind::Directory)
                        && !next_directories.contains(entry.relative_path.as_str()))
            })
            .map(|entry| entry.relative_path.clone())
            .collect::<HashSet<_>>();
        remove_cached_subtrees(directories_by_path, &removed_paths);
    }
    // Insert after pruning: a directory replaced by a same-path file stays
    // in its parent's new listing, but its old loaded descendants do not.
    directories_by_path.insert(relative_path, children);
}

fn remove_cached_subtrees(
    directories_by_path: &mut HashMap<String, Vec<WorkspaceFileEntry>>,
    removed: &HashSet<String>,
) {
    if removed.is_empty() {
        return;
    }
    // Preserve the exact former slash boundary semantics, including for
    // noncanonical inputs. Test every ancestor against the removed roots
    // instead of scanning all directory/entry rows once per removed root.
    let single = (removed.len() == 1).then(|| {
        let path = removed.iter().next().expect("one removed path");
        (path.as_str(), format!("{path}/"))
    });
    let is_removed = |path: &str| {
        if let Some((root, prefix)) = &single {
            return path == *root || path.starts_with(prefix);
        }
        removed.contains(path)
            || path
                .match_indices('/')
                .any(|(index, _)| removed.contains(&path[..index]))
    };
    directories_by_path.retain(|path, _| !is_removed(path));
    for children in directories_by_path.values_mut() {
        children.retain(|child| !is_removed(&child.relative_path));
    }
}

/// Ordinary native directory snapshots have one owner per child path.
/// Keep the older path-membership policy for rootless or noncanonical
/// payloads accepted by the stateless API (including duplicate bindings).
fn is_direct_cache(directories: &HashMap<String, Vec<WorkspaceFileEntry>>) -> bool {
    directories.contains_key("")
        && directories.iter().all(|(path, children)| {
            if !path.is_empty()
                && path
                    .split('/')
                    .any(|part| part.is_empty() || part == "." || part == "..")
            {
                return false;
            }
            let mut seen = HashSet::with_capacity(children.len());
            children.iter().all(|entry| {
                let (parent, name) = entry
                    .relative_path
                    .rsplit_once('/')
                    .unwrap_or(("", entry.relative_path.as_str()));
                parent == path
                    && !name.is_empty()
                    && name != "."
                    && name != ".."
                    && !entry.relative_path.starts_with('/')
                    && seen.insert(entry.relative_path.as_str())
            })
        })
}

fn prune_unreachable_directories(directories: &mut HashMap<String, Vec<WorkspaceFileEntry>>) {
    // Collect only stale directory keys (not owned copies of all entries).
    // Normal caches must be transitively reachable from the loaded root
    // through actual directory entries. A stale late update, a removed
    // ancestor, or a directory becoming a file must not resurrect a subtree.
    let stale = if is_direct_cache(directories) {
        let mut reachable = HashSet::from([""]);
        let mut pending = vec![""];
        while let Some(path) = pending.pop() {
            if let Some(children) = directories.get(path) {
                for entry in children {
                    if matches!(entry.kind, WorkspaceFileKind::Directory) {
                        let child = entry.relative_path.as_str();
                        if directories.contains_key(child) && reachable.insert(child) {
                            pending.push(child);
                        }
                    }
                }
            }
        }
        directories
            .keys()
            .filter(|path| !reachable.contains(path.as_str()))
            .cloned()
            .collect::<Vec<_>>()
    } else {
        let known_paths = directories
            .values()
            .flat_map(|children| children.iter().map(|entry| entry.relative_path.as_str()))
            .collect::<HashSet<_>>();
        directories
            .keys()
            .filter(|path| !path.is_empty() && !known_paths.contains(path.as_str()))
            .cloned()
            .collect::<Vec<_>>()
    };
    for path in stale {
        directories.remove(&path);
    }
}

fn build_nodes(
    workspace_name: &str,
    workspace_path: &str,
    directories_by_path: &HashMap<String, Vec<WorkspaceFileEntry>>,
) -> (
    Vec<WorkspaceExplorerTreeNode>,
    Vec<WorkspaceExplorerEntryBinding>,
) {
    let mut nodes = HashMap::<String, WorkspaceExplorerTreeNode>::new();
    let mut entry_bindings = Vec::<WorkspaceExplorerEntryBinding>::new();
    nodes.insert(
        ROOT_ID.to_string(),
        WorkspaceExplorerTreeNode {
            id: ROOT_ID.to_string(),
            name: workspace_name.to_string(),
            kind: WorkspaceExplorerTreeNodeKind::Root,
            parent_id: String::new(),
            virtual_path: "/".to_string(),
            source_path: workspace_path.to_string(),
            entry_id: None,
            child_ids: Vec::new(),
            is_expanded: true,
            is_virtual: false,
        },
    );

    let mut directory_paths = directories_by_path.keys().cloned().collect::<Vec<_>>();
    directory_paths.sort_by(|left, right| {
        left.matches('/')
            .count()
            .cmp(&right.matches('/').count())
            .then_with(|| left.cmp(right))
    });
    for directory_path in directory_paths {
        if let Some(entries) = directories_by_path.get(&directory_path) {
            for entry in entries {
                add_entry_node(&mut nodes, &mut entry_bindings, entry);
                if matches!(entry.kind, WorkspaceFileKind::Directory)
                    && entry.has_children_hint
                    && !directories_by_path.contains_key(&entry.relative_path)
                {
                    add_placeholder(&mut nodes, &entry.relative_path);
                }
            }
        }
    }

    let mut nodes = nodes.into_values().collect::<Vec<_>>();
    nodes.sort_by(|left, right| {
        if left.id == ROOT_ID {
            return std::cmp::Ordering::Less;
        }
        if right.id == ROOT_ID {
            return std::cmp::Ordering::Greater;
        }
        left.virtual_path.cmp(&right.virtual_path)
    });
    entry_bindings.sort_by(|left, right| left.node_id.cmp(&right.node_id));
    (nodes, entry_bindings)
}

fn add_entry_node(
    nodes: &mut HashMap<String, WorkspaceExplorerTreeNode>,
    entry_bindings: &mut Vec<WorkspaceExplorerEntryBinding>,
    entry: &WorkspaceFileEntry,
) {
    let parts = entry
        .relative_path
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let mut parent_id = ROOT_ID.to_string();
    let mut current_path = String::new();
    for (index, part) in parts.iter().enumerate() {
        current_path = if current_path.is_empty() {
            (*part).to_string()
        } else {
            format!("{current_path}/{part}")
        };
        let is_leaf = index == parts.len() - 1;
        let node_id = node_id_for_path(&current_path);
        if !nodes.contains_key(&node_id) {
            nodes.insert(
                node_id.clone(),
                WorkspaceExplorerTreeNode {
                    id: node_id.clone(),
                    name: (*part).to_string(),
                    kind: if is_leaf && !matches!(entry.kind, WorkspaceFileKind::Directory) {
                        WorkspaceExplorerTreeNodeKind::File
                    } else {
                        WorkspaceExplorerTreeNodeKind::Folder
                    },
                    parent_id: parent_id.clone(),
                    virtual_path: format!("/{current_path}"),
                    source_path: current_path.clone(),
                    entry_id: is_leaf.then_some(entry.relative_path.clone()),
                    child_ids: Vec::new(),
                    is_expanded: false,
                    is_virtual: false,
                },
            );
            append_child(nodes, &parent_id, &node_id);
        }
        if is_leaf {
            entry_bindings.push(WorkspaceExplorerEntryBinding {
                node_id: node_id.clone(),
                relative_path: entry.relative_path.clone(),
            });
        }
        parent_id = node_id;
    }
}

fn add_placeholder(nodes: &mut HashMap<String, WorkspaceExplorerTreeNode>, parent_path: &str) {
    let parent_id = node_id_for_path(parent_path);
    let node_id = format!("{PLACEHOLDER_PREFIX}{parent_path}");
    if nodes.contains_key(&node_id) || !nodes.contains_key(&parent_id) {
        return;
    }
    nodes.insert(
        node_id.clone(),
        WorkspaceExplorerTreeNode {
            id: node_id.clone(),
            name: String::new(),
            kind: WorkspaceExplorerTreeNodeKind::File,
            parent_id: parent_id.clone(),
            virtual_path: format!("/{parent_path}/.alera-placeholder"),
            source_path: String::new(),
            entry_id: None,
            child_ids: Vec::new(),
            is_expanded: false,
            is_virtual: true,
        },
    );
    append_child(nodes, &parent_id, &node_id);
}

fn append_child(
    nodes: &mut HashMap<String, WorkspaceExplorerTreeNode>,
    parent_id: &str,
    child_id: &str,
) {
    let Some(parent) = nodes.get_mut(parent_id) else {
        return;
    };
    if !parent.child_ids.iter().any(|id| id == child_id) {
        parent.child_ids.push(child_id.to_string());
    }
}

fn sorted_directories(
    directories_by_path: HashMap<String, Vec<WorkspaceFileEntry>>,
) -> Vec<WorkspaceExplorerDirectoryChildren> {
    let mut directories = directories_by_path
        .into_iter()
        .map(
            |(relative_path, children)| WorkspaceExplorerDirectoryChildren {
                relative_path,
                children,
            },
        )
        .collect::<Vec<_>>();
    directories.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    directories
}

fn node_id_for_path(relative_path: &str) -> String {
    format!("path:{relative_path}")
}

fn normalize_relative_path(relative_path: &str) -> String {
    relative_path.trim_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn entry(path: &str, kind: WorkspaceFileKind) -> WorkspaceFileEntry {
        WorkspaceFileEntry {
            relative_path: path.into(),
            name: path.rsplit('/').next().unwrap_or_default().into(),
            kind,
            size: 0,
            modified_millis: 1,
            content_token: "1".into(),
            is_ignored: false,
            is_hidden: false,
            is_symlink: false,
            is_protected: false,
            has_children_hint: kind == WorkspaceFileKind::Directory,
            git_status: None,
        }
    }

    fn dir(path: &str) -> WorkspaceFileEntry {
        entry(path, WorkspaceFileKind::Directory)
    }

    fn file(path: &str) -> WorkspaceFileEntry {
        entry(path, WorkspaceFileKind::File)
    }

    fn children(
        path: &str,
        entries: Vec<WorkspaceFileEntry>,
    ) -> WorkspaceExplorerDirectoryChildren {
        WorkspaceExplorerDirectoryChildren {
            relative_path: path.into(),
            children: entries,
        }
    }

    type DirectoryCache = HashMap<String, Vec<WorkspaceFileEntry>>;

    fn project(
        directories: Vec<WorkspaceExplorerDirectoryChildren>,
        replacement: Option<WorkspaceExplorerDirectoryChildren>,
    ) -> WorkspaceExplorerTreeProjection {
        project_tree("alera".into(), "/repo".into(), directories, replacement)
    }

    // Independent pre-Phase-32 oracle for ordinary deletion and unusual
    // stateless payloads. The same-path kind transition is deliberately
    // corrected rather than compared to that previous bug.
    fn legacy_replace_directory_children(
        directories_by_path: &mut HashMap<String, Vec<WorkspaceFileEntry>>,
        relative_path: String,
        children: Vec<WorkspaceFileEntry>,
    ) {
        if let Some(previous) = directories_by_path.get(&relative_path) {
            let next_paths = children
                .iter()
                .map(|entry| entry.relative_path.as_str())
                .collect::<HashSet<_>>();
            let removed_paths = previous
                .iter()
                .filter_map(|entry| {
                    (!next_paths.contains(entry.relative_path.as_str()))
                        .then_some(entry.relative_path.clone())
                })
                .collect::<Vec<_>>();
            for removed_path in removed_paths {
                legacy_remove_cached_subtree(directories_by_path, &removed_path);
            }
        }
        directories_by_path.insert(relative_path, children);
    }

    fn legacy_remove_cached_subtree(
        directories_by_path: &mut HashMap<String, Vec<WorkspaceFileEntry>>,
        relative_path: &str,
    ) {
        directories_by_path.remove(relative_path);
        let prefix = format!("{relative_path}/");
        directories_by_path.retain(|path, _| path != relative_path && !path.starts_with(&prefix));
        for children in directories_by_path.values_mut() {
            children.retain(|child| {
                child.relative_path != relative_path && !child.relative_path.starts_with(&prefix)
            });
        }
    }

    fn legacy_prune_unreachable_directories(
        directories_by_path: &mut HashMap<String, Vec<WorkspaceFileEntry>>,
    ) {
        let known_paths = directories_by_path
            .values()
            .flat_map(|children| children.iter().map(|entry| entry.relative_path.clone()))
            .collect::<HashSet<_>>();
        directories_by_path.retain(|path, _| path.is_empty() || known_paths.contains(path));
    }

    #[test]
    fn replacing_directory_with_file_prunes_descendants_without_touching_siblings() {
        let seed = project(
            vec![
                children("", vec![dir("src"), dir("src-old")]),
                children("src", vec![file("src/file"), dir("src/nested")]),
                children("src/nested", vec![file("src/nested/old")]),
                children("src-old", vec![file("src-old/keep")]),
            ],
            None,
        );
        let replacement = children("", vec![file("src"), dir("src-old")]);
        let next = project(seed.directories.clone(), Some(replacement.clone()));
        assert_eq!(
            next.directories
                .iter()
                .map(|row| row.relative_path.as_str())
                .collect::<Vec<_>>(),
            ["", "src-old"]
        );
        assert!(next
            .nodes
            .iter()
            .all(|node| !node.id.starts_with("path:src/")));
        assert!(next
            .entry_bindings
            .iter()
            .all(|row| !row.relative_path.starts_with("src/")));
        let converted = next
            .nodes
            .iter()
            .find(|node| node.id == "path:src")
            .unwrap();
        assert_eq!(converted.kind, WorkspaceExplorerTreeNodeKind::File);
        assert!(converted.child_ids.is_empty());
        assert!(next.nodes.iter().any(|node| node.id == "path:src-old/keep"));

        // The stateless API and session reseed must also repair a stale
        // cache supplied by a client, with no explicit replacement event.
        let mut stale = seed.directories;
        stale[0] = replacement;
        let reseeded = project(stale, None);
        assert_eq!(reseeded.directories, next.directories);
        assert_eq!(reseeded.nodes, next.nodes);
        assert_eq!(reseeded.entry_bindings, next.entry_bindings);

        // A late response for a deleted directory cannot resurrect it
        // through its nested children, and recreating the folder starts lazy.
        let late = project(
            next.directories,
            Some(children("src", vec![dir("src/nested"), file("src/stale")])),
        );
        assert_eq!(late.directories, reseeded.directories);
        assert_eq!(late.nodes, reseeded.nodes);
        assert_eq!(late.entry_bindings, reseeded.entry_bindings);
        let recreated = project(
            late.directories,
            Some(children("", vec![dir("src"), dir("src-old")])),
        );
        assert!(recreated
            .nodes
            .iter()
            .any(|node| node.id == "__alera_placeholder__:src"));
        assert!(recreated
            .nodes
            .iter()
            .all(|node| !node.id.starts_with("path:src/")));
    }

    #[test]
    fn root_reachability_prunes_orphan_chains_even_if_child_paths_are_referenced() {
        let actual = project(
            vec![
                children("", vec![dir("live")]),
                children("live", vec![dir("live/loaded"), dir("live/lazy")]),
                children("live/loaded", vec![]),
                children("orphan", vec![dir("orphan/nested")]),
                children("orphan/nested", vec![dir("orphan/nested/deeper")]),
                children(
                    "orphan/nested/deeper",
                    vec![file("orphan/nested/deeper/stale")],
                ),
            ],
            None,
        );
        assert_eq!(
            actual
                .directories
                .iter()
                .map(|dir| dir.relative_path.as_str())
                .collect::<Vec<_>>(),
            ["", "live", "live/loaded"]
        );
        assert!(actual
            .nodes
            .iter()
            .all(|node| !node.virtual_path.starts_with("/orphan")));
        assert!(actual
            .nodes
            .iter()
            .any(|node| node.id == "__alera_placeholder__:live/lazy"));
        let late = project(
            actual.directories.clone(),
            Some(children("live/missing", vec![file("live/missing/stale")])),
        );
        assert_eq!(late.nodes, actual.nodes);
        assert_eq!(late.entry_bindings, actual.entry_bindings);
        assert_eq!(late.directories, actual.directories);
    }

    #[test]
    fn batched_subtree_removal_preserves_exact_legacy_slash_boundaries() {
        // Deliberately include noncanonical inputs to verify exact old
        // matching, not a filesystem/path-normalization approximation.
        let paths = [
            "",
            "a",
            "a/b",
            "a/b/c",
            "a/b-extra",
            "a-extra",
            "a//b",
            "a/",
            "/a",
            "b",
            "b/c",
            "中文",
            "中文/子",
            "中文檔",
            "space dir",
            "space dir/item",
            "a\\b",
        ];
        let all: DirectoryCache = paths
            .iter()
            .map(|path| {
                (
                    (*path).to_owned(),
                    paths.iter().map(|child| file(child)).collect(),
                )
            })
            .collect();
        for removed in [
            vec![],
            vec!["a"],
            vec!["a/b", "b", "中文"],
            vec!["a", "a/b", "a//b", "space dir"],
            vec!["", "a/"],
            vec!["a\\b", "/a"],
        ] {
            let mut expected = all.clone();
            for root in &removed {
                legacy_remove_cached_subtree(&mut expected, root);
            }
            let mut actual = all.clone();
            remove_cached_subtrees(
                &mut actual,
                &removed.into_iter().map(str::to_owned).collect(),
            );
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn noncanonical_and_rootless_caches_keep_legacy_stateless_membership() {
        for mut cache in [
            DirectoryCache::from([
                ("parent".into(), vec![dir("parent/sub")]),
                ("parent/sub".into(), vec![file("parent/sub/a")]),
            ]),
            DirectoryCache::from([
                ("".into(), vec![dir("a"), dir("a/nested")]),
                ("a/nested".into(), vec![file("a/nested/file")]),
            ]),
            DirectoryCache::from([
                ("".into(), vec![dir("a"), dir("a")]),
                ("a".into(), vec![file("a/file")]),
            ]),
        ] {
            assert!(!is_direct_cache(&cache));
            let mut expected = cache.clone();
            legacy_prune_unreachable_directories(&mut expected);
            prune_unreachable_directories(&mut cache);
            assert_eq!(cache, expected);
        }
    }

    fn wide_cache(folder_count: usize, file_count: usize) -> DirectoryCache {
        let mut root = Vec::new();
        let mut result = DirectoryCache::new();
        for index in 0..folder_count {
            let path = format!("folder-{index:03}");
            root.push(dir(&path));
            result.insert(
                path.clone(),
                (0..file_count)
                    .map(|number| file(&format!("{path}/file-{number:03}.rs")))
                    .collect(),
            );
        }
        result.insert(String::new(), root);
        result
    }

    #[test]
    #[ignore = "synthetic native batched subtree pruning benchmark"]
    fn benchmark_batch_explorer_subtree_pruning() {
        let seed = wide_cache(200, 200);
        fn median(samples: &mut [Duration]) -> f64 {
            samples.sort();
            samples[samples.len() / 2].as_secs_f64() * 1000.0
        }
        for removed_count in [0, 1, 80] {
            let replacement: Vec<_> = seed[""].iter().skip(removed_count).cloned().collect();
            let mut expected = seed.clone();
            legacy_replace_directory_children(&mut expected, String::new(), replacement.clone());
            legacy_prune_unreachable_directories(&mut expected);
            let mut warm = seed.clone();
            replace_directory_children(&mut warm, String::new(), replacement.clone());
            prune_unreachable_directories(&mut warm);
            assert_eq!(warm, expected);
            let mut old = Vec::new();
            let mut new = Vec::new();
            for sample in 0..7 {
                for optimized in if sample % 2 == 0 {
                    [false, true]
                } else {
                    [true, false]
                } {
                    let mut candidate = seed.clone();
                    let rows = replacement.clone();
                    let start = Instant::now();
                    if optimized {
                        replace_directory_children(&mut candidate, String::new(), rows);
                        prune_unreachable_directories(&mut candidate);
                    } else {
                        legacy_replace_directory_children(&mut candidate, String::new(), rows);
                        legacy_prune_unreachable_directories(&mut candidate);
                    }
                    let elapsed = start.elapsed();
                    assert_eq!(candidate, expected);
                    if optimized {
                        new.push(elapsed);
                    } else {
                        old.push(elapsed);
                    }
                }
            }
            let old = median(&mut old);
            let new = median(&mut new);
            println!("Classic batch pruning folders=200 files=40000 removed_folders={removed_count} remaining_dirs={} old_ms={old:.3} new_ms={new:.3} speedup={:.2}x", expected.len(), old/new);
        }
    }
}
