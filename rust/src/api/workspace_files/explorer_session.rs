use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use super::{
    explorer_tree, WorkspaceExplorerDirectoryChildren, WorkspaceExplorerEntryBinding,
    WorkspaceExplorerProjectionDelta, WorkspaceExplorerProjectionHandle, WorkspaceExplorerTreeNode,
    WorkspaceExplorerTreeProjection, WorkspaceFileError, WorkspaceFileErrorKind,
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
