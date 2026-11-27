use std::collections::HashMap;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use super::{
    explorer_tree, WorkspaceExplorerDirectoryChildren, WorkspaceExplorerEntryBinding,
    WorkspaceExplorerProjectionDelta, WorkspaceExplorerProjectionHandle, WorkspaceExplorerTreeNode,
    WorkspaceExplorerTreeNodeKind, WorkspaceExplorerTreeProjection, WorkspaceFileError,
    WorkspaceFileErrorKind, WorkspaceFileKind,
};

const MAX_SESSIONS: usize = 64;
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static SESSIONS: OnceLock<Mutex<HashMap<u64, ExplorerProjectionSession>>> = OnceLock::new();

struct ExplorerProjectionSession {
    workspace_name: String,
    workspace_path: String,
    version: u64,
    projection: WorkspaceExplorerTreeProjection,
    // Proven by the full projector, preserved only by validated local edits.
    direct_cache: bool,
}

fn sessions() -> &'static Mutex<HashMap<u64, ExplorerProjectionSession>> {
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn session_error(kind: WorkspaceFileErrorKind, context: &str) -> WorkspaceFileError {
    WorkspaceFileError::new(kind, context.to_owned())
}

/// Seed a native snapshot once from the Dart cache. Later updates send just
/// one changed directory and receive only the changed tree nodes/bindings.
pub(super) fn start(
    workspace_name: String,
    workspace_path: String,
    directories: Vec<WorkspaceExplorerDirectoryChildren>,
) -> Result<WorkspaceExplorerProjectionHandle, WorkspaceFileError> {
    let (projection, direct_cache) = explorer_tree::project_tree_with_direct_cache(
        workspace_name.clone(),
        workspace_path.clone(),
        directories,
        None,
    );
    let mut registry = sessions()
        .lock()
        .map_err(|_| session_error(WorkspaceFileErrorKind::Io, "Explorer session lock poisoned"))?;
    if registry.len() >= MAX_SESSIONS {
        return Err(session_error(
            WorkspaceFileErrorKind::Unsupported,
            "Too many Explorer projection sessions",
        ));
    }
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    registry.insert(
        id,
        ExplorerProjectionSession {
            workspace_name,
            workspace_path,
            version: 0,
            projection,
            direct_cache,
        },
    );
    Ok(WorkspaceExplorerProjectionHandle { id, version: 0 })
}

/// Reject an out-of-order update rather than applying a directory replacement
/// to a stale native tree. The caller can re-create a session from its Dart
/// cache or fall back to the original full-snapshot projection API.
pub(super) fn update(
    handle: WorkspaceExplorerProjectionHandle,
    replacement: WorkspaceExplorerDirectoryChildren,
) -> Result<WorkspaceExplorerProjectionDelta, WorkspaceFileError> {
    let mut registry = sessions()
        .lock()
        .map_err(|_| session_error(WorkspaceFileErrorKind::Io, "Explorer session lock poisoned"))?;
    let current = registry.get_mut(&handle.id).ok_or_else(|| {
        session_error(
            WorkspaceFileErrorKind::NotFound,
            "Explorer projection session expired",
        )
    })?;
    if current.version != handle.version {
        return Err(session_error(
            WorkspaceFileErrorKind::Conflict,
            "Explorer projection version mismatch",
        ));
    }
    let next_version = current.version + 1;
    // Both the directories and their children are ordered. When the paths,
    // file kinds and directory-placeholder hints have not changed, the Rust
    // node/binding graph would be *identical*. Avoid cloning all directories,
    // rebuilding every node, and comparing three whole-tree collections for
    // common Git-status, mtime, file-size and content-token updates.
    //
    // Root bootstrap and structural changes still take the complete path.
    let normalized_path = replacement.relative_path.trim_matches('/');
    if let Ok(index) = current
        .projection
        .directories
        .binary_search_by(|directory| directory.relative_path.as_str().cmp(normalized_path))
    {
        let previous = &current.projection.directories[index];
        if same_explorer_node_layout(previous, &replacement) {
            let changed = previous.children != replacement.children;
            let changed_directories = if changed {
                current.projection.directories[index].children = replacement.children;
                vec![current.projection.directories[index].clone()]
            } else {
                Vec::new()
            };
            current.version = next_version;
            return Ok(WorkspaceExplorerProjectionDelta {
                version: next_version,
                changed_directories,
                removed_directory_paths: Vec::new(),
                changed_nodes: Vec::new(),
                removed_node_ids: Vec::new(),
                changed_bindings: Vec::new(),
                removed_binding_node_ids: Vec::new(),
            });
        }
    }
    // File-only structural updates cannot prune cached child directories or
    // create lazy placeholder nodes. Edit only this folder's entries, its
    // child-node list, and the added/removed file bindings. Keep the full
    // projector as the authoritative fallback for mixed/complex changes.
    if let Ok(index) = current
        .projection
        .directories
        .binary_search_by(|directory| directory.relative_path.as_str().cmp(normalized_path))
    {
        if let Some(delta) = try_update_file_only_children(
            current,
            index,
            normalized_path,
            &replacement,
            next_version,
            true,
        ) {
            return Ok(delta);
        }
        if let Some(delta) =
            try_update_mixed_children(current, index, normalized_path, &replacement, next_version)
        {
            return Ok(delta);
        }
        if let Some(delta) =
            try_prune_loaded_children(current, index, normalized_path, &replacement, next_version)
        {
            return Ok(delta);
        }
    } else if let Some(delta) =
        try_expand_directory(current, normalized_path, &replacement, next_version)
    {
        return Ok(delta);
    }

    Ok(rebuild_projection(current, replacement, next_version))
}

/// The session already owns the old directory buffers. Move them into
/// the next tree, tracking the directory delta during replacement/pruning.
/// Only the old node/binding graph is needed for the remaining diff.
fn rebuild_projection(
    current: &mut ExplorerProjectionSession,
    replacement: WorkspaceExplorerDirectoryChildren,
    next_version: u64,
) -> WorkspaceExplorerProjectionDelta {
    let directories = std::mem::take(&mut current.projection.directories);
    let was_empty = directories.is_empty();
    let (updated, directory_changes) = explorer_tree::update_owned_tree(
        &current.workspace_name,
        &current.workspace_path,
        directories,
        replacement,
    );
    let bootstrap_root = was_empty && !updated.directories.is_empty();
    let mut delta =
        try_diff_sorted_graph(&current.projection, &updated, next_version, bootstrap_root)
            .unwrap_or_else(|| {
                diff_graph_legacy(&current.projection, &updated, next_version, bootstrap_root)
            });
    delta.changed_directories = directory_changes.changed;
    delta.removed_directory_paths = directory_changes.removed;
    current.version = next_version;
    current.direct_cache = directory_changes.direct_cache;
    current.projection = updated;
    delta
}

/// These are precisely the input fields affecting Explorer's node graph.
/// All other WorkspaceFileEntry metadata belongs to directory cache and
/// visible row state but not to node IDs, order, kinds or placeholder layout.
fn same_explorer_node_layout(
    previous: &WorkspaceExplorerDirectoryChildren,
    replacement: &WorkspaceExplorerDirectoryChildren,
) -> bool {
    previous.children.len() == replacement.children.len()
        && previous
            .children
            .iter()
            .zip(&replacement.children)
            .all(|(old, new)| {
                old.relative_path == new.relative_path
                    && old.name == new.name
                    && old.kind == new.kind
                    && old.has_children_hint == new.has_children_hint
            })
}

/// Conservative structural fast path for a folder whose direct children are
/// all ordinary files. The full native projector must still handle directory
/// removal/pruning, lazy placeholders, implicit parents and special file kinds.
fn direct_unique_files(relative_directory: &str, entries: &[super::WorkspaceFileEntry]) -> bool {
    let prefix = if relative_directory.is_empty() {
        String::new()
    } else {
        format!("{relative_directory}/")
    };
    let mut seen = std::collections::HashSet::with_capacity(entries.len());
    entries.iter().all(|entry| {
        if !matches!(entry.kind, WorkspaceFileKind::File) {
            return false;
        }
        let Some(name) = entry.relative_path.strip_prefix(&prefix) else {
            return false;
        };
        !name.is_empty()
            && name != "."
            && name != ".."
            && !name.contains('/')
            && seen.insert(entry.relative_path.as_str())
    })
}

// Small edits can use ordered vector insertion/removal instead of scanning
// and sorting all loaded nodes. Bound memmove work; bulk edits retain the
// previous linear retain and sort strategy.
const MAX_SPARSE_FILE_EDITS: usize = 8;

fn unique_sorted_index<T>(items: &[T], target: &str, key: impl Fn(&T) -> &str) -> Option<usize> {
    let index = items.binary_search_by(|item| key(item).cmp(target)).ok()?;
    if index
        .checked_sub(1)
        .is_some_and(|prior| key(&items[prior]) == target)
        || items.get(index + 1).is_some_and(|next| key(next) == target)
    {
        return None;
    }
    Some(index)
}

/// Preflight an edit before mutating either vector. Ambiguous keys (e.g.
/// synthetic placeholder paths) retain the legacy strategy. The caller's
/// sorted collections are kept sorted across every successful edit.
fn sparse_removal_plan<T>(
    items: &[T],
    removed: &[&str],
    added: &[T],
    key: impl Fn(&T) -> &str,
) -> Option<Vec<usize>> {
    let mut indices = Vec::with_capacity(removed.len());
    for target in removed {
        indices.push(unique_sorted_index(items, target, &key)?);
    }
    for item in added {
        if items
            .binary_search_by(|old| key(old).cmp(key(item)))
            .is_ok()
        {
            return None;
        }
    }
    indices.sort_unstable_by(|a, b| b.cmp(a));
    if indices.windows(2).any(|pair| pair[0] == pair[1]) {
        return None;
    }
    Some(indices)
}

fn apply_sparse_plan<T>(
    items: &mut Vec<T>,
    removed_indices: Vec<usize>,
    added: impl IntoIterator<Item = T>,
    key: impl Fn(&T) -> &str,
) {
    for index in removed_indices {
        items.remove(index);
    }
    for item in added {
        // Already checked that all new keys are absent from the original
        // collection; caller constructs uniquely keyed additions.
        let index = items
            .binary_search_by(|old| key(old).cmp(key(&item)))
            .expect_err("preflighted new key");
        items.insert(index, item);
    }
}

fn try_update_file_only_children(
    current: &mut ExplorerProjectionSession,
    directory_index: usize,
    relative_directory: &str,
    replacement: &WorkspaceExplorerDirectoryChildren,
    next_version: u64,
    use_sparse_edits: bool,
) -> Option<WorkspaceExplorerProjectionDelta> {
    let previous = &current.projection.directories[directory_index];
    if !direct_unique_files(relative_directory, &previous.children)
        || !direct_unique_files(relative_directory, &replacement.children)
    {
        return None;
    }
    let parent_id = if relative_directory.is_empty() {
        "workspace-root".to_owned()
    } else {
        format!("path:{relative_directory}")
    };
    let parent_virtual_path = format!("/{relative_directory}");
    let parent_index = if use_sparse_edits {
        unique_sorted_index(&current.projection.nodes, &parent_virtual_path, |node| {
            &node.virtual_path
        })
        .filter(|&index| current.projection.nodes[index].id == parent_id)
    } else {
        None
    }
    .or_else(|| {
        current
            .projection
            .nodes
            .iter()
            .position(|node| node.id == parent_id)
    })?;
    let expected_old_ids: Vec<String> = previous
        .children
        .iter()
        .map(|entry| format!("path:{}", entry.relative_path))
        .collect();
    // The parent might be an implicit/intermediate node with children from
    // elsewhere. In that case the full projector is the only safe option.
    if current.projection.nodes[parent_index].child_ids != expected_old_ids {
        return None;
    }

    let old_paths = previous
        .children
        .iter()
        .map(|entry| entry.relative_path.as_str())
        .collect::<std::collections::HashSet<_>>();
    let new_paths = replacement
        .children
        .iter()
        .map(|entry| entry.relative_path.as_str())
        .collect::<std::collections::HashSet<_>>();
    let mut removed_node_ids = previous
        .children
        .iter()
        .filter(|entry| !new_paths.contains(entry.relative_path.as_str()))
        .map(|entry| format!("path:{}", entry.relative_path))
        .collect::<Vec<_>>();
    // Full projector orders removed nodes by virtual path; direct siblings
    // have the same ordering as relative paths.
    removed_node_ids.sort();

    let mut new_nodes = Vec::new();
    let mut new_bindings = Vec::new();
    for entry in &replacement.children {
        if old_paths.contains(entry.relative_path.as_str()) {
            continue;
        }
        let node_id = format!("path:{}", entry.relative_path);
        new_nodes.push(WorkspaceExplorerTreeNode {
            id: node_id.clone(),
            name: entry
                .relative_path
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_owned(),
            kind: WorkspaceExplorerTreeNodeKind::File,
            parent_id: parent_id.clone(),
            virtual_path: format!("/{}", entry.relative_path),
            source_path: entry.relative_path.clone(),
            entry_id: Some(entry.relative_path.clone()),
            child_ids: Vec::new(),
            is_expanded: false,
            is_virtual: false,
        });
        new_bindings.push(WorkspaceExplorerEntryBinding {
            node_id,
            relative_path: entry.relative_path.clone(),
        });
    }

    let next_child_ids = replacement
        .children
        .iter()
        .map(|entry| format!("path:{}", entry.relative_path))
        .collect::<Vec<_>>();
    let mut changed_nodes = Vec::with_capacity(new_nodes.len() + 1);
    let parent = &mut current.projection.nodes[parent_index];
    if parent.child_ids != next_child_ids {
        parent.child_ids = next_child_ids;
        changed_nodes.push(parent.clone());
    }
    changed_nodes.extend(new_nodes.iter().cloned());
    changed_nodes.sort_by(|left, right| {
        if left.id == "workspace-root" {
            std::cmp::Ordering::Less
        } else if right.id == "workspace-root" {
            std::cmp::Ordering::Greater
        } else {
            left.virtual_path.cmp(&right.virtual_path)
        }
    });
    new_bindings.sort_by(|left, right| left.node_id.cmp(&right.node_id));

    let sparse_plan =
        if use_sparse_edits && removed_node_ids.len() + new_nodes.len() <= MAX_SPARSE_FILE_EDITS {
            // Virtual path order is the native node order; binding order is
            // by node ID. Verify both plans before changing either vector.
            let removed_paths = removed_node_ids
                .iter()
                .map(|id| format!("/{}", id.strip_prefix("path:").expect("file node ID")))
                .collect::<Vec<_>>();
            let removed_paths = removed_paths.iter().map(String::as_str).collect::<Vec<_>>();
            let removed_ids = removed_node_ids
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            sparse_removal_plan(
                &current.projection.nodes,
                &removed_paths,
                &new_nodes,
                |node| &node.virtual_path,
            )
            .filter(|indices| {
                indices.iter().all(|&index| {
                    removed_ids.contains(&current.projection.nodes[index].id.as_str())
                })
            })
            .and_then(|node_indices| {
                sparse_removal_plan(
                    &current.projection.entry_bindings,
                    &removed_ids,
                    &new_bindings,
                    |binding| &binding.node_id,
                )
                .map(|binding_indices| (node_indices, binding_indices))
            })
        } else {
            None
        };

    if let Some((node_indices, binding_indices)) = sparse_plan {
        apply_sparse_plan(
            &mut current.projection.nodes,
            node_indices,
            new_nodes,
            |node| &node.virtual_path,
        );
        apply_sparse_plan(
            &mut current.projection.entry_bindings,
            binding_indices,
            new_bindings.iter().cloned(),
            |binding| &binding.node_id,
        );
    } else {
        if !removed_node_ids.is_empty() {
            let removed = removed_node_ids
                .iter()
                .map(String::as_str)
                .collect::<std::collections::HashSet<_>>();
            current
                .projection
                .nodes
                .retain(|node| !removed.contains(node.id.as_str()));
            current
                .projection
                .entry_bindings
                .retain(|binding| !removed.contains(binding.node_id.as_str()));
        }
        if !new_nodes.is_empty() {
            current.projection.nodes.extend(new_nodes);
            current.projection.nodes.sort_by(|left, right| {
                if left.id == "workspace-root" {
                    std::cmp::Ordering::Less
                } else if right.id == "workspace-root" {
                    std::cmp::Ordering::Greater
                } else {
                    left.virtual_path.cmp(&right.virtual_path)
                }
            });
            current
                .projection
                .entry_bindings
                .extend(new_bindings.iter().cloned());
            current
                .projection
                .entry_bindings
                .sort_by(|left, right| left.node_id.cmp(&right.node_id));
        }
    }

    current.projection.directories[directory_index].children = replacement.children.clone();
    current.version = next_version;
    Some(WorkspaceExplorerProjectionDelta {
        version: next_version,
        changed_directories: vec![current.projection.directories[directory_index].clone()],
        removed_directory_paths: Vec::new(),
        changed_nodes,
        removed_node_ids: removed_node_ids.clone(),
        changed_bindings: new_bindings,
        removed_binding_node_ids: removed_node_ids,
    })
}

/// Bound vector memmoves for sparse mixed edits. Bulk mutations and loaded
/// directory removal use the full owned-cache projector.
const MAX_MIXED_NODE_EDITS: usize = 16;

fn append_local_leaf_nodes(
    nodes: &mut Vec<WorkspaceExplorerTreeNode>,
    parent_id: &str,
    entry: &super::WorkspaceFileEntry,
) {
    let path = &entry.relative_path;
    let node_id = explorer_tree::node_id_for_path(path);
    let name = path.rsplit('/').next().expect("direct child");
    let mut node = explorer_tree::direct_entry_node(parent_id, name, entry, &node_id);
    if entry.kind == WorkspaceFileKind::Directory && entry.has_children_hint {
        let placeholder = explorer_tree::placeholder_node(path, &node_id);
        node.child_ids.push(placeholder.id.clone());
        nodes.push(placeholder);
    }
    nodes.push(node);
}

fn leaf_shape(entry: &super::WorkspaceFileEntry) -> (bool, bool) {
    let folder = entry.kind == WorkspaceFileKind::Directory;
    (folder, folder && entry.has_children_hint)
}

/// A rooted, unique direct-child cache has no cross-owner references or
/// orphan directory rows. If all loaded child folders survive as folders,
/// only this parent's links, changed leaves and their lazy placeholders can
/// change. Plan every edit before touching session state.
fn try_update_mixed_children(
    current: &mut ExplorerProjectionSession,
    directory_index: usize,
    relative_directory: &str,
    replacement: &WorkspaceExplorerDirectoryChildren,
    next_version: u64,
) -> Option<WorkspaceExplorerProjectionDelta> {
    if !current.direct_cache
        || !explorer_tree::direct_children_are_valid(relative_directory, &replacement.children)
    {
        return None;
    }
    let projection = &current.projection;
    let previous = &projection.directories[directory_index];
    let parent_id = if relative_directory.is_empty() {
        "workspace-root".to_owned()
    } else {
        explorer_tree::node_id_for_path(relative_directory)
    };
    let parent_index = unique_sorted_index(
        &projection.nodes,
        &format!("/{relative_directory}"),
        |node| &node.virtual_path,
    )?;
    let parent = &projection.nodes[parent_index];
    if parent.id != parent_id
        || parent.child_ids.len() != previous.children.len()
        || !parent
            .child_ids
            .iter()
            .zip(&previous.children)
            .all(|(id, child)| id.strip_prefix("path:") == Some(child.relative_path.as_str()))
    {
        return None;
    }

    let old_by_path: HashMap<_, _> = previous
        .children
        .iter()
        .map(|entry| (entry.relative_path.as_str(), entry))
        .collect();
    let new_by_path: HashMap<_, _> = replacement
        .children
        .iter()
        .map(|entry| (entry.relative_path.as_str(), entry))
        .collect();
    let mut old_local = Vec::new();
    let mut new_local = Vec::new();
    let mut removed_bindings = Vec::new();
    let mut added_bindings = Vec::new();
    for old in &previous.children {
        let path = old.relative_path.as_str();
        let next = new_by_path.get(path).copied();
        if old.kind == WorkspaceFileKind::Directory
            && projection
                .directories
                .binary_search_by(|row| row.relative_path.as_str().cmp(path))
                .is_ok()
        {
            // Loaded folders keep their existing descendant graph, even if
            // has_children_hint changes. Removal or a kind conversion prunes
            // the subtree and is deliberately left to the full projector.
            if !next.is_some_and(|entry| entry.kind == WorkspaceFileKind::Directory) {
                return None;
            }
            continue;
        }
        if next.is_some_and(|entry| leaf_shape(entry) == leaf_shape(old)) {
            continue;
        }
        append_local_leaf_nodes(&mut old_local, &parent_id, old);
        if let Some(entry) = next {
            append_local_leaf_nodes(&mut new_local, &parent_id, entry);
        } else {
            removed_bindings.push(explorer_tree::node_id_for_path(path));
        }
        if old_local.len() + new_local.len() > MAX_MIXED_NODE_EDITS {
            return None;
        }
    }
    for new in &replacement.children {
        if old_by_path.contains_key(new.relative_path.as_str()) {
            continue;
        }
        append_local_leaf_nodes(&mut new_local, &parent_id, new);
        added_bindings.push(WorkspaceExplorerEntryBinding {
            node_id: explorer_tree::node_id_for_path(&new.relative_path),
            relative_path: new.relative_path.clone(),
        });
        if old_local.len() + new_local.len() > MAX_MIXED_NODE_EDITS {
            return None;
        }
    }

    old_local.sort_unstable_by(|a, b| a.virtual_path.cmp(&b.virtual_path));
    new_local.sort_unstable_by(|a, b| a.virtual_path.cmp(&b.virtual_path));
    removed_bindings.sort_unstable();
    added_bindings.sort_unstable_by(|a, b| a.node_id.cmp(&b.node_id));
    // Check the planned old local nodes against the authoritative graph.
    // Also protects against any future change in projection invariants.
    for old in &old_local {
        let index = unique_sorted_index(&projection.nodes, &old.virtual_path, |node| {
            &node.virtual_path
        })?;
        if projection.nodes[index] != *old {
            return None;
        }
    }
    let (mut changed_nodes, removed_node_ids) = diff_ordered(
        &old_local,
        &new_local,
        |node| &node.virtual_path,
        |node| &node.id,
    );
    let removed_paths = old_local
        .iter()
        .filter(|node| removed_node_ids.contains(&node.id))
        .map(|node| node.virtual_path.as_str())
        .collect::<Vec<_>>();
    let mut added_nodes = Vec::new();
    let mut modified_nodes = Vec::new();
    for node in &changed_nodes {
        if old_local
            .binary_search_by(|old| old.virtual_path.cmp(&node.virtual_path))
            .is_ok()
        {
            let index = unique_sorted_index(&projection.nodes, &node.virtual_path, |old| {
                &old.virtual_path
            })?;
            if projection.nodes[index].id != node.id {
                return None;
            }
            modified_nodes.push((index, node.clone()));
        } else {
            added_nodes.push(node.clone());
        }
    }
    let node_indices =
        sparse_removal_plan(&projection.nodes, &removed_paths, &added_nodes, |node| {
            &node.virtual_path
        })?;
    let removed_ids = removed_bindings
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let binding_indices = sparse_removal_plan(
        &projection.entry_bindings,
        &removed_ids,
        &added_bindings,
        |binding| &binding.node_id,
    )?;
    let next_child_ids = replacement
        .children
        .iter()
        .map(|entry| explorer_tree::node_id_for_path(&entry.relative_path))
        .collect::<Vec<_>>();

    // No fallible preflight remains: all mutations below affect only this
    // parent and the bounded, previously validated local edit plan.
    if parent.child_ids != next_child_ids {
        let mut next_parent = parent.clone();
        next_parent.child_ids = next_child_ids;
        modified_nodes.push((parent_index, next_parent.clone()));
        changed_nodes.push(next_parent);
    }
    for (index, node) in modified_nodes {
        current.projection.nodes[index] = node;
    }
    apply_sparse_plan(
        &mut current.projection.nodes,
        node_indices,
        added_nodes,
        |node| &node.virtual_path,
    );
    apply_sparse_plan(
        &mut current.projection.entry_bindings,
        binding_indices,
        added_bindings.iter().cloned(),
        |binding| &binding.node_id,
    );
    changed_nodes.sort_unstable_by(|a, b| a.virtual_path.cmp(&b.virtual_path));
    current.projection.directories[directory_index].children = replacement.children.clone();
    current.version = next_version;
    Some(WorkspaceExplorerProjectionDelta {
        version: next_version,
        changed_directories: vec![current.projection.directories[directory_index].clone()],
        removed_directory_paths: Vec::new(),
        changed_nodes,
        removed_node_ids,
        changed_bindings: added_bindings,
        removed_binding_node_ids: removed_bindings,
    })
}

/// All descendants have a slash-delimited prefix. The exact root is a
/// separate key: siblings such as "src-old" can sort between "src" and
/// "src/child". Replace the final ASCII slash with its successor to bound
/// the prefix with two binary searches, including for Unicode paths.
fn descendant_range<T>(items: &[T], path: &str, key: impl Fn(&T) -> &str) -> Range<usize> {
    let lower = format!("{path}/");
    let upper = format!("{path}0");
    items.partition_point(|item| key(item) < lower.as_str())
        ..items.partition_point(|item| key(item) < upper.as_str())
}

fn add_range(ranges: &mut Vec<Range<usize>>, range: Range<usize>) {
    if !range.is_empty() {
        ranges.push(range);
    }
}

/// Sort and coalesce adjacent ranges. Overlapping plans indicate an
/// inconsistent graph and must be rejected before any mutation.
fn normalize_ranges(mut ranges: Vec<Range<usize>>) -> Option<Vec<Range<usize>>> {
    ranges.sort_unstable_by_key(|range| range.start);
    let mut result: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        if let Some(last) = result.last_mut() {
            if last.end > range.start {
                return None;
            }
            if last.end == range.start {
                last.end = range.end;
                continue;
            }
        }
        result.push(range);
    }
    Some(result)
}

fn removed_range_keys<T>(
    items: &[T],
    ranges: &[Range<usize>],
    key: impl Fn(&T) -> &str,
) -> Vec<String> {
    let mut removed = Vec::with_capacity(ranges.iter().map(|range| range.len()).sum());
    for range in ranges {
        removed.extend(items[range.clone()].iter().map(|item| key(item).to_owned()));
    }
    removed
}

/// A few ranges can be removed with bounded memmoves. For many disjoint
/// subtrees, compact the vector once instead of shifting it per directory.
fn remove_ranges<T>(items: &mut Vec<T>, ranges: &[Range<usize>]) {
    if ranges.len() <= 4 {
        for range in ranges.iter().rev() {
            items.drain(range.clone());
        }
    } else {
        let mut range_index = 0;
        let mut index = 0;
        items.retain(|_| {
            while range_index < ranges.len() && index >= ranges[range_index].end {
                range_index += 1;
            }
            let keep = range_index == ranges.len() || index < ranges[range_index].start;
            index += 1;
            keep
        });
    }
}

/// First expansion of a proven rooted directory can only replace that
/// folder's optional lazy placeholder and add direct children (and their
/// placeholders). Splice the sorted descendant ranges in one operation,
/// even when the real child is named ".alera-placeholder".
fn try_expand_directory(
    current: &mut ExplorerProjectionSession,
    relative_directory: &str,
    replacement: &WorkspaceExplorerDirectoryChildren,
    next_version: u64,
) -> Option<WorkspaceExplorerProjectionDelta> {
    if !current.direct_cache
        || relative_directory.is_empty()
        || !explorer_tree::direct_children_are_valid(relative_directory, &replacement.children)
    {
        return None;
    }
    let projection = &current.projection;
    let directory_index = projection
        .directories
        .binary_search_by(|row| row.relative_path.as_str().cmp(relative_directory))
        .err()?;
    let parent_id = explorer_tree::node_id_for_path(relative_directory);
    let parent_path = format!("/{relative_directory}");
    let parent_index =
        unique_sorted_index(&projection.nodes, &parent_path, |node| &node.virtual_path)?;
    let parent = &projection.nodes[parent_index];
    if parent.id != parent_id
        || parent.kind != WorkspaceExplorerTreeNodeKind::Folder
        || parent.is_virtual
    {
        return None;
    }
    let node_range = descendant_range(&projection.nodes, &parent_path, |node| &node.virtual_path);
    let binding_range = descendant_range(&projection.entry_bindings, &parent_id, |binding| {
        &binding.node_id
    });
    if !binding_range.is_empty()
        || !descendant_range(&projection.directories, relative_directory, |row| {
            &row.relative_path
        })
        .is_empty()
    {
        return None;
    }
    let old_nodes = &projection.nodes[node_range.clone()];
    if old_nodes.len() != parent.child_ids.len()
        || old_nodes.len() > 1
        || !old_nodes.iter().zip(&parent.child_ids).all(|(node, id)| {
            node.id == *id
                && *node == explorer_tree::placeholder_node(relative_directory, &parent_id)
        })
    {
        return None;
    }
    let removed_node_ids = old_nodes.iter().map(|node| node.id.clone()).collect();
    let mut new_nodes = Vec::with_capacity(replacement.children.len());
    let mut new_bindings = Vec::with_capacity(replacement.children.len());
    let mut child_ids = Vec::with_capacity(replacement.children.len());
    for entry in &replacement.children {
        append_local_leaf_nodes(&mut new_nodes, &parent_id, entry);
        let node_id = explorer_tree::node_id_for_path(&entry.relative_path);
        child_ids.push(node_id.clone());
        new_bindings.push(WorkspaceExplorerEntryBinding {
            node_id,
            relative_path: entry.relative_path.clone(),
        });
    }
    new_nodes.sort_unstable_by(|a, b| a.virtual_path.cmp(&b.virtual_path));
    new_bindings.sort_unstable_by(|a, b| a.node_id.cmp(&b.node_id));
    let mut changed_nodes = Vec::with_capacity(new_nodes.len() + 1);
    if parent.child_ids != child_ids {
        let mut next_parent = parent.clone();
        next_parent.child_ids = child_ids;
        changed_nodes.push(next_parent);
    }
    changed_nodes.extend(new_nodes.iter().cloned());
    let row = WorkspaceExplorerDirectoryChildren {
        relative_path: relative_directory.to_owned(),
        children: replacement.children.clone(),
    };

    if let Some(next_parent) = changed_nodes.first().filter(|node| node.id == parent_id) {
        current.projection.nodes[parent_index] = next_parent.clone();
    }
    current.projection.nodes.splice(node_range, new_nodes);
    current
        .projection
        .entry_bindings
        .splice(binding_range, new_bindings.iter().cloned());
    current
        .projection
        .directories
        .insert(directory_index, row.clone());
    current.version = next_version;
    Some(WorkspaceExplorerProjectionDelta {
        version: next_version,
        changed_directories: vec![row],
        removed_directory_paths: Vec::new(),
        changed_nodes,
        removed_node_ids,
        changed_bindings: new_bindings,
        removed_binding_node_ids: Vec::new(),
    })
}

/// Prune loaded subtrees for a removal-only listing update (including
/// loaded folder-to-file conversions). Surviving child nodes may reorder,
/// and retained loaded folders may change their lazy hint. Other simultaneous
/// graph changes use the full projector. The validated cache guarantees that
/// descendants are owned exclusively by the corresponding directory.
fn try_prune_loaded_children(
    current: &mut ExplorerProjectionSession,
    directory_index: usize,
    relative_directory: &str,
    replacement: &WorkspaceExplorerDirectoryChildren,
    next_version: u64,
) -> Option<WorkspaceExplorerProjectionDelta> {
    if !current.direct_cache
        || !explorer_tree::direct_children_are_valid(relative_directory, &replacement.children)
    {
        return None;
    }
    let projection = &current.projection;
    let previous = &projection.directories[directory_index];
    let parent_id = if relative_directory.is_empty() {
        "workspace-root".to_owned()
    } else {
        explorer_tree::node_id_for_path(relative_directory)
    };
    let parent_index = unique_sorted_index(
        &projection.nodes,
        &format!("/{relative_directory}"),
        |node| &node.virtual_path,
    )?;
    let parent = &projection.nodes[parent_index];
    if parent.id != parent_id
        || parent.child_ids.len() != previous.children.len()
        || !parent
            .child_ids
            .iter()
            .zip(&previous.children)
            .all(|(id, child)| id.strip_prefix("path:") == Some(child.relative_path.as_str()))
    {
        return None;
    }

    let new_by_path: HashMap<_, _> = replacement
        .children
        .iter()
        .map(|entry| (entry.relative_path.as_str(), entry))
        .collect();
    let mut matched = 0;
    let mut has_loaded_removal = false;
    let mut node_ranges = Vec::new();
    let mut binding_ranges = Vec::new();
    let mut directory_ranges = Vec::new();
    let mut modified_nodes = Vec::new();
    for old in &previous.children {
        let path = old.relative_path.as_str();
        let next = new_by_path.get(path).copied();
        if next.is_some() {
            matched += 1;
        }
        let loaded_index = if old.kind == WorkspaceFileKind::Directory {
            projection
                .directories
                .binary_search_by(|row| row.relative_path.as_str().cmp(path))
                .ok()
        } else {
            None
        };
        if let Some(next) = next {
            if loaded_index.is_some() && next.kind == WorkspaceFileKind::Directory {
                continue;
            }
            if loaded_index.is_none() {
                if leaf_shape(old) == leaf_shape(next) {
                    continue;
                }
                return None;
            }
            // A loaded directory converted to a leaf keeps its binding and
            // root node identity, but loses every cached descendant.
        }
        let node_id = explorer_tree::node_id_for_path(path);
        let node_path = format!("/{path}");
        let node_index =
            unique_sorted_index(&projection.nodes, &node_path, |node| &node.virtual_path)?;
        if projection.nodes[node_index].id != node_id {
            return None;
        }
        let binding_index = unique_sorted_index(&projection.entry_bindings, &node_id, |binding| {
            &binding.node_id
        })?;
        if projection.entry_bindings[binding_index].relative_path != path {
            return None;
        }
        if let Some(next) = next {
            let name = path.rsplit('/').next().expect("direct child");
            modified_nodes.push((
                node_index,
                explorer_tree::direct_entry_node(&parent_id, name, next, &node_id),
            ));
        } else {
            add_range(&mut node_ranges, node_index..node_index + 1);
            add_range(&mut binding_ranges, binding_index..binding_index + 1);
        }
        add_range(
            &mut node_ranges,
            descendant_range(&projection.nodes, &node_path, |node| &node.virtual_path),
        );
        add_range(
            &mut binding_ranges,
            descendant_range(&projection.entry_bindings, &node_id, |binding| {
                &binding.node_id
            }),
        );
        if let Some(index) = loaded_index {
            has_loaded_removal = true;
            add_range(&mut directory_ranges, index..index + 1);
            add_range(
                &mut directory_ranges,
                descendant_range(&projection.directories, path, |row| &row.relative_path),
            );
        }
    }
    if !has_loaded_removal || matched != replacement.children.len() {
        return None;
    }
    let node_ranges = normalize_ranges(node_ranges)?;
    let binding_ranges = normalize_ranges(binding_ranges)?;
    let directory_ranges = normalize_ranges(directory_ranges)?;
    let removed_node_ids = removed_range_keys(&projection.nodes, &node_ranges, |node| &node.id);
    let removed_binding_node_ids =
        removed_range_keys(&projection.entry_bindings, &binding_ranges, |binding| {
            &binding.node_id
        });
    let removed_directory_paths =
        removed_range_keys(&projection.directories, &directory_ranges, |row| {
            &row.relative_path
        });
    let next_child_ids = replacement
        .children
        .iter()
        .map(|entry| explorer_tree::node_id_for_path(&entry.relative_path))
        .collect::<Vec<_>>();
    if parent.child_ids != next_child_ids {
        let mut next_parent = parent.clone();
        next_parent.child_ids = next_child_ids;
        modified_nodes.push((parent_index, next_parent));
    }
    let mut changed_nodes = modified_nodes
        .iter()
        .map(|(_, node)| node.clone())
        .collect::<Vec<_>>();
    changed_nodes.sort_unstable_by(|a, b| a.virtual_path.cmp(&b.virtual_path));

    // All ranges and keys are preflighted. Update nodes and the parent row
    // before removing ranges so their original indices are still valid.
    for (index, node) in modified_nodes {
        current.projection.nodes[index] = node;
    }
    current.projection.directories[directory_index].children = replacement.children.clone();
    let changed_directories = vec![current.projection.directories[directory_index].clone()];
    remove_ranges(&mut current.projection.nodes, &node_ranges);
    remove_ranges(&mut current.projection.entry_bindings, &binding_ranges);
    remove_ranges(&mut current.projection.directories, &directory_ranges);
    current.version = next_version;
    Some(WorkspaceExplorerProjectionDelta {
        version: next_version,
        changed_directories,
        removed_directory_paths,
        changed_nodes,
        removed_node_ids,
        changed_bindings: Vec::new(),
        removed_binding_node_ids,
    })
}

pub(super) fn stop(handle: WorkspaceExplorerProjectionHandle) {
    if let Ok(mut registry) = sessions().lock() {
        registry.remove(&handle.id);
    }
}

#[cfg(test)]
fn diff_projections(
    before: &WorkspaceExplorerTreeProjection,
    after: &WorkspaceExplorerTreeProjection,
    version: u64,
) -> WorkspaceExplorerProjectionDelta {
    try_diff_sorted_projections(before, after, version)
        .unwrap_or_else(|| diff_projections_legacy(before, after, version))
}

/// The full projector and the incremental file-only path already maintain
/// these collection orders. Only a strict order permits a merge diff:
/// duplicate bindings or colliding placeholder paths must keep the original
/// ID-based, last-value-wins comparison semantics.
fn strictly_ordered<T>(items: &[T], key: impl Fn(&T) -> &str) -> bool {
    items.windows(2).all(|pair| key(&pair[0]) < key(&pair[1]))
}

fn node_path_identifies_id(node: &WorkspaceExplorerTreeNode) -> bool {
    if node.id == "workspace-root" {
        return node.virtual_path == "/";
    }
    let Some(path) = node.virtual_path.strip_prefix('/') else {
        return false;
    };
    if let Some(relative) = node.id.strip_prefix("path:") {
        return !relative.is_empty() && path == relative;
    }
    if let Some(parent) = node.id.strip_prefix("__alera_placeholder__:") {
        return path.strip_suffix("/.alera-placeholder") == Some(parent);
    }
    false
}

/// Merge two strictly ordered collections without allocating whole-tree
/// lookup maps. The order key may differ from the stable identity (nodes
/// are sorted by virtual path but identified by node ID). When a placeholder
/// and a real node occupy the same path across snapshots, emit the new node
/// and remove the old ID. Preserve each side's original output ordering.
fn diff_ordered<T: Clone + PartialEq>(
    before: &[T],
    after: &[T],
    key: impl Fn(&T) -> &str,
    identity: impl Fn(&T) -> &str,
) -> (Vec<T>, Vec<String>) {
    use std::cmp::Ordering;
    let mut changed = Vec::new();
    let mut removed = Vec::new();
    let mut old = before.iter().peekable();
    let mut new = after.iter().peekable();
    while let (Some(&previous), Some(&current)) = (old.peek(), new.peek()) {
        match key(previous).cmp(key(current)) {
            Ordering::Less => {
                removed.push(identity(previous).to_owned());
                old.next();
            }
            Ordering::Greater => {
                changed.push(current.clone());
                new.next();
            }
            Ordering::Equal => {
                if identity(previous) != identity(current) {
                    removed.push(identity(previous).to_owned());
                    changed.push(current.clone());
                } else if previous != current {
                    changed.push(current.clone());
                }
                old.next();
                new.next();
            }
        }
    }
    removed.extend(old.map(|item| identity(item).to_owned()));
    changed.extend(new.cloned());
    (changed, removed)
}

#[cfg(test)]
fn try_diff_sorted_projections(
    before: &WorkspaceExplorerTreeProjection,
    after: &WorkspaceExplorerTreeProjection,
    version: u64,
) -> Option<WorkspaceExplorerProjectionDelta> {
    fn directory_path(directory: &WorkspaceExplorerDirectoryChildren) -> &str {
        &directory.relative_path
    }
    if !strictly_ordered(&before.directories, directory_path)
        || !strictly_ordered(&after.directories, directory_path)
    {
        return None;
    }
    let mut delta = try_diff_sorted_graph(
        before,
        after,
        version,
        before.directories.is_empty() && !after.directories.is_empty(),
    )?;
    (delta.changed_directories, delta.removed_directory_paths) = diff_ordered(
        &before.directories,
        &after.directories,
        directory_path,
        directory_path,
    );
    Some(delta)
}

fn try_diff_sorted_graph(
    before: &WorkspaceExplorerTreeProjection,
    after: &WorkspaceExplorerTreeProjection,
    version: u64,
    bootstrap_root: bool,
) -> Option<WorkspaceExplorerProjectionDelta> {
    // Use noncapturing functions for the borrowed keys to preserve the
    // input lifetime across the generic comparison helper.
    fn binding_id(binding: &WorkspaceExplorerEntryBinding) -> &str {
        &binding.node_id
    }
    fn node_path(node: &WorkspaceExplorerTreeNode) -> &str {
        &node.virtual_path
    }
    fn node_id(node: &WorkspaceExplorerTreeNode) -> &str {
        &node.id
    }
    for projection in [before, after] {
        if !strictly_ordered(&projection.nodes, node_path)
            || !strictly_ordered(&projection.entry_bindings, binding_id)
            || !projection.nodes.iter().all(node_path_identifies_id)
        {
            return None;
        }
    }
    let (mut changed_nodes, removed_node_ids) =
        diff_ordered(&before.nodes, &after.nodes, node_path, node_id);
    let (changed_bindings, removed_binding_node_ids) = diff_ordered(
        &before.entry_bindings,
        &after.entry_bindings,
        binding_id,
        binding_id,
    );
    // Match legacy first-loaded-empty-root bootstrap behavior.
    if bootstrap_root {
        if let Some(root) = after
            .nodes
            .first()
            .filter(|node| node.id == "workspace-root")
        {
            if changed_nodes
                .first()
                .is_none_or(|node| node.id != "workspace-root")
            {
                changed_nodes.insert(0, root.clone());
            }
        }
    }
    Some(WorkspaceExplorerProjectionDelta {
        version,
        changed_directories: Vec::new(),
        removed_directory_paths: Vec::new(),
        changed_nodes,
        removed_node_ids,
        changed_bindings,
        removed_binding_node_ids,
    })
}

#[cfg(test)]
fn diff_projections_legacy(
    before: &WorkspaceExplorerTreeProjection,
    after: &WorkspaceExplorerTreeProjection,
    version: u64,
) -> WorkspaceExplorerProjectionDelta {
    let previous_directories = before
        .directories
        .iter()
        .map(|directory| (directory.relative_path.as_str(), directory))
        .collect::<HashMap<_, _>>();
    let current_directories = after
        .directories
        .iter()
        .map(|directory| directory.relative_path.as_str())
        .collect::<std::collections::HashSet<_>>();
    let changed_directories = after
        .directories
        .iter()
        .filter(|directory| {
            previous_directories
                .get(directory.relative_path.as_str())
                .is_none_or(|previous| *previous != *directory)
        })
        .cloned()
        .collect();
    let removed_directory_paths = before
        .directories
        .iter()
        .filter(|directory| !current_directories.contains(directory.relative_path.as_str()))
        .map(|directory| directory.relative_path.clone())
        .collect();

    let mut delta = diff_graph_legacy(
        before,
        after,
        version,
        before.directories.is_empty() && !after.directories.is_empty(),
    );
    delta.changed_directories = changed_directories;
    delta.removed_directory_paths = removed_directory_paths;
    delta
}

fn diff_graph_legacy(
    before: &WorkspaceExplorerTreeProjection,
    after: &WorkspaceExplorerTreeProjection,
    version: u64,
    bootstrap_root: bool,
) -> WorkspaceExplorerProjectionDelta {
    let previous_nodes = before
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<HashMap<_, _>>();
    let current_nodes = after
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    let changed_nodes = after
        .nodes
        .iter()
        .filter(|node| {
            // The first client update may turn an empty native workspace into
            // an explicitly loaded (but still empty) root directory. The UI
            // has no local projection yet, so include the root node once.
            (node.id == "workspace-root" && bootstrap_root)
                || previous_nodes
                    .get(node.id.as_str())
                    .is_none_or(|previous| *previous != *node)
        })
        .cloned()
        .collect::<Vec<WorkspaceExplorerTreeNode>>();
    let removed_node_ids = before
        .nodes
        .iter()
        .filter(|node| !current_nodes.contains(node.id.as_str()))
        .map(|node| node.id.clone())
        .collect();

    let previous_bindings = before
        .entry_bindings
        .iter()
        .map(|binding| (binding.node_id.as_str(), binding))
        .collect::<HashMap<_, _>>();
    let current_bindings = after
        .entry_bindings
        .iter()
        .map(|binding| binding.node_id.as_str())
        .collect::<std::collections::HashSet<_>>();
    let changed_bindings = after
        .entry_bindings
        .iter()
        .filter(|binding| {
            previous_bindings
                .get(binding.node_id.as_str())
                .is_none_or(|previous| *previous != *binding)
        })
        .cloned()
        .collect::<Vec<WorkspaceExplorerEntryBinding>>();
    let removed_binding_node_ids = before
        .entry_bindings
        .iter()
        .filter(|binding| !current_bindings.contains(binding.node_id.as_str()))
        .map(|binding| binding.node_id.clone())
        .collect();

    WorkspaceExplorerProjectionDelta {
        version,
        changed_directories: Vec::new(),
        removed_directory_paths: Vec::new(),
        changed_nodes,
        removed_node_ids,
        changed_bindings,
        removed_binding_node_ids,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::workspace_files::{
        WorkspaceFileEntry, WorkspaceFileGitStatus, WorkspaceFileKind,
    };

    fn entry(path: &str, kind: WorkspaceFileKind, has_children_hint: bool) -> WorkspaceFileEntry {
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
            has_children_hint,
            git_status: None,
        }
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

    /// Manual native CPU profile, not a CI timing assertion.
    /// cargo test --release --manifest-path rust/Cargo.toml \
    ///   profile_explorer_delta_native_cpu --lib -- --ignored --nocapture
    #[test]
    #[ignore = "manual 100/300/1000-directory Explorer native CPU benchmark"]
    fn profile_explorer_delta_native_cpu() {
        use std::hint::black_box;
        use std::time::Instant;

        for directory_count in [100usize, 300, 1000] {
            let mut directories = Vec::with_capacity(directory_count + 1);
            directories.push(children(
                "",
                (0..directory_count)
                    .map(|index| entry(&format!("dir{index}"), WorkspaceFileKind::Directory, true))
                    .collect(),
            ));
            for index in 0..directory_count {
                directories.push(children(
                    &format!("dir{index}"),
                    vec![entry(
                        &format!("dir{index}/original.txt"),
                        WorkspaceFileKind::File,
                        false,
                    )],
                ));
            }
            let target = format!("dir{}", directory_count / 2);
            let mut current = explorer_tree::project_tree(
                "alera".into(),
                "/repo".into(),
                directories.clone(),
                None,
            );
            let started = start("alera".into(), "/repo".into(), directories).unwrap();
            let mut handle = started.clone();
            let mut full_projection_us = Vec::new();
            let mut delta_diff_us = Vec::new();
            let mut native_update_us = Vec::new();
            let mut emitted_nodes = 0usize;
            for round in 0..11 {
                let mut changed = entry(
                    &format!("{target}/original.txt"),
                    WorkspaceFileKind::File,
                    false,
                );
                changed.size = round;
                let replacement = children(
                    &target,
                    vec![
                        changed,
                        entry(
                            &format!("{target}/extra.txt"),
                            WorkspaceFileKind::File,
                            false,
                        ),
                    ],
                );
                let began = Instant::now();
                let projected = explorer_tree::project_tree(
                    "alera".into(),
                    "/repo".into(),
                    current.directories.clone(),
                    Some(replacement.clone()),
                );
                let full_elapsed = began.elapsed().as_secs_f64() * 1_000_000.0;
                let began = Instant::now();
                let expected =
                    diff_projections(black_box(&current), black_box(&projected), round + 1);
                let diff_elapsed = began.elapsed().as_secs_f64() * 1_000_000.0;
                let began = Instant::now();
                let actual = update(handle.clone(), replacement).unwrap();
                let delta_elapsed = began.elapsed().as_secs_f64() * 1_000_000.0;
                assert_eq!(actual.changed_nodes, expected.changed_nodes);
                assert_eq!(actual.changed_bindings, expected.changed_bindings);
                assert_eq!(actual.changed_directories, expected.changed_directories);
                assert_eq!(actual.removed_node_ids, expected.removed_node_ids);
                assert_eq!(
                    actual.removed_directory_paths,
                    expected.removed_directory_paths
                );
                assert_eq!(
                    actual.removed_binding_node_ids,
                    expected.removed_binding_node_ids
                );
                handle.version = actual.version;
                current = projected;
                emitted_nodes = actual.changed_nodes.len();
                if round >= 2 {
                    full_projection_us.push(full_elapsed);
                    delta_diff_us.push(diff_elapsed);
                    native_update_us.push(delta_elapsed);
                }
            }
            full_projection_us.sort_by(f64::total_cmp);
            delta_diff_us.sort_by(f64::total_cmp);
            native_update_us.sort_by(f64::total_cmp);
            eprintln!(
                "EXPLORER_NATIVE_PROFILE directories={directory_count} full_nodes={} changed_nodes={emitted_nodes} full_rebuild_median_us={:.1} delta_diff_median_us={:.1} native_session_update_median_us={:.1} samples=9",
                current.nodes.len(),
                full_projection_us[4],
                delta_diff_us[4],
                native_update_us[4]
            );
            stop(started);
        }
    }

    #[test]
    fn metadata_only_edits_match_full_tree_and_do_not_change_node_shape() {
        let root = children("", vec![entry("src", WorkspaceFileKind::Directory, true)]);
        let original = children(
            "src",
            vec![
                entry("src/a.rs", WorkspaceFileKind::File, false),
                entry("src/b.rs", WorkspaceFileKind::File, false),
            ],
        );
        let snapshots = vec![root, original.clone()];
        let before =
            explorer_tree::project_tree("alera".into(), "/repo".into(), snapshots.clone(), None);
        let handle = start("alera".into(), "/repo".into(), snapshots).unwrap();
        let mut replacement = original.clone();
        replacement.children[0].size = 1024;
        replacement.children[0].modified_millis = 456;
        replacement.children[0].content_token = "updated-token".into();
        replacement.children[0].git_status = Some(WorkspaceFileGitStatus::Modified);
        replacement.children[0].is_hidden = true;
        replacement.children[0].is_ignored = true;
        replacement.children[1].is_protected = true;
        replacement.children[1].is_symlink = true;

        let expected = explorer_tree::project_tree(
            "alera".into(),
            "/repo".into(),
            before.directories.clone(),
            Some(replacement.clone()),
        );
        let delta = update(handle.clone(), replacement.clone()).unwrap();
        assert_eq!(delta.version, 1);
        assert_eq!(delta.changed_directories, vec![replacement]);
        assert!(delta.changed_nodes.is_empty());
        assert!(delta.removed_node_ids.is_empty());
        assert!(delta.changed_bindings.is_empty());
        assert!(delta.removed_binding_node_ids.is_empty());
        assert!(delta.removed_directory_paths.is_empty());
        let registry = sessions().lock().unwrap();
        let session = registry.get(&handle.id).unwrap();
        assert_eq!(session.projection.directories, expected.directories);
        assert_eq!(session.projection.nodes, expected.nodes);
        assert_eq!(session.projection.entry_bindings, expected.entry_bindings);
        drop(registry);
        stop(handle);
    }

    #[test]
    fn structure_changes_and_reordering_match_full_tree_reference() {
        let root = children("", vec![entry("src", WorkspaceFileKind::Directory, true)]);
        let original = children(
            "src",
            vec![
                entry("src/alpha", WorkspaceFileKind::Directory, false),
                entry("src/beta.rs", WorkspaceFileKind::File, false),
            ],
        );
        let snapshots = vec![root, original];
        let mut full =
            explorer_tree::project_tree("alera".into(), "/repo".into(), snapshots.clone(), None);
        let handle = start("alera".into(), "/repo".into(), snapshots).unwrap();
        let updates = [
            // Reorder equal paths: childIds must reflect the new order.
            children(
                "src",
                vec![
                    entry("src/beta.rs", WorkspaceFileKind::File, false),
                    entry("src/alpha", WorkspaceFileKind::Directory, false),
                ],
            ),
            // Placeholder visibility changes the projected node graph.
            children(
                "src",
                vec![
                    entry("src/beta.rs", WorkspaceFileKind::File, false),
                    entry("src/alpha", WorkspaceFileKind::Directory, true),
                ],
            ),
            // Removal prunes nodes and bindings.
            children(
                "src",
                vec![entry("src/alpha", WorkspaceFileKind::Directory, true)],
            ),
        ];
        for (round, replacement) in updates.into_iter().enumerate() {
            let expected = explorer_tree::project_tree(
                "alera".into(),
                "/repo".into(),
                full.directories,
                Some(replacement.clone()),
            );
            let actual = update(
                WorkspaceExplorerProjectionHandle {
                    id: handle.id,
                    version: round as u64,
                },
                replacement,
            )
            .unwrap();
            assert_eq!(actual.version, round as u64 + 1);
            let registry = sessions().lock().unwrap();
            let session = registry.get(&handle.id).unwrap();
            assert_eq!(session.projection.directories, expected.directories);
            assert_eq!(session.projection.nodes, expected.nodes);
            assert_eq!(session.projection.entry_bindings, expected.entry_bindings);
            drop(registry);
            full = expected;
        }
        stop(handle);
    }

    #[test]
    fn file_only_structure_bulk_add_remove_reorder_matches_full_projector() {
        for target in ["", "src/deep"] {
            let mut directories = vec![children(
                "",
                if target.is_empty() {
                    vec![entry("readme.txt", WorkspaceFileKind::File, false)]
                } else {
                    vec![entry("src", WorkspaceFileKind::Directory, true)]
                },
            )];
            if !target.is_empty() {
                directories.push(children(
                    "src",
                    vec![entry("src/deep", WorkspaceFileKind::Directory, true)],
                ));
                directories.push(children(
                    target,
                    vec![entry("src/deep/readme.txt", WorkspaceFileKind::File, false)],
                ));
            }
            let mut full = explorer_tree::project_tree(
                "alera".into(),
                "/repo".into(),
                directories.clone(),
                None,
            );
            let handle = start("alera".into(), "/repo".into(), directories).unwrap();
            let original = if target.is_empty() {
                "readme.txt".to_owned()
            } else {
                format!("{target}/readme.txt")
            };
            for round in 0..7 {
                let mut entries = vec![entry(&original, WorkspaceFileKind::File, false)];
                if round % 3 != 2 {
                    for number in 0..125 {
                        entries.push(entry(
                            &if target.is_empty() {
                                format!("file_{number:03}.rs")
                            } else {
                                format!("{target}/file_{number:03}.rs")
                            },
                            WorkspaceFileKind::File,
                            false,
                        ));
                    }
                }
                if round % 3 == 1 {
                    entries.reverse();
                    entries[0].size = 1024;
                }
                let replacement = children(target, entries);
                let expected = explorer_tree::project_tree(
                    "alera".into(),
                    "/repo".into(),
                    full.directories.clone(),
                    Some(replacement.clone()),
                );
                let expected_delta = diff_projections(&full, &expected, round + 1);
                let actual = update(
                    WorkspaceExplorerProjectionHandle {
                        id: handle.id,
                        version: round,
                    },
                    replacement,
                )
                .unwrap();
                assert_eq!(
                    actual.changed_directories, expected_delta.changed_directories,
                    "directories at {target:?} round {round}"
                );
                assert_eq!(
                    actual.removed_directory_paths, expected_delta.removed_directory_paths,
                    "removed directories at {target:?} round {round}"
                );
                assert_eq!(
                    actual.changed_nodes, expected_delta.changed_nodes,
                    "changed nodes at {target:?} round {round}"
                );
                assert_eq!(
                    actual.removed_node_ids, expected_delta.removed_node_ids,
                    "removed nodes at {target:?} round {round}"
                );
                assert_eq!(
                    actual.changed_bindings, expected_delta.changed_bindings,
                    "changed bindings at {target:?} round {round}"
                );
                assert_eq!(
                    actual.removed_binding_node_ids, expected_delta.removed_binding_node_ids,
                    "removed bindings at {target:?} round {round}"
                );
                let registry = sessions().lock().unwrap();
                let actual_full = &registry.get(&handle.id).unwrap().projection;
                assert_eq!(
                    actual_full.directories, expected.directories,
                    "full directories {target:?} round {round}"
                );
                assert_eq!(
                    actual_full.nodes, expected.nodes,
                    "full nodes {target:?} round {round}"
                );
                assert_eq!(
                    actual_full.entry_bindings, expected.entry_bindings,
                    "full bindings {target:?} round {round}"
                );
                drop(registry);
                full = expected;
            }
            stop(handle);
        }
    }

    /// Manual Release CPU benchmark of large file-only structural updates
    /// in one loaded folder among many loaded worktree folders.
    /// cargo test --release --manifest-path rust/Cargo.toml \
    ///   profile_explorer_structural_add_remove --lib -- --ignored --nocapture
    #[test]
    #[ignore = "manual native Explorer structural 100/300/1000-directory Release benchmark"]
    fn profile_explorer_structural_add_remove() {
        use std::time::Instant;
        for count in [100usize, 300, 1000] {
            let mut snapshots = vec![children(
                "",
                (0..count)
                    .map(|i| entry(&format!("dir{i}"), WorkspaceFileKind::Directory, true))
                    .collect(),
            )];
            for i in 0..count {
                snapshots.push(children(
                    &format!("dir{i}"),
                    vec![entry(
                        &format!("dir{i}/original.txt"),
                        WorkspaceFileKind::File,
                        false,
                    )],
                ));
            }
            let path = format!("dir{}", count / 2);
            let handle = start("alera".into(), "/repo".into(), snapshots).unwrap();
            let mut version = 0;
            let mut additions = Vec::new();
            let mut removals = Vec::new();
            for step in 0..18 {
                let mut entries = vec![entry(
                    &format!("{path}/original.txt"),
                    WorkspaceFileKind::File,
                    false,
                )];
                if step % 2 == 0 {
                    for i in 0..150 {
                        entries.push(entry(
                            &format!("{path}/f{i:03}.rs"),
                            WorkspaceFileKind::File,
                            false,
                        ));
                    }
                }
                let now = Instant::now();
                let result = update(
                    WorkspaceExplorerProjectionHandle {
                        id: handle.id,
                        version,
                    },
                    children(&path, entries),
                )
                .unwrap();
                let elapsed = now.elapsed().as_secs_f64() * 1_000_000.0;
                version = result.version;
                if step >= 4 {
                    if step % 2 == 0 {
                        additions.push(elapsed);
                    } else {
                        removals.push(elapsed);
                    }
                }
            }
            additions.sort_by(f64::total_cmp);
            removals.sort_by(f64::total_cmp);
            eprintln!(
                "EXPLORER_STRUCTURAL_PROFILE directories={count} batch_files=150 add_median_us={:.1} remove_median_us={:.1} samples={}",
                additions[additions.len() / 2],
                removals[removals.len() / 2],
                additions.len(),
            );
            stop(handle);
        }
    }

    fn assert_projection_equal(
        actual: &WorkspaceExplorerTreeProjection,
        expected: &WorkspaceExplorerTreeProjection,
    ) {
        assert_eq!(actual.directories, expected.directories);
        assert_eq!(actual.nodes, expected.nodes);
        assert_eq!(actual.entry_bindings, expected.entry_bindings);
    }

    fn assert_delta_equal(
        actual: &WorkspaceExplorerProjectionDelta,
        expected: &WorkspaceExplorerProjectionDelta,
    ) {
        assert_eq!(actual.version, expected.version);
        assert_eq!(actual.changed_directories, expected.changed_directories);
        assert_eq!(
            actual.removed_directory_paths,
            expected.removed_directory_paths
        );
        assert_eq!(actual.changed_nodes, expected.changed_nodes);
        assert_eq!(actual.removed_node_ids, expected.removed_node_ids);
        assert_eq!(actual.changed_bindings, expected.changed_bindings);
        assert_eq!(
            actual.removed_binding_node_ids,
            expected.removed_binding_node_ids
        );
    }

    #[test]
    fn sparse_plan_keeps_order_and_rejects_ambiguous_or_existing_keys() {
        let mut items = vec!["a", "c", "e", "g"];
        let plan = sparse_removal_plan(&items, &["c", "g"], &["b", "f"], |item| item).unwrap();
        assert_eq!(plan, vec![3, 1]);
        apply_sparse_plan(&mut items, plan, ["b", "f"], |item| item);
        assert_eq!(items, ["a", "b", "e", "f"]);
        assert!(sparse_removal_plan(&["a", "a", "b"], &["a"], &["c"], |item| item).is_none());
        assert!(sparse_removal_plan(&items, &["missing"], &["c"], |item| item).is_none());
        assert!(sparse_removal_plan(&items, &["e", "e"], &["c"], |item| item).is_none());
        assert!(sparse_removal_plan(&items, &["a"], &["e"], |item| item).is_none());
    }

    fn sparse_fixture(
        folder_count: usize,
        files_per_folder: usize,
    ) -> WorkspaceExplorerTreeProjection {
        let mut directories = vec![children(
            "",
            (0..folder_count)
                .map(|index| {
                    entry(
                        &format!("folder-{index:03}"),
                        WorkspaceFileKind::Directory,
                        true,
                    )
                })
                .collect(),
        )];
        for index in 0..folder_count {
            let folder = format!("folder-{index:03}");
            directories.push(children(
                &folder,
                (0..files_per_folder)
                    .map(|number| {
                        entry(
                            &format!("{folder}/file-{number:04}.rs"),
                            WorkspaceFileKind::File,
                            false,
                        )
                    })
                    .collect(),
            ));
        }
        explorer_tree::project_tree("alera".into(), "/repo".into(), directories, None)
    }

    fn local_session(projection: WorkspaceExplorerTreeProjection) -> ExplorerProjectionSession {
        ExplorerProjectionSession {
            workspace_name: "alera".into(),
            workspace_path: "/repo".into(),
            version: 0,
            projection,
            direct_cache: false,
        }
    }

    fn rebuild_reference(
        session: &mut ExplorerProjectionSession,
        replacement: WorkspaceExplorerDirectoryChildren,
        version: u64,
    ) -> WorkspaceExplorerProjectionDelta {
        let updated = explorer_tree::project_tree(
            session.workspace_name.clone(),
            session.workspace_path.clone(),
            session.projection.directories.clone(),
            Some(replacement),
        );
        let delta = diff_projections(&session.projection, &updated, version);
        session.version = version;
        session.projection = updated;
        delta
    }

    fn direct_test_session(
        directories: Vec<WorkspaceExplorerDirectoryChildren>,
    ) -> ExplorerProjectionSession {
        let (projection, direct_cache) = explorer_tree::project_tree_with_direct_cache(
            "alera".into(),
            "/repo".into(),
            directories,
            None,
        );
        assert!(direct_cache);
        let mut session = local_session(projection);
        session.direct_cache = true;
        session
    }

    fn apply_mixed_test(
        session: &mut ExplorerProjectionSession,
        replacement: &WorkspaceExplorerDirectoryChildren,
        version: u64,
    ) -> Option<WorkspaceExplorerProjectionDelta> {
        let path = replacement.relative_path.trim_matches('/');
        let index = session
            .projection
            .directories
            .binary_search_by(|row| row.relative_path.as_str().cmp(path))
            .ok()?;
        try_update_mixed_children(session, index, path, replacement, version)
    }

    #[test]
    fn mixed_incremental_preserves_loaded_subtrees_lazy_kind_and_child_order() {
        use WorkspaceFileKind::{Directory, File};
        let mut session = direct_test_session(vec![
            children(
                "",
                vec![
                    entry("src", Directory, true),
                    entry("keep", Directory, true),
                ],
            ),
            children(
                "src",
                vec![
                    entry("src/loaded", Directory, true),
                    entry("src/lazy", Directory, true),
                    entry("src/file", File, false),
                ],
            ),
            children(
                "src/loaded",
                vec![entry("src/loaded/.alera-placeholder", File, false)],
            ),
            children("keep", vec![entry("keep/untouched", File, false)]),
        ]);
        let mut reference = local_session(session.projection.clone());
        let untouched_id = session
            .projection
            .nodes
            .iter()
            .find(|node| node.id == "path:keep/untouched")
            .unwrap()
            .id
            .as_ptr();
        let loaded_id = session
            .projection
            .nodes
            .iter()
            .find(|node| node.id == "path:src/loaded/.alera-placeholder")
            .unwrap()
            .id
            .as_ptr();
        let updates = [
            children(
                "src",
                vec![
                    entry("src/file", Directory, true),
                    entry("src/loaded", Directory, false),
                    entry("src/lazy", Directory, false),
                    entry("src/new", File, false),
                ],
            ),
            children(
                "/src/",
                vec![
                    entry("src/loaded", Directory, true),
                    entry("src/file", File, false),
                    entry("src/lazy", File, false),
                    entry("src/new", Directory, true),
                ],
            ),
            children(
                "src",
                vec![
                    entry("src/new", Directory, false),
                    entry("src/loaded", Directory, true),
                    entry("src/lazy", Directory, true),
                ],
            ),
            children(
                "",
                vec![
                    entry("keep", Directory, false),
                    entry("src", Directory, true),
                ],
            ),
            children(
                "src",
                vec![
                    entry("src/loaded", Directory, true),
                    entry("src/lazy", Directory, true),
                ],
            ),
        ];
        for (index, replacement) in updates.into_iter().enumerate() {
            let version = index as u64 + 1;
            let expected = rebuild_reference(&mut reference, replacement.clone(), version);
            let actual = apply_mixed_test(&mut session, &replacement, version)
                .expect("eligible sparse mixed edit");
            assert_delta_equal(&actual, &expected);
            assert_projection_equal(&session.projection, &reference.projection);
            assert!(session.direct_cache);
            assert_eq!(
                session
                    .projection
                    .nodes
                    .iter()
                    .find(|node| node.id == "path:keep/untouched")
                    .unwrap()
                    .id
                    .as_ptr(),
                untouched_id
            );
            assert_eq!(
                session
                    .projection
                    .nodes
                    .iter()
                    .find(|node| node.id == "path:src/loaded/.alera-placeholder")
                    .unwrap()
                    .id
                    .as_ptr(),
                loaded_id
            );
        }
    }

    #[test]
    fn mixed_incremental_rejects_loaded_removal_noncanonical_duplicates_and_bulk_before_mutation() {
        use WorkspaceFileKind::{Directory, File};
        let mut session = direct_test_session(vec![
            children("", vec![entry("src", Directory, true)]),
            children(
                "src",
                vec![
                    entry("src/loaded", Directory, true),
                    entry("src/file", File, false),
                ],
            ),
            children("src/loaded", vec![entry("src/loaded/child", File, false)]),
        ]);
        let seed = session.projection.clone();
        let mut bulk = vec![
            entry("src/loaded", Directory, true),
            entry("src/file", File, false),
        ];
        bulk.extend((0..20).map(|n| entry(&format!("src/lazy-{n}"), Directory, true)));
        let rejected = [
            children("src", vec![entry("src/file", File, false)]),
            children(
                "src",
                vec![
                    entry("src/loaded", File, false),
                    entry("src/file", File, false),
                ],
            ),
            children(
                "src",
                vec![
                    entry("src/loaded", Directory, true),
                    entry("src/.", File, false),
                ],
            ),
            children(
                "src",
                vec![
                    entry("src/loaded", Directory, true),
                    entry("src/file", File, false),
                    entry("src/file", Directory, true),
                ],
            ),
            children("src", bulk),
        ];
        for replacement in rejected {
            assert!(apply_mixed_test(&mut session, &replacement, 1).is_none());
            assert_projection_equal(&session.projection, &seed);
            assert_eq!(session.version, 0);
            let mut fallback = local_session(seed.clone());
            let mut reference = local_session(seed.clone());
            let actual = rebuild_projection(&mut fallback, replacement.clone(), 1);
            let expected = rebuild_reference(&mut reference, replacement, 1);
            assert_delta_equal(&actual, &expected);
            assert_projection_equal(&fallback.projection, &reference.projection);
        }
        session.direct_cache = false;
        let replacement = children(
            "src",
            vec![
                entry("src/loaded", Directory, true),
                entry("src/lazy", Directory, true),
            ],
        );
        assert!(apply_mixed_test(&mut session, &replacement, 1).is_none());
        assert_projection_equal(&session.projection, &seed);
    }

    #[test]
    fn mixed_incremental_session_flag_survives_fast_edits_and_revalidates_on_rebuild() {
        use WorkspaceFileKind::{Directory, File};
        let seed = vec![
            children("", vec![entry("src", Directory, true)]),
            children("src", vec![entry("src/file", File, false)]),
        ];
        let mut reference = direct_test_session(seed.clone());
        let handle = start("alera".into(), "/repo".into(), seed).unwrap();
        let updates = [
            children("src", vec![entry("src/renamed", File, false)]),
            children(
                "src",
                vec![
                    entry("src/renamed", File, false),
                    entry("src/lazy", Directory, true),
                ],
            ),
            children(
                "src/lazy",
                vec![entry("src/lazy/.alera-placeholder", File, false)],
            ),
            children("src", vec![entry("src/lazy", File, false)]),
            children("src", vec![entry("src/.", File, false)]),
            children(
                "src",
                vec![
                    entry("src/file", File, false),
                    entry("src/new", Directory, true),
                ],
            ),
        ];
        for (index, replacement) in updates.into_iter().enumerate() {
            let version = index as u64 + 1;
            let expected = rebuild_reference(&mut reference, replacement.clone(), version);
            let actual = update(
                WorkspaceExplorerProjectionHandle {
                    id: handle.id,
                    version: index as u64,
                },
                replacement,
            )
            .unwrap();
            assert_delta_equal(&actual, &expected);
            let registry = sessions().lock().unwrap();
            let current = &registry[&handle.id];
            assert_projection_equal(&current.projection, &reference.projection);
            assert_eq!(current.direct_cache, index != 4);
        }
        stop(handle);
    }

    #[test]
    #[ignore = "synthetic native sparse mixed structural update benchmark"]
    fn benchmark_mixed_explorer_incremental() {
        use std::time::{Duration, Instant};
        use WorkspaceFileKind::Directory;
        let seed = sparse_fixture(200, 200);
        let mut mixed = seed
            .directories
            .iter()
            .find(|row| row.relative_path == "folder-100")
            .unwrap()
            .clone();
        mixed
            .children
            .push(entry("folder-100/lazy", Directory, true));
        let mut reordered = seed
            .directories
            .iter()
            .find(|row| row.relative_path.is_empty())
            .unwrap()
            .clone();
        reordered.children.reverse();
        let mut root_add = seed
            .directories
            .iter()
            .find(|row| row.relative_path.is_empty())
            .unwrap()
            .clone();
        root_add
            .children
            .push(entry("new-root-folder", Directory, true));
        fn median(samples: &mut [Duration]) -> f64 {
            samples.sort();
            samples[samples.len() / 2].as_secs_f64() * 1000.0
        }
        for (label, replacement) in [
            ("mixed-add", mixed),
            ("root-reorder", reordered),
            ("root-add", root_add),
        ] {
            let mut reference = local_session(seed.clone());
            let expected = rebuild_projection(&mut reference, replacement.clone(), 1);
            let mut old = Vec::new();
            let mut new = Vec::new();
            for sample in 0..7 {
                for optimized in if sample % 2 == 0 {
                    [false, true]
                } else {
                    [true, false]
                } {
                    let mut session = local_session(seed.clone());
                    session.direct_cache = true;
                    let update = replacement.clone();
                    let start = Instant::now();
                    let actual = if optimized {
                        apply_mixed_test(&mut session, &update, 1).expect("eligible mixed fixture")
                    } else {
                        rebuild_projection(&mut session, update, 1)
                    };
                    let elapsed = start.elapsed();
                    assert_delta_equal(&actual, &expected);
                    assert_projection_equal(&session.projection, &reference.projection);
                    if optimized {
                        new.push(elapsed);
                    } else {
                        old.push(elapsed);
                    }
                }
            }
            let old = median(&mut old);
            let new = median(&mut new);
            println!("Classic mixed incremental shape={label} before_nodes={} changed_nodes={} old_ms={old:.3} new_ms={new:.3} speedup={:.2}x", seed.nodes.len(), expected.changed_nodes.len(), old/new);
        }
    }

    fn apply_expansion_or_prune_test(
        session: &mut ExplorerProjectionSession,
        replacement: &WorkspaceExplorerDirectoryChildren,
        version: u64,
    ) -> Option<WorkspaceExplorerProjectionDelta> {
        let path = replacement.relative_path.trim_matches('/');
        match session
            .projection
            .directories
            .binary_search_by(|row| row.relative_path.as_str().cmp(path))
        {
            Ok(index) => try_prune_loaded_children(session, index, path, replacement, version),
            Err(_) => try_expand_directory(session, path, replacement, version),
        }
    }

    #[test]
    fn incremental_first_expansion_replaces_placeholder_and_preserves_other_buffers() {
        use WorkspaceFileKind::{Directory, File};
        let mut session = direct_test_session(vec![
            children(
                "",
                vec![
                    entry("a", Directory, true),
                    entry("a-old", Directory, true),
                    entry("a0", Directory, false),
                    entry("中文", Directory, false),
                    entry("a.b", File, false),
                ],
            ),
            children("a-old", vec![entry("a-old/untouched", File, false)]),
        ]);
        let mut reference = local_session(session.projection.clone());
        let unchanged = session
            .projection
            .nodes
            .iter()
            .find(|node| node.id == "path:a-old/untouched")
            .unwrap()
            .id
            .as_ptr();
        let updates = [
            children(
                "/a/",
                vec![
                    entry("a/z", File, false),
                    entry("a/.alera-placeholder", File, false),
                    entry("a/sub", Directory, true),
                ],
            ),
            children(
                "a/sub",
                vec![
                    entry("a/sub/child", File, false),
                    entry("a/sub/empty", Directory, false),
                ],
            ),
            children("a/sub/empty", vec![]),
            children("a0", vec![]),
            children("中文", vec![entry("中文/檔案", File, false)]),
        ];
        for (index, replacement) in updates.into_iter().enumerate() {
            let version = index as u64 + 1;
            let expected = rebuild_reference(&mut reference, replacement.clone(), version);
            let actual = apply_expansion_or_prune_test(&mut session, &replacement, version)
                .expect("eligible first expansion");
            assert_delta_equal(&actual, &expected);
            assert_projection_equal(&session.projection, &reference.projection);
            assert_eq!(session.version, version);
            assert!(session.direct_cache);
            assert_eq!(
                session
                    .projection
                    .nodes
                    .iter()
                    .find(|node| node.id == "path:a-old/untouched")
                    .unwrap()
                    .id
                    .as_ptr(),
                unchanged
            );
            if index == 0 {
                assert_eq!(actual.removed_node_ids, ["__alera_placeholder__:a"]);
                assert!(actual
                    .changed_nodes
                    .iter()
                    .any(|node| node.id == "path:a/.alera-placeholder"));
            }
        }
    }

    #[test]
    fn incremental_prune_preserves_prefix_siblings_loaded_conversion_and_delta_order() {
        use WorkspaceFileKind::{Directory, File};
        let mut session = direct_test_session(vec![
            children(
                "",
                vec![
                    entry("a", Directory, true),
                    entry("a-old", Directory, true),
                    entry("a0", Directory, true),
                    entry("b", Directory, true),
                    entry("b.old", File, false),
                    entry("中文", Directory, true),
                ],
            ),
            children(
                "a",
                vec![
                    entry("a/deep", Directory, true),
                    entry("a/lazy", Directory, true),
                ],
            ),
            children("a/deep", vec![entry("a/deep/f", File, false)]),
            children("a-old", vec![entry("a-old/keep", File, false)]),
            children("a0", vec![entry("a0/keep", File, false)]),
            children("b", vec![entry("b/file", File, false)]),
            children("中文", vec![]),
        ]);
        let mut reference = local_session(session.projection.clone());
        let unchanged = session
            .projection
            .nodes
            .iter()
            .find(|node| node.id == "path:a-old/keep")
            .unwrap()
            .id
            .as_ptr();
        let updates = [
            // Remove a nested subtree and an ordinary file while converting a
            // second loaded folder. Retain same-prefix siblings in new order.
            children(
                "/",
                vec![
                    entry("a0", Directory, false),
                    entry("b", File, false),
                    entry("a-old", Directory, true),
                    entry("中文", Directory, true),
                ],
            ),
            // Delete a loaded empty folder.
            children(
                "",
                vec![
                    entry("a-old", Directory, true),
                    entry("a0", Directory, false),
                    entry("b", File, false),
                ],
            ),
        ];
        for (index, replacement) in updates.into_iter().enumerate() {
            let version = index as u64 + 1;
            let expected = rebuild_reference(&mut reference, replacement.clone(), version);
            let actual = apply_expansion_or_prune_test(&mut session, &replacement, version)
                .expect("eligible loaded pruning");
            assert_delta_equal(&actual, &expected);
            assert_projection_equal(&session.projection, &reference.projection);
            assert_eq!(
                session
                    .projection
                    .nodes
                    .iter()
                    .find(|node| node.id == "path:a-old/keep")
                    .unwrap()
                    .id
                    .as_ptr(),
                unchanged
            );
            if index == 0 {
                assert_eq!(actual.removed_directory_paths, ["a", "a/deep", "b"]);
                assert!(actual
                    .removed_node_ids
                    .iter()
                    .any(|id| id == "__alera_placeholder__:a/lazy"));
                assert!(!actual
                    .removed_binding_node_ids
                    .iter()
                    .any(|id| id == "path:b"));
                assert!(actual
                    .changed_nodes
                    .iter()
                    .any(|node| node.id == "path:b"
                        && node.kind == WorkspaceExplorerTreeNodeKind::File));
            }
        }
        // The removed subtree cannot be resurrected by a delayed listing.
        let late = children("a/deep", vec![entry("a/deep/late", File, false)]);
        let before = session.projection.clone();
        assert!(apply_expansion_or_prune_test(&mut session, &late, 3).is_none());
        assert_projection_equal(&session.projection, &before);
        let expected = rebuild_reference(&mut reference, late.clone(), 3);
        let actual = rebuild_projection(&mut session, late, 3);
        assert_delta_equal(&actual, &expected);
        assert_projection_equal(&session.projection, &reference.projection);
    }

    #[test]
    fn incremental_expansion_and_prune_decline_unsafe_edits_without_mutation() {
        use WorkspaceFileKind::{Directory, File};
        let mut session = direct_test_session(vec![
            children(
                "",
                vec![
                    entry("loaded", Directory, true),
                    entry("lazy", Directory, true),
                    entry("file", File, false),
                ],
            ),
            children("loaded", vec![entry("loaded/child", File, false)]),
        ]);
        let seed = session.projection.clone();
        let rejected = [
            children("missing", vec![entry("missing/child", File, false)]),
            children("file", vec![entry("file/child", File, false)]),
            children("lazy", vec![entry("lazy/.", File, false)]),
            children(
                "lazy",
                vec![
                    entry("lazy/dup", File, false),
                    entry("lazy/dup", File, false),
                ],
            ),
            children(
                "",
                vec![entry("lazy", Directory, true), entry("new", File, false)],
            ),
            children(
                "",
                vec![entry("lazy", Directory, false), entry("file", File, false)],
            ),
        ];
        for replacement in rejected {
            assert!(apply_expansion_or_prune_test(&mut session, &replacement, 1).is_none());
            assert_eq!(session.version, 0);
            assert_projection_equal(&session.projection, &seed);
        }
        session.direct_cache = false;
        let valid = children("lazy", vec![entry("lazy/child", File, false)]);
        assert!(apply_expansion_or_prune_test(&mut session, &valid, 1).is_none());
        assert_projection_equal(&session.projection, &seed);
    }

    #[test]
    fn incremental_prune_many_disjoint_subtrees_matches_full_projection() {
        let seed = sparse_fixture(24, 12);
        let mut session = local_session(seed.clone());
        session.direct_cache = true;
        let mut reference = local_session(seed);
        let mut replacement = session.projection.directories[0].clone();
        replacement.children = replacement
            .children
            .into_iter()
            .enumerate()
            .filter(|(index, _)| index % 2 == 1)
            .map(|(_, child)| child)
            .collect();
        let expected = rebuild_reference(&mut reference, replacement.clone(), 1);
        let actual = apply_expansion_or_prune_test(&mut session, &replacement, 1)
            .expect("disjoint loaded subtree removal");
        assert_delta_equal(&actual, &expected);
        assert_projection_equal(&session.projection, &reference.projection);
        assert_eq!(actual.removed_directory_paths.len(), 12);
        assert_eq!(actual.removed_node_ids.len(), 12 * 13);
    }

    #[test]
    #[ignore = "synthetic native first-expansion and loaded-subtree pruning benchmark"]
    fn benchmark_explorer_expansion_and_pruning() {
        use std::time::{Duration, Instant};
        use WorkspaceFileKind::{Directory, File};
        let mut seed_session = local_session(sparse_fixture(200, 200));
        let mut root = seed_session.projection.directories[0].clone();
        root.children.push(entry("lazy", Directory, true));
        rebuild_projection(&mut seed_session, root, 1);
        let seed = seed_session.projection;
        let expand = children(
            "lazy",
            (0..200)
                .map(|index| entry(&format!("lazy/file-{index:04}"), File, false))
                .collect(),
        );
        let mut remove_one = seed.directories[0].clone();
        remove_one.children.remove(100);
        let mut remove_many = seed.directories[0].clone();
        remove_many.children = remove_many
            .children
            .into_iter()
            .enumerate()
            .filter(|(index, _)| *index >= 160 || index % 2 == 1)
            .map(|(_, child)| child)
            .collect();
        let mut convert = seed.directories[0].clone();
        convert.children[100] = entry("folder-100", File, false);
        fn median(samples: &mut [Duration]) -> f64 {
            samples.sort();
            samples[samples.len() / 2].as_secs_f64() * 1000.0
        }
        for (label, replacement) in [
            ("expand-200", expand),
            ("remove-one", remove_one),
            ("remove-80", remove_many),
            ("convert", convert),
        ] {
            let mut reference = local_session(seed.clone());
            let expected = rebuild_projection(&mut reference, replacement.clone(), 1);
            let mut old = Vec::new();
            let mut new = Vec::new();
            for sample in 0..7 {
                for optimized in if sample % 2 == 0 {
                    [false, true]
                } else {
                    [true, false]
                } {
                    let mut session = local_session(seed.clone());
                    session.direct_cache = true;
                    let update = replacement.clone();
                    let start = Instant::now();
                    let actual = if optimized {
                        apply_expansion_or_prune_test(&mut session, &update, 1)
                            .expect("eligible fixture")
                    } else {
                        rebuild_projection(&mut session, update, 1)
                    };
                    let elapsed = start.elapsed();
                    assert_delta_equal(&actual, &expected);
                    assert_projection_equal(&session.projection, &reference.projection);
                    if optimized {
                        new.push(elapsed);
                    } else {
                        old.push(elapsed);
                    }
                }
            }
            let old = median(&mut old);
            let new = median(&mut new);
            println!("Classic expansion/pruning shape={label} before_nodes={} changed_nodes={} removed_nodes={} old_ms={old:.3} new_ms={new:.3} speedup={:.2}x", seed.nodes.len(), expected.changed_nodes.len(), expected.removed_node_ids.len(), old/new);
        }
    }

    fn check_owned_rebuild_sequence(
        seed: WorkspaceExplorerTreeProjection,
        updates: Vec<WorkspaceExplorerDirectoryChildren>,
    ) {
        let mut reference = local_session(seed.clone());
        let mut owned = local_session(seed);
        for (index, replacement) in updates.into_iter().enumerate() {
            let version = index as u64 + 1;
            let expected = rebuild_reference(&mut reference, replacement.clone(), version);
            let actual = rebuild_projection(&mut owned, replacement, version);
            assert_delta_equal(&actual, &expected);
            assert_projection_equal(&owned.projection, &reference.projection);
            assert_eq!(owned.version, version);
        }
    }

    #[test]
    fn owned_rebuild_preserves_bootstrap_lazy_kind_and_late_update_deltas() {
        use WorkspaceFileKind::{Directory, File};
        let seed = explorer_tree::project_tree("alera".into(), "/repo".into(), vec![], None);
        check_owned_rebuild_sequence(
            seed,
            vec![
                children("", vec![]),
                children(
                    "/",
                    vec![
                        entry("src", Directory, true),
                        entry("src-old", Directory, true),
                    ],
                ),
                children(
                    "src",
                    vec![
                        entry("src/a", File, false),
                        entry("src/sub", Directory, true),
                    ],
                ),
                children("src/sub", vec![entry("src/sub/deep", File, false)]),
                children("src-old", vec![entry("src-old/keep", File, false)]),
                children(
                    "src/",
                    vec![
                        entry("src/sub", Directory, true),
                        entry("src/a", File, false),
                    ],
                ),
                children(
                    "src",
                    vec![entry("src/sub", File, false), entry("src/a", File, false)],
                ),
                children("src/sub", vec![entry("src/sub/late", File, false)]),
                children("", vec![entry("src-old", Directory, true)]),
                children("src", vec![entry("src/late", File, false)]),
                children(
                    "",
                    vec![
                        entry("src-old", Directory, true),
                        entry("src", Directory, true),
                    ],
                ),
                children("src", vec![entry("src/new", File, false)]),
                children("/src/", vec![entry("src/new", File, false)]),
            ],
        );
    }

    #[test]
    fn owned_rebuild_tracks_cross_owner_pruning_and_reuses_untouched_buffers() {
        use WorkspaceFileKind::{Directory, File};
        // A stateless noncanonical listing can reference another owner's
        // entries. Removing a subtree must report every retained row edited
        // by pruning, not just the replacement row.
        let seed = explorer_tree::project_tree(
            "alera".into(),
            "/repo".into(),
            vec![
                children(
                    "",
                    vec![
                        entry("a", Directory, true),
                        entry("keep", Directory, true),
                        entry("untouched", Directory, true),
                    ],
                ),
                children("a", vec![entry("a/sub", Directory, true)]),
                children("a/sub", vec![entry("a/sub/shared", File, false)]),
                children(
                    "keep",
                    vec![
                        entry("a/sub/shared", File, false),
                        entry("keep/file", File, false),
                    ],
                ),
                children("untouched", vec![entry("untouched/file", File, false)]),
            ],
            None,
        );
        let mut owned = local_session(seed.clone());
        let mut reference = local_session(seed);
        let replacement = children("a", vec![entry("a/new", File, false)]);
        let before = owned
            .projection
            .directories
            .iter()
            .find(|row| row.relative_path == "untouched")
            .unwrap();
        let entry_buffer = before.children.as_ptr();
        let path_buffer = before.children[0].relative_path.as_ptr();
        let expected = rebuild_reference(&mut reference, replacement.clone(), 1);
        let actual = rebuild_projection(&mut owned, replacement, 1);
        assert_delta_equal(&actual, &expected);
        assert_projection_equal(&owned.projection, &reference.projection);
        assert_eq!(
            actual
                .changed_directories
                .iter()
                .map(|row| row.relative_path.as_str())
                .collect::<Vec<_>>(),
            ["a", "keep"]
        );
        assert_eq!(actual.removed_directory_paths, ["a/sub"]);
        let after = owned
            .projection
            .directories
            .iter()
            .find(|row| row.relative_path == "untouched")
            .unwrap();
        assert_eq!(after.children.as_ptr(), entry_buffer);
        assert_eq!(after.children[0].relative_path.as_ptr(), path_buffer);

        // Rootless and duplicate-path inputs use the original graph fallback.
        let rootless = explorer_tree::project_tree(
            "alera".into(),
            "/repo".into(),
            vec![
                children("parent", vec![entry("parent/sub", Directory, true)]),
                children(
                    "parent/sub",
                    vec![
                        entry("parent/sub/x", File, false),
                        entry("parent/sub/x", File, false),
                    ],
                ),
            ],
            None,
        );
        check_owned_rebuild_sequence(
            rootless,
            vec![
                children("parent/sub", vec![entry("parent/sub/y", File, false)]),
                children("", vec![entry("parent", Directory, true)]),
                children("parent", vec![entry("parent/sub", Directory, true)]),
            ],
        );
    }

    #[test]
    #[ignore = "synthetic native owned-cache full Explorer rebuild benchmark"]
    fn benchmark_owned_explorer_cache_rebuild() {
        use std::time::{Duration, Instant};
        use WorkspaceFileKind::Directory;
        let seed = sparse_fixture(200, 200);
        let mut mixed = seed
            .directories
            .iter()
            .find(|row| row.relative_path == "folder-100")
            .unwrap()
            .clone();
        mixed
            .children
            .push(entry("folder-100/lazy", Directory, true));
        let mut removed = seed
            .directories
            .iter()
            .find(|row| row.relative_path.is_empty())
            .unwrap()
            .clone();
        removed.children.drain(..80);
        let mut reordered = seed
            .directories
            .iter()
            .find(|row| row.relative_path.is_empty())
            .unwrap()
            .clone();
        reordered.children.reverse();
        fn median(samples: &mut [Duration]) -> f64 {
            samples.sort();
            samples[samples.len() / 2].as_secs_f64() * 1000.0
        }
        for (label, replacement) in [
            ("mixed", mixed),
            ("remove-80", removed),
            ("reorder", reordered),
        ] {
            let mut reference = local_session(seed.clone());
            let expected = rebuild_reference(&mut reference, replacement.clone(), 1);
            let mut old = Vec::new();
            let mut new = Vec::new();
            for sample in 0..7 {
                for optimized in if sample % 2 == 0 {
                    [false, true]
                } else {
                    [true, false]
                } {
                    let mut session = local_session(seed.clone());
                    let update = replacement.clone();
                    let start = Instant::now();
                    let delta = if optimized {
                        rebuild_projection(&mut session, update, 1)
                    } else {
                        rebuild_reference(&mut session, update, 1)
                    };
                    let elapsed = start.elapsed();
                    assert_delta_equal(&delta, &expected);
                    assert_projection_equal(&session.projection, &reference.projection);
                    if optimized {
                        new.push(elapsed);
                    } else {
                        old.push(elapsed);
                    }
                }
            }
            let old = median(&mut old);
            let new = median(&mut new);
            println!("Classic owned cache rebuild shape={label} before_nodes={} changed_dirs={} removed_dirs={} old_ms={old:.3} new_ms={new:.3} speedup={:.2}x", seed.nodes.len(), expected.changed_directories.len(), expected.removed_directory_paths.len(), old/new);
        }
    }

    fn apply_file_only(
        session: &mut ExplorerProjectionSession,
        replacement: &WorkspaceExplorerDirectoryChildren,
        sparse: bool,
    ) -> WorkspaceExplorerProjectionDelta {
        let path = replacement.relative_path.as_str();
        let directory_index = session
            .projection
            .directories
            .binary_search_by(|dir| dir.relative_path.as_str().cmp(path))
            .unwrap();
        let version = session.version + 1;
        try_update_file_only_children(session, directory_index, path, replacement, version, sparse)
            .expect("fixture uses file-only update")
    }

    #[test]
    fn sparse_and_bulk_file_edits_match_legacy_and_full_projection() {
        let seed = sparse_fixture(6, 16);
        let mut old = local_session(seed.clone());
        let mut new = local_session(seed.clone());
        let mut full = seed;
        let path = "folder-003";
        let original = full
            .directories
            .iter()
            .find(|dir| dir.relative_path == path)
            .unwrap()
            .children
            .clone();
        for (round, removed_count) in [1usize, 4, 5, 0, 8, 16, 1].into_iter().enumerate() {
            let mut rows = original
                .iter()
                .skip(removed_count)
                .cloned()
                .collect::<Vec<_>>();
            for index in 0..removed_count {
                rows.push(entry(
                    &format!("{path}/rename-{round}-{index}.rs"),
                    WorkspaceFileKind::File,
                    false,
                ));
            }
            if round % 2 == 0 {
                rows.reverse();
            }
            if let Some(first) = rows.first_mut() {
                first.size = round as u64;
                first.git_status = Some(WorkspaceFileGitStatus::Modified);
            }
            let replacement = children(path, rows);
            let expected = explorer_tree::project_tree(
                "alera".into(),
                "/repo".into(),
                full.directories.clone(),
                Some(replacement.clone()),
            );
            let expected_delta = diff_projections(&full, &expected, round as u64 + 1);
            let old_delta = apply_file_only(&mut old, &replacement, false);
            let new_delta = apply_file_only(&mut new, &replacement, true);
            assert_delta_equal(&old_delta, &expected_delta);
            assert_delta_equal(&new_delta, &expected_delta);
            assert_projection_equal(&old.projection, &expected);
            assert_projection_equal(&new.projection, &expected);
            full = expected;
        }
    }

    #[test]
    #[ignore = "synthetic native sparse structural update benchmark"]
    fn benchmark_sparse_explorer_structural_edits() {
        use std::time::{Duration, Instant};
        let seed = sparse_fixture(200, 200);
        let replacements: Vec<_> = seed
            .directories
            .iter()
            .filter(|dir| !dir.relative_path.is_empty())
            .take(30)
            .map(|dir| {
                let mut rows = dir.children.clone();
                rows.remove(0);
                rows.push(entry(
                    &format!("{}/renamed.rs", dir.relative_path),
                    WorkspaceFileKind::File,
                    false,
                ));
                children(&dir.relative_path, rows)
            })
            .collect();
        let mut expected = local_session(seed.clone());
        let mut warm = local_session(seed.clone());
        for replacement in &replacements {
            let old = apply_file_only(&mut expected, replacement, false);
            let new = apply_file_only(&mut warm, replacement, true);
            assert_delta_equal(&new, &old);
            assert_projection_equal(&warm.projection, &expected.projection);
        }
        let mut old = Vec::new();
        let mut new = Vec::new();
        for sample in 0..7 {
            for sparse in if sample % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            } {
                let mut session = local_session(seed.clone());
                let start = Instant::now();
                for replacement in &replacements {
                    let delta = apply_file_only(&mut session, replacement, sparse);
                    assert_eq!(delta.changed_nodes.len(), 2);
                    assert_eq!(delta.removed_node_ids.len(), 1);
                }
                let elapsed = start.elapsed();
                assert_projection_equal(&session.projection, &expected.projection);
                if sparse {
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
        println!("Classic native sparse update folders=200 nodes={} bindings={} renames={} old_ms={old:.3} new_ms={new:.3} speedup={:.2}x", seed.nodes.len(), seed.entry_bindings.len(), replacements.len(), old/new);
    }

    fn mixed_root_replacement(
        seed: &WorkspaceExplorerTreeProjection,
    ) -> WorkspaceExplorerDirectoryChildren {
        let mut rows = seed
            .directories
            .iter()
            .find(|dir| dir.relative_path.is_empty())
            .unwrap()
            .children
            .clone();
        rows.retain(|row| row.relative_path != "folder-002");
        let converted = rows
            .iter_mut()
            .find(|row| row.relative_path == "folder-004")
            .unwrap();
        *converted = entry("folder-004", WorkspaceFileKind::File, false);
        rows.push(entry("new-dir", WorkspaceFileKind::Directory, true));
        rows.reverse();
        children("", rows)
    }

    fn full_with_replacement(
        before: &WorkspaceExplorerTreeProjection,
        replacement: WorkspaceExplorerDirectoryChildren,
    ) -> WorkspaceExplorerTreeProjection {
        explorer_tree::project_tree(
            "alera".into(),
            "/repo".into(),
            before.directories.clone(),
            Some(replacement),
        )
    }

    #[test]
    fn ordered_projection_diff_matches_legacy_for_mixed_changes_and_placeholder_replacement() {
        let initial = sparse_fixture(8, 12);
        let mut current = initial.clone();
        let replacements = [
            mixed_root_replacement(&initial),
            children(
                "new-dir",
                vec![
                    entry("new-dir/.alera-placeholder", WorkspaceFileKind::File, false),
                    entry("new-dir/nested", WorkspaceFileKind::Directory, true),
                ],
            ),
            children(
                "new-dir",
                vec![
                    entry("new-dir/nested", WorkspaceFileKind::Directory, false),
                    entry("new-dir/renamed.rs", WorkspaceFileKind::File, false),
                ],
            ),
            children("folder-003", vec![]),
        ];
        for (round, replacement) in replacements.into_iter().enumerate() {
            let next = full_with_replacement(&current, replacement);
            let version = round as u64 + 1;
            let expected = diff_projections_legacy(&current, &next, version);
            let actual = try_diff_sorted_projections(&current, &next, version)
                .expect("ordinary native snapshots have strictly ordered keys");
            assert_delta_equal(&actual, &expected);
            if round == 0 {
                // Directory-to-file replacement now removes the stale cache
                // as well as an ordinary deleted directory.
                assert_eq!(actual.removed_directory_paths, ["folder-002", "folder-004"]);
            }
            if round == 1 {
                assert!(actual
                    .removed_node_ids
                    .iter()
                    .any(|id| id == "__alera_placeholder__:new-dir"));
                assert!(actual
                    .changed_nodes
                    .iter()
                    .any(|node| node.id == "path:new-dir/.alera-placeholder"));
            }
            current = next;
        }
        let empty = explorer_tree::project_tree("alera".into(), "/repo".into(), vec![], None);
        let loaded = full_with_replacement(&empty, children("", vec![]));
        let expected = diff_projections_legacy(&empty, &loaded, 1);
        let actual = try_diff_sorted_projections(&empty, &loaded, 1).expect("empty root bootstrap");
        assert_delta_equal(&actual, &expected);
        assert_eq!(actual.changed_nodes[0].id, "workspace-root");
    }

    #[test]
    fn ordered_diff_falls_back_for_duplicate_bindings_path_collisions_and_invalid_order() {
        let seed = sparse_fixture(8, 4);
        let lazy = full_with_replacement(&seed, mixed_root_replacement(&seed));
        let real = full_with_replacement(
            &lazy,
            children(
                "new-dir",
                vec![entry(
                    "new-dir/.alera-placeholder",
                    WorkspaceFileKind::File,
                    false,
                )],
            ),
        );
        let mut duplicate = real.clone();
        let mut binding = duplicate.entry_bindings[0].clone();
        binding.relative_path = "different-path".into();
        duplicate.entry_bindings.insert(1, binding);
        let mut collision = real.clone();
        collision.nodes.push(
            lazy.nodes
                .iter()
                .find(|node| node.id == "__alera_placeholder__:new-dir")
                .unwrap()
                .clone(),
        );
        collision
            .nodes
            .sort_by(|a, b| a.virtual_path.cmp(&b.virtual_path));
        let mut unordered = real.clone();
        unordered.nodes.swap(0, 1);
        let mut relocated_id = real.clone();
        relocated_id
            .nodes
            .iter_mut()
            .find(|node| node.id == "path:new-dir")
            .unwrap()
            .virtual_path = "/new-dir-other".into();
        relocated_id
            .nodes
            .sort_by(|a, b| a.virtual_path.cmp(&b.virtual_path));
        for unusual in [duplicate, collision, unordered, relocated_id] {
            for (before, after) in [(&real, &unusual), (&unusual, &real)] {
                assert!(try_diff_sorted_projections(before, after, 9).is_none());
                assert_delta_equal(
                    &diff_projections(before, after, 9),
                    &diff_projections_legacy(before, after, 9),
                );
            }
        }
    }

    #[test]
    #[ignore = "synthetic native mixed-tree projection diff benchmark"]
    fn benchmark_ordered_explorer_projection_diff() {
        use std::hint::black_box;
        use std::time::{Duration, Instant};
        let before = sparse_fixture(200, 200);
        let after = full_with_replacement(&before, mixed_root_replacement(&before));
        let expected = diff_projections_legacy(&before, &after, 1);
        let warm = try_diff_sorted_projections(&before, &after, 1).expect("ordered fixture");
        assert_delta_equal(&warm, &expected);
        let mut old = Vec::new();
        let mut new = Vec::new();
        const REPEATS: usize = 10;
        for sample in 0..7 {
            for optimized in if sample % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            } {
                let mut elapsed = Duration::ZERO;
                for _ in 0..REPEATS {
                    let start = Instant::now();
                    let actual = if optimized {
                        diff_projections(black_box(&before), black_box(&after), 1)
                    } else {
                        diff_projections_legacy(black_box(&before), black_box(&after), 1)
                    };
                    elapsed += start.elapsed();
                    assert_delta_equal(&actual, &expected);
                }
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
        println!("Classic ordered projection diff before_nodes={} after_nodes={} changed_nodes={} removed_nodes={} repeats={} old_ms={old:.3} new_ms={new:.3} speedup={:.2}x", before.nodes.len(), after.nodes.len(), expected.changed_nodes.len(), expected.removed_node_ids.len(), REPEATS, old/new);
    }

    #[test]
    fn live_directory_to_file_removes_cached_children_and_rejects_late_listing() {
        use std::fs;
        let workspace = tempfile::tempdir().unwrap();
        let base = workspace.path();
        fs::create_dir_all(base.join("src/nested")).unwrap();
        fs::write(base.join("src/nested/deep.txt"), "old").unwrap();
        fs::create_dir_all(base.join("src-extra")).unwrap();
        fs::write(base.join("src-extra/keep.txt"), "keep").unwrap();
        let workspace_path = base.to_string_lossy().into_owned();
        let list = |path: &str| {
            children(
                path,
                super::super::list_workspace_children(
                    workspace_path.clone(),
                    path.into(),
                    false,
                    false,
                )
                .unwrap(),
            )
        };
        let root = list("");
        let old_src = list("src");
        let nested = list("src/nested");
        let extra = list("src-extra");
        let handle = start(
            "fixture".into(),
            workspace_path.clone(),
            vec![root, old_src.clone(), nested, extra],
        )
        .unwrap();
        fs::remove_dir_all(base.join("src")).unwrap();
        fs::write(base.join("src"), "replacement").unwrap();
        let changed = update(handle.clone(), list("")).unwrap();
        assert_eq!(changed.removed_directory_paths, ["src", "src/nested"]);
        assert!(changed
            .removed_node_ids
            .contains(&"path:src/nested/deep.txt".into()));
        assert!(changed
            .removed_binding_node_ids
            .contains(&"path:src/nested/deep.txt".into()));
        let file_node = changed
            .changed_nodes
            .iter()
            .find(|node| node.id == "path:src")
            .unwrap();
        assert_eq!(file_node.kind, WorkspaceExplorerTreeNodeKind::File);
        assert!(file_node.child_ids.is_empty());
        assert!(changed
            .removed_node_ids
            .iter()
            .all(|id| !id.starts_with("path:src-extra")));
        let late = update(
            WorkspaceExplorerProjectionHandle {
                id: handle.id,
                version: changed.version,
            },
            old_src,
        )
        .unwrap();
        assert!(late.changed_directories.is_empty());
        assert!(late.changed_nodes.is_empty());
        assert!(late.changed_bindings.is_empty());
        fs::remove_file(base.join("src")).unwrap();
        fs::create_dir_all(base.join("src")).unwrap();
        fs::write(base.join("src/new.txt"), "fresh").unwrap();
        let recreated = update(
            WorkspaceExplorerProjectionHandle {
                id: handle.id,
                version: late.version,
            },
            list(""),
        )
        .unwrap();
        assert!(recreated
            .changed_nodes
            .iter()
            .any(|node| node.id == "__alera_placeholder__:src"));
        let fresh = update(
            WorkspaceExplorerProjectionHandle {
                id: handle.id,
                version: recreated.version,
            },
            list("src"),
        )
        .unwrap();
        assert!(fresh
            .changed_nodes
            .iter()
            .any(|node| node.id == "path:src/new.txt"));
        assert!(fresh
            .changed_nodes
            .iter()
            .all(|node| node.id != "path:src/nested/deep.txt"));
        stop(handle);
    }

    #[test]
    fn first_loaded_empty_root_includes_root_node_for_client_bootstrap() {
        let handle = start("alera".into(), "/repo".into(), vec![]).unwrap();
        let delta = update(handle.clone(), children("", vec![])).unwrap();
        assert_eq!(delta.changed_directories.len(), 1);
        assert!(delta
            .changed_nodes
            .iter()
            .any(|node| node.id == "workspace-root"));
        stop(handle);
    }

    #[test]
    fn update_one_of_many_directories_returns_only_touched_nodes() {
        let dir_count = 300;
        let mut directories = Vec::with_capacity(dir_count + 1);
        directories.push(children(
            "",
            (0..dir_count)
                .map(|index| entry(&format!("dir{index}"), WorkspaceFileKind::Directory, true))
                .collect(),
        ));
        for index in 0..dir_count {
            directories.push(children(
                &format!("dir{index}"),
                vec![entry(
                    &format!("dir{index}/unchanged.txt"),
                    WorkspaceFileKind::File,
                    false,
                )],
            ));
        }
        let full =
            explorer_tree::project_tree("alera".into(), "/repo".into(), directories.clone(), None);
        let handle = start("alera".into(), "/repo".into(), directories).unwrap();
        let delta = update(
            handle.clone(),
            children(
                "dir123",
                vec![
                    entry("dir123/unchanged.txt", WorkspaceFileKind::File, false),
                    entry("dir123/added.txt", WorkspaceFileKind::File, false),
                ],
            ),
        )
        .unwrap();
        assert!(full.nodes.len() > 600);
        assert_eq!(delta.changed_directories.len(), 1);
        assert_eq!(delta.changed_directories[0].relative_path, "dir123");
        assert!(delta.changed_nodes.len() < 10);
        assert_eq!(delta.changed_bindings.len(), 1);
        assert!(delta
            .changed_bindings
            .iter()
            .any(|binding| binding.relative_path == "dir123/added.txt"));
        assert!(delta.removed_node_ids.is_empty());
        stop(handle);
    }

    #[test]
    fn delta_preserves_native_full_projection_for_growth_and_pruning() {
        let root = children("", vec![entry("src", WorkspaceFileKind::Directory, true)]);
        let root_view =
            explorer_tree::project_tree("alera".into(), "/repo".into(), vec![root.clone()], None);
        let handle = start("alera".into(), "/repo".into(), vec![root]).unwrap();
        let changed = children(
            "src",
            vec![entry("src/main.dart", WorkspaceFileKind::File, false)],
        );
        let first = update(handle.clone(), changed.clone()).unwrap();
        assert_eq!(first.version, 1);
        assert_eq!(first.changed_directories.len(), 1);
        assert!(first
            .changed_nodes
            .iter()
            .any(|node| node.id == "path:src/main.dart"));
        assert!(first
            .removed_node_ids
            .iter()
            .any(|id| id == "__alera_placeholder__:src"));
        let baseline = explorer_tree::project_tree(
            "alera".into(),
            "/repo".into(),
            root_view.directories,
            Some(changed),
        );
        let changed_root = children("", vec![]);
        let second = update(
            WorkspaceExplorerProjectionHandle {
                id: handle.id,
                version: 1,
            },
            changed_root,
        )
        .unwrap();
        assert_eq!(second.version, 2);
        assert_eq!(second.removed_directory_paths, vec!["src"]);
        assert!(second
            .removed_node_ids
            .contains(&"path:src/main.dart".to_string()));
        assert!(second.removed_node_ids.contains(&"path:src".to_string()));
        let final_projection = explorer_tree::project_tree(
            "alera".into(),
            "/repo".into(),
            baseline.directories,
            Some(children("", vec![])),
        );
        assert_eq!(final_projection.nodes.len(), 1);
        stop(handle);
    }

    #[test]
    fn session_detects_stale_version_and_returns_compact_metadata_delta() {
        let root = children("", vec![entry("readme.md", WorkspaceFileKind::File, false)]);
        let handle = start("alera".into(), "/repo".into(), vec![root.clone()]).unwrap();
        let no_change = update(handle.clone(), root.clone()).unwrap();
        assert_eq!(no_change.version, 1);
        assert!(no_change.changed_nodes.is_empty());
        assert!(no_change.changed_directories.is_empty());
        let stale = update(handle.clone(), root.clone());
        assert!(matches!(
            stale,
            Err(WorkspaceFileError {
                kind: WorkspaceFileErrorKind::Conflict,
                ..
            })
        ));
        let mut changed = root;
        changed.children[0].git_status = Some(WorkspaceFileGitStatus::Modified);
        let metadata = update(
            WorkspaceExplorerProjectionHandle {
                id: handle.id,
                version: 1,
            },
            changed,
        )
        .unwrap();
        assert_eq!(metadata.changed_directories.len(), 1);
        assert!(metadata.changed_nodes.is_empty());
        stop(handle.clone());
        assert!(matches!(
            update(
                WorkspaceExplorerProjectionHandle {
                    id: handle.id,
                    version: 2
                },
                children("", vec![])
            ),
            Err(WorkspaceFileError {
                kind: WorkspaceFileErrorKind::NotFound,
                ..
            })
        ));
    }
}
