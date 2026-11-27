use std::collections::HashMap;
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
    let projection = explorer_tree::project_tree(
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
    }

    let updated = explorer_tree::project_tree(
        current.workspace_name.clone(),
        current.workspace_path.clone(),
        current.projection.directories.clone(),
        Some(replacement),
    );
    let next_version = current.version + 1;
    let delta = diff_projections(&current.projection, &updated, next_version);
    current.version = next_version;
    current.projection = updated;
    Ok(delta)
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
        !name.is_empty() && !name.contains('/') && seen.insert(entry.relative_path.as_str())
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

pub(super) fn stop(handle: WorkspaceExplorerProjectionHandle) {
    if let Ok(mut registry) = sessions().lock() {
        registry.remove(&handle.id);
    }
}

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

fn try_diff_sorted_projections(
    before: &WorkspaceExplorerTreeProjection,
    after: &WorkspaceExplorerTreeProjection,
    version: u64,
) -> Option<WorkspaceExplorerProjectionDelta> {
    // Use noncapturing functions for the borrowed keys to preserve the
    // input lifetime across the generic comparison helper.
    fn directory_path(directory: &WorkspaceExplorerDirectoryChildren) -> &str {
        &directory.relative_path
    }
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
        if !strictly_ordered(&projection.directories, directory_path)
            || !strictly_ordered(&projection.nodes, node_path)
            || !strictly_ordered(&projection.entry_bindings, binding_id)
            || !projection.nodes.iter().all(node_path_identifies_id)
        {
            return None;
        }
    }
    let (changed_directories, removed_directory_paths) = diff_ordered(
        &before.directories,
        &after.directories,
        directory_path,
        directory_path,
    );
    let (mut changed_nodes, removed_node_ids) =
        diff_ordered(&before.nodes, &after.nodes, node_path, node_id);
    let (changed_bindings, removed_binding_node_ids) = diff_ordered(
        &before.entry_bindings,
        &after.entry_bindings,
        binding_id,
        binding_id,
    );
    // Match legacy first-loaded-empty-root bootstrap behavior.
    if before.directories.is_empty() && !after.directories.is_empty() {
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
        changed_directories,
        removed_directory_paths,
        changed_nodes,
        removed_node_ids,
        changed_bindings,
        removed_binding_node_ids,
    })
}

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
            (node.id == "workspace-root"
                && before.directories.is_empty()
                && !after.directories.is_empty())
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
        changed_directories,
        removed_directory_paths,
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
