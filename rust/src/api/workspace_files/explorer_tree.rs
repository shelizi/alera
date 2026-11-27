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
    let direct_cache = prune_unreachable_directories(&mut directories_by_path);
    finish_projection(
        &workspace_name,
        &workspace_path,
        directories_by_path,
        direct_cache,
    )
}

pub(super) struct DirectoryChanges {
    pub changed: Vec<WorkspaceExplorerDirectoryChildren>,
    pub removed: Vec<String>,
}

/// The session owns the normalized, uniquely keyed directory rows already.
/// Move their entry buffers into the next projection and record only rows
/// changed by replacement or pruning instead of cloning the entire cache.
pub(super) fn update_owned_tree(
    workspace_name: &str,
    workspace_path: &str,
    directories: Vec<WorkspaceExplorerDirectoryChildren>,
    replacement: WorkspaceExplorerDirectoryChildren,
) -> (WorkspaceExplorerTreeProjection, DirectoryChanges) {
    let old_paths = directories
        .iter()
        .map(|row| row.relative_path.clone())
        .collect::<Vec<_>>();
    let mut cache = directories
        .into_iter()
        .map(|row| (row.relative_path, row.children))
        .collect::<HashMap<_, _>>();
    let path = normalize_relative_path(&replacement.relative_path);
    let replacement_changed = cache
        .get(&path)
        .is_none_or(|old| old != &replacement.children);
    let mut changed_paths = HashSet::new();
    replace_directory_children_tracked(&mut cache, path.clone(), replacement.children, |changed| {
        if changed != path {
            changed_paths.insert(changed.to_owned());
        }
    });
    if replacement_changed {
        changed_paths.insert(path);
    }
    let direct_cache = prune_unreachable_directories(&mut cache);
    let removed = old_paths
        .into_iter()
        .filter(|path| !cache.contains_key(path))
        .collect();
    let projection = finish_projection(workspace_name, workspace_path, cache, direct_cache);
    let changed = projection
        .directories
        .iter()
        .filter(|row| changed_paths.contains(&row.relative_path))
        .cloned()
        .collect();
    (projection, DirectoryChanges { changed, removed })
}

fn finish_projection(
    workspace_name: &str,
    workspace_path: &str,
    directories_by_path: HashMap<String, Vec<WorkspaceFileEntry>>,
    direct_cache: bool,
) -> WorkspaceExplorerTreeProjection {
    let (nodes, entry_bindings) = if direct_cache {
        build_direct_nodes(workspace_name, workspace_path, &directories_by_path)
    } else {
        build_nodes(workspace_name, workspace_path, &directories_by_path)
    };
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
    replace_directory_children_tracked(directories_by_path, relative_path, children, |_| {});
}

fn replace_directory_children_tracked(
    directories_by_path: &mut HashMap<String, Vec<WorkspaceFileEntry>>,
    relative_path: String,
    children: Vec<WorkspaceFileEntry>,
    on_changed: impl FnMut(&str),
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
        remove_cached_subtrees_tracked(directories_by_path, &removed_paths, on_changed);
    }
    // Insert after pruning: a directory replaced by a same-path file stays
    // in its parent's new listing, but its old loaded descendants do not.
    directories_by_path.insert(relative_path, children);
}

#[cfg(test)]
fn remove_cached_subtrees(
    directories_by_path: &mut HashMap<String, Vec<WorkspaceFileEntry>>,
    removed: &HashSet<String>,
) {
    remove_cached_subtrees_tracked(directories_by_path, removed, |_| {});
}

fn remove_cached_subtrees_tracked(
    directories_by_path: &mut HashMap<String, Vec<WorkspaceFileEntry>>,
    removed: &HashSet<String>,
    mut on_changed: impl FnMut(&str),
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
    for (path, children) in directories_by_path.iter_mut() {
        let old_len = children.len();
        children.retain(|child| !is_removed(&child.relative_path));
        if old_len != children.len() {
            on_changed(path);
        }
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

/// Return whether the pruned cache is a validated rooted direct-child tree.
fn prune_unreachable_directories(
    directories: &mut HashMap<String, Vec<WorkspaceFileEntry>>,
) -> bool {
    // Collect only stale directory keys (not owned copies of all entries).
    // Normal caches must be transitively reachable from the loaded root
    // through actual directory entries. A stale late update, a removed
    // ancestor, or a directory becoming a file must not resurrect a subtree.
    let direct_cache = is_direct_cache(directories);
    let stale = if direct_cache {
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
    direct_cache
}

/// A validated, pruned direct cache has one parent per entry, no duplicate
/// paths and no unreachable loaded directories. Build directly in vectors:
/// file leaves need neither an owned hash key nor a parent-node hash lookup.
fn build_direct_nodes(
    workspace_name: &str,
    workspace_path: &str,
    directories: &HashMap<String, Vec<WorkspaceFileEntry>>,
) -> (
    Vec<WorkspaceExplorerTreeNode>,
    Vec<WorkspaceExplorerEntryBinding>,
) {
    let entry_count = directories.values().map(Vec::len).sum::<usize>();
    let mut nodes = Vec::with_capacity(entry_count + 1);
    let mut bindings = Vec::with_capacity(entry_count);
    nodes.push(root_node(workspace_name, workspace_path));
    let mut pending = vec![(0, directories[""].as_slice())];
    while let Some((parent_index, entries)) = pending.pop() {
        let parent_id = nodes[parent_index].id.clone();
        let mut child_ids = Vec::with_capacity(entries.len());
        for entry in entries {
            let path = &entry.relative_path;
            let name = path.rsplit('/').next().expect("validated direct path");
            let node_id = node_id_for_path(path);
            let mut node = direct_entry_node(&parent_id, name, entry, &node_id);
            let mut placeholder = None;
            if entry.kind == WorkspaceFileKind::Directory {
                if let Some(children) = directories.get(path) {
                    pending.push((nodes.len(), children.as_slice()));
                } else if entry.has_children_hint {
                    let lazy = placeholder_node(path, &node_id);
                    node.child_ids.push(lazy.id.clone());
                    placeholder = Some(lazy);
                }
            }
            nodes.push(node);
            if let Some(lazy) = placeholder {
                nodes.push(lazy);
            }
            child_ids.push(node_id.clone());
            bindings.push(WorkspaceExplorerEntryBinding {
                node_id,
                relative_path: path.clone(),
            });
        }
        nodes[parent_index].child_ids = child_ids;
    }
    // Root remains first. All other virtual paths and binding IDs are unique.
    nodes[1..].sort_unstable_by(|left, right| left.virtual_path.cmp(&right.virtual_path));
    bindings.sort_unstable_by(|left, right| left.node_id.cmp(&right.node_id));
    (nodes, bindings)
}

fn root_node(workspace_name: &str, workspace_path: &str) -> WorkspaceExplorerTreeNode {
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
        root_node(workspace_name, workspace_path),
    );

    // Borrow directory keys and compute depth once rather than cloning
    // every path and rescanning it in the sort comparator.
    let mut directory_paths = directories_by_path
        .keys()
        .map(|path| (path.matches('/').count(), path))
        .collect::<Vec<_>>();
    directory_paths.sort_unstable();
    for (_, directory_path) in directory_paths {
        if let Some(entries) = directories_by_path.get(directory_path) {
            // A canonical cached directory whose parent node already exists
            // needs no repeated ancestor traversal for each direct child.
            let parent_id = if directory_path.is_empty() {
                ROOT_ID.to_owned()
            } else {
                node_id_for_path(directory_path)
            };
            let direct_prefix = if directory_path.is_empty() {
                Some(String::new())
            } else if !directory_path.split('/').any(str::is_empty)
                && nodes.contains_key(&parent_id)
            {
                Some(format!("{directory_path}/"))
            } else {
                None
            };
            for entry in entries {
                let direct_name = direct_prefix.as_deref().and_then(|prefix| {
                    let name = entry.relative_path.strip_prefix(prefix)?;
                    (!name.is_empty() && !name.contains('/')).then_some(name)
                });
                if let Some(name) = direct_name {
                    add_direct_entry_node(&mut nodes, &mut entry_bindings, &parent_id, name, entry);
                } else {
                    add_entry_node(&mut nodes, &mut entry_bindings, entry);
                }
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

/// The caller already verified the existing parent and exact direct path.
/// Preserve first-inserted node fields and all duplicate bindings, just as
/// the general path walker does.
fn add_direct_entry_node(
    nodes: &mut HashMap<String, WorkspaceExplorerTreeNode>,
    entry_bindings: &mut Vec<WorkspaceExplorerEntryBinding>,
    parent_id: &str,
    name: &str,
    entry: &WorkspaceFileEntry,
) {
    let path = &entry.relative_path;
    let node_id = node_id_for_path(path);
    if !nodes.contains_key(&node_id) {
        nodes.insert(
            node_id.clone(),
            direct_entry_node(parent_id, name, entry, &node_id),
        );
        append_new_child(nodes, parent_id, &node_id);
    }
    entry_bindings.push(WorkspaceExplorerEntryBinding {
        node_id,
        relative_path: path.clone(),
    });
}

fn direct_entry_node(
    parent_id: &str,
    name: &str,
    entry: &WorkspaceFileEntry,
    node_id: &str,
) -> WorkspaceExplorerTreeNode {
    let path = &entry.relative_path;
    WorkspaceExplorerTreeNode {
        id: node_id.to_owned(),
        name: name.to_owned(),
        kind: if entry.kind == WorkspaceFileKind::Directory {
            WorkspaceExplorerTreeNodeKind::Folder
        } else {
            WorkspaceExplorerTreeNodeKind::File
        },
        parent_id: parent_id.to_owned(),
        virtual_path: format!("/{path}"),
        source_path: path.clone(),
        entry_id: Some(path.clone()),
        child_ids: Vec::new(),
        is_expanded: false,
        is_virtual: false,
    }
}

fn add_entry_node(
    nodes: &mut HashMap<String, WorkspaceExplorerTreeNode>,
    entry_bindings: &mut Vec<WorkspaceExplorerEntryBinding>,
    entry: &WorkspaceFileEntry,
) {
    let mut parts = entry
        .relative_path
        .split('/')
        .filter(|part| !part.is_empty())
        .peekable();
    let mut parent_id = ROOT_ID.to_string();
    let mut current_path = String::new();
    while let Some(part) = parts.next() {
        if !current_path.is_empty() {
            current_path.push('/');
        }
        current_path.push_str(part);
        let is_leaf = parts.peek().is_none();
        let node_id = node_id_for_path(&current_path);
        if !nodes.contains_key(&node_id) {
            nodes.insert(
                node_id.clone(),
                WorkspaceExplorerTreeNode {
                    id: node_id.clone(),
                    name: part.to_string(),
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
            append_new_child(nodes, &parent_id, &node_id);
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
    nodes.insert(node_id.clone(), placeholder_node(parent_path, &parent_id));
    append_new_child(nodes, &parent_id, &node_id);
}

fn placeholder_node(parent_path: &str, parent_id: &str) -> WorkspaceExplorerTreeNode {
    WorkspaceExplorerTreeNode {
        id: format!("{PLACEHOLDER_PREFIX}{parent_path}"),
        name: String::new(),
        kind: WorkspaceExplorerTreeNodeKind::File,
        parent_id: parent_id.to_owned(),
        virtual_path: format!("/{parent_path}/.alera-placeholder"),
        source_path: String::new(),
        entry_id: None,
        child_ids: Vec::new(),
        is_expanded: false,
        is_virtual: true,
    }
}

/// Both callers invoke this only immediately after inserting a previously
/// absent globally unique node ID. No child list could already contain that
/// ID: all links are created here, and build_nodes never removes nodes.
/// Keep insertion order without a quadratic sibling scan or another set.
fn append_new_child(
    nodes: &mut HashMap<String, WorkspaceExplorerTreeNode>,
    parent_id: &str,
    child_id: &str,
) {
    let Some(parent) = nodes.get_mut(parent_id) else {
        return;
    };
    parent.child_ids.push(child_id.to_string());
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

    // The previous builder is an independent oracle for node contents,
    // child insertion order, duplicate bindings and lazy placeholders.
    fn legacy_build_nodes(
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
                    legacy_add_entry_node(&mut nodes, &mut entry_bindings, entry);
                    if matches!(entry.kind, WorkspaceFileKind::Directory)
                        && entry.has_children_hint
                        && !directories_by_path.contains_key(&entry.relative_path)
                    {
                        legacy_add_placeholder(&mut nodes, &entry.relative_path);
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

    fn legacy_add_entry_node(
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
                legacy_append_child(nodes, &parent_id, &node_id);
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

    fn legacy_add_placeholder(
        nodes: &mut HashMap<String, WorkspaceExplorerTreeNode>,
        parent_path: &str,
    ) {
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
        legacy_append_child(nodes, &parent_id, &node_id);
    }

    fn legacy_append_child(
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

    #[test]
    fn linear_child_links_preserve_legacy_projection_for_duplicate_and_implicit_paths() {
        let fixtures = [
            wide_cache(4, 40),
            DirectoryCache::from([
                (
                    "".into(),
                    vec![
                        file("z.txt"),
                        file("a/deep/first"),
                        dir("a"),
                        dir("a/lazy"),
                        file("a/lazy/.alera-placeholder"),
                        file("a//deep/first"),
                        file("a/deep/second"),
                        dir("a/lazy"),
                        file(""),
                        file("/"),
                        file("a/深/文件"),
                        file("a/deep/first"),
                        dir("a/space dir"),
                        file("a/space dir/name.txt"),
                    ],
                ),
                (
                    "a/deep".into(),
                    vec![file("a/deep/second"), file("a/deep/third")],
                ),
                ("a/深".into(), vec![file("a/深/新")]),
            ]),
            DirectoryCache::from([
                ("".into(), vec![dir("lazy"), dir("loaded"), dir("empty")]),
                (
                    "loaded".into(),
                    vec![file("loaded/z"), file("loaded/a"), dir("loaded/sub")],
                ),
                ("empty".into(), vec![]),
            ]),
            DirectoryCache::new(),
        ];
        for fixture in fixtures {
            let (mut actual_nodes, actual_bindings) = build_nodes("alera", "/repo", &fixture);
            let (mut expected_nodes, expected_bindings) =
                legacy_build_nodes("alera", "/repo", &fixture);
            // The original output sort is by virtual path; an actual file
            // named .alera-placeholder may tie a synthetic placeholder.
            // Compare that ordered path sequence and every node's fields,
            // including child order. Relative order of equal sort keys was
            // never guaranteed because HashMap iteration is randomized.
            assert_eq!(
                actual_nodes
                    .iter()
                    .map(|node| node.virtual_path.as_str())
                    .collect::<Vec<_>>(),
                expected_nodes
                    .iter()
                    .map(|node| node.virtual_path.as_str())
                    .collect::<Vec<_>>()
            );
            actual_nodes.sort_by(|a, b| a.id.cmp(&b.id));
            expected_nodes.sort_by(|a, b| a.id.cmp(&b.id));
            assert_eq!(actual_nodes, expected_nodes);
            assert_eq!(actual_bindings, expected_bindings);

            let by_id: HashMap<_, _> = actual_nodes
                .iter()
                .map(|node| (node.id.as_str(), node))
                .collect();
            assert_eq!(by_id.len(), actual_nodes.len());
            for node in &actual_nodes {
                let unique: HashSet<_> = node.child_ids.iter().collect();
                assert_eq!(unique.len(), node.child_ids.len());
                for child in &node.child_ids {
                    assert_eq!(by_id[child.as_str()].parent_id, node.id);
                }
            }
        }
    }

    #[test]
    #[ignore = "synthetic native full Explorer tree construction benchmark"]
    fn benchmark_linear_explorer_child_links() {
        let wide = DirectoryCache::from([(
            String::new(),
            (0..12000)
                .rev()
                .map(|index| file(&format!("file-{index:05}.txt")))
                .collect(),
        )]);
        let nested = wide_cache(200, 200);
        fn median(samples: &mut [Duration]) -> f64 {
            samples.sort();
            samples[samples.len() / 2].as_secs_f64() * 1000.0
        }
        for (label, fixture) in [("wide", wide), ("nested", nested)] {
            let expected = legacy_build_nodes("alera", "/repo", &fixture);
            assert_eq!(build_nodes("alera", "/repo", &fixture), expected);
            let mut old = Vec::new();
            let mut new = Vec::new();
            for sample in 0..7 {
                for optimized in if sample % 2 == 0 {
                    [false, true]
                } else {
                    [true, false]
                } {
                    let start = Instant::now();
                    let actual = if optimized {
                        build_nodes("alera", "/repo", &fixture)
                    } else {
                        legacy_build_nodes("alera", "/repo", &fixture)
                    };
                    let elapsed = start.elapsed();
                    assert_eq!(actual, expected);
                    if optimized {
                        new.push(elapsed);
                    } else {
                        old.push(elapsed);
                    }
                }
            }
            let old = median(&mut old);
            let new = median(&mut new);
            println!("Classic full node build shape={label} cached_dirs={} nodes={} bindings={} old_ms={old:.3} new_ms={new:.3} speedup={:.2}x", fixture.len(), expected.0.len(), expected.1.len(), old/new);
        }
    }

    #[test]
    fn direct_vector_projection_matches_previous_builder_and_prunes_stale_directories() {
        let fixtures = [
            DirectoryCache::from([(String::new(), vec![])]),
            wide_cache(3, 19),
            DirectoryCache::from([
                (
                    "".into(),
                    vec![
                        dir("z"),
                        dir("empty"),
                        dir("lazy"),
                        file("中文.txt"),
                        dir("a"),
                    ],
                ),
                (
                    "a".into(),
                    vec![file("a/.alera-placeholder"), dir("a/deep"), file("a/z")],
                ),
                (
                    "a/deep".into(),
                    vec![file("a/deep/first"), dir("a/deep/child")],
                ),
                ("a/deep/child".into(), vec![file("a/deep/child/x")]),
                ("z".into(), vec![file("z/second"), file("z/first")]),
                ("empty".into(), vec![]),
                ("orphan".into(), vec![dir("orphan/child")]),
                ("orphan/child".into(), vec![file("orphan/child/stale")]),
                ("中文.txt".into(), vec![file("中文.txt/stale")]),
            ]),
        ];
        for mut cache in fixtures {
            let raw = cache
                .iter()
                .map(|(path, entries)| children(path, entries.clone()))
                .collect();
            assert!(prune_unreachable_directories(&mut cache));
            let expected = build_nodes("alera", "/repo", &cache);
            assert_eq!(build_direct_nodes("alera", "/repo", &cache), expected);
            let actual = project(raw, None);
            assert_eq!((actual.nodes, actual.entry_bindings), expected);
            assert!(actual
                .directories
                .iter()
                .all(|dir| cache.contains_key(&dir.relative_path)));
        }
    }

    #[test]
    fn direct_vector_projection_handles_loaded_empty_and_lazy_directory_hints() {
        let mut loaded = dir("loaded");
        loaded.has_children_hint = false;
        let mut empty = dir("empty");
        empty.has_children_hint = false;
        let cache = DirectoryCache::from([
            ("".into(), vec![dir("lazy"), loaded, empty]),
            ("loaded".into(), vec![file("loaded/file")]),
        ]);
        assert!(is_direct_cache(&cache));
        let (nodes, bindings) = build_direct_nodes("alera", "/repo", &cache);
        assert_eq!(
            (nodes.clone(), bindings),
            build_nodes("alera", "/repo", &cache)
        );
        assert!(nodes
            .iter()
            .any(|node| node.id == "__alera_placeholder__:lazy"));
        assert!(nodes
            .iter()
            .all(|node| node.id != "__alera_placeholder__:empty"));
        assert!(nodes.iter().any(|node| node.id == "path:loaded/file"));
    }

    #[test]
    #[ignore = "synthetic native direct-vector Explorer projection benchmark"]
    fn benchmark_direct_vector_explorer_projection() {
        let wide = DirectoryCache::from([(
            String::new(),
            (0..12000)
                .rev()
                .map(|index| file(&format!("file-{index:05}.txt")))
                .collect(),
        )]);
        let nested = wide_cache(200, 200);
        let lazy = DirectoryCache::from([(
            String::new(),
            (0..12000)
                .rev()
                .map(|index| dir(&format!("dir-{index:05}")))
                .collect(),
        )]);
        fn median(samples: &mut [Duration]) -> f64 {
            samples.sort();
            samples[samples.len() / 2].as_secs_f64() * 1000.0
        }
        for (label, mut cache) in [("wide", wide), ("nested", nested), ("lazy", lazy)] {
            assert!(prune_unreachable_directories(&mut cache));
            let expected = build_nodes("alera", "/repo", &cache);
            assert_eq!(build_direct_nodes("alera", "/repo", &cache), expected);
            let mut old = Vec::new();
            let mut new = Vec::new();
            for sample in 0..7 {
                for optimized in if sample % 2 == 0 {
                    [false, true]
                } else {
                    [true, false]
                } {
                    let start = Instant::now();
                    let actual = if optimized {
                        build_direct_nodes("alera", "/repo", &cache)
                    } else {
                        build_nodes("alera", "/repo", &cache)
                    };
                    let elapsed = start.elapsed();
                    assert_eq!(actual, expected);
                    if optimized {
                        new.push(elapsed);
                    } else {
                        old.push(elapsed);
                    }
                }
            }
            let old = median(&mut old);
            let new = median(&mut new);
            println!("Classic direct vector projection shape={label} nodes={} bindings={} old_ms={old:.3} new_ms={new:.3} speedup={:.2}x", expected.0.len(), expected.1.len(), old/new);
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
