# Flutter Explorer metadata-only Delta index patch — 2026-10-20

## Problem

After the native Rust metadata-only update fast path, Flutter still accepted a
Delta by materializing complete HashMaps of all loaded directories, tree nodes
and node-to-file bindings, sorting them, rebuilding a full projected tree
snapshot, and clearing/rebuilding three independent Dart caches. Git status,
file size, mtime, hidden flag, and content-token updates almost always change
only WorkspaceFileEntry metadata, not tree node identity or structure.

## Change

The Explorer Delta application now distinguishes metadata-only updates:
- When there is an existing projection and no changed/removed nodes or entry
  bindings and no removed directories, retain its exact nodes and binding
  list by reference; do not allocate their complete maps or sort them.
- Replace only the changed loaded-directory records in a new ordered
  directory list. No resort is needed for existing relative paths.
- Update affected directory-to-children, relative-path-to-entry and
  path-node-ID-to-entry maps only for those directories. The path-node-ID
  convention is the existing native builder format: path:<relativePath>.
- Empty/no-change Deltas leave the existing projection and indexes untouched.
- Any structural change, expired initial projection or removed directory uses
  the old full Delta merge and _applyProjection indexing path.

This change DOES NOT skip the upstream Flutter DirectoryTreeController tree
rebuild requested by directory refreshers. UI tree rebuild and painting
remain end-to-end profile targets, as does memory peak under very large
workspace workloads.

## Verification

- New Explorer widget regression creates a loaded folder with two files,
  updates only one file's Git status (normal -> modified -> normal), confirms
  the displayed M marker changes correctly and the other file remains
  visible, and confirms no stateless projection fallback.
- Full Explorer widget suite covers watcher coalescing, directory listing,
  modified node graph, expanded sibling subtrees, stale native versions,
  workspace switches, file actions, and selection.
- The separate Rust fast path includes 275 passing native library tests and
  a Release CPU benchmark for 100/300/1000 loaded directories.

No numerical claim for Dart or full Windows Desktop UI speedup is made by
this patch because no comparable Flutter Release AOT end-to-end trace or
heap-allocation profile has been collected.
