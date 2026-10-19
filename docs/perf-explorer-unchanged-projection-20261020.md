# Alera Explorer unchanged-directory native projection fast path

Date: 2026-10-20
Branch: local main

## Existing data path

Explorer directory refresh lists native WorkspaceFileEntry records, then sent
ALL previously loaded directory snapshots plus the replacement directory
through the Flutter/Rust Bridge to projectWorkspaceExplorerTree. Rust rebuilds
the tree and returns the full projection. Dart then regenerates three indexes
(the directory map, path map, and node-ID bindings).

This was performed even for watcher events where a re-listing produced exactly
the same directory entries (often newly allocated Dart objects with equal
metadata).

## Change

Add workspaceExplorerDirectoryChildrenUnchanged(previous, current) and check
it before the projection call. Equality requires the same ordered entries
and **all** WorkspaceFileEntry fields: relativePath, name, kind, size,
modifiedMillis, contentToken, ignored, hidden, symlink, protected,
hasChildrenHint and gitStatus. Null/unknown previous snapshot always
requests a real projection. True metadata changes, removals, additions
and sorting changes retain the old native path.

On a true no-op, the caller still invokes _syncWatchedDirectories to retain
the existing best-effort watcher re-registration retry behavior. It skips
both the full-snapshot FFI request/response and full Dart tree-index rebuild.

This is NOT a stateful Rust native tree snapshot/delta protocol. Native
projectWorkspaceExplorerTree still builds from complete snapshots for
meaningful changes. That larger migration would require a native per-workspace
snapshot lifetime/version API, FFI shape changes and more extensive lifecycle
and concurrency tests.

## Verification

TDD red-first:
- Existing "watched directory updates are skipped when nothing changed"
  watcher test with a fake projection call counter failed, expected 2,
  actual 3.

After optimization:
- Same watcher test passes: unchanged directory results skip an additional
  native projection call even with newly instantiated equal entries.
- New test confirms changed directory results trigger exactly one projection
  and render the new file.
- Comparator test checks equal content, order change, Git status, hidden flag,
  and missing previous snapshot.
- Full Explorer widget suite: 60 passed, 0 failures. This covers multiple
  selection, filtering, watcher storms, mode reloads, stale subtree pruning,
  file actions and native drag paths.

Performance claim: at most one complete native tree projection is skipped per
unchanged loaded-directory refresh. We did not measure end-to-end desktop
frame latency or Rust-side CPU savings numerically.

## Follow-up

Profile changed directory refreshes in very large workspaces. If meaningful
updates remain slow, introduce Rust ownership of a versioned tree snapshot
and return UI-node deltas, without serializing all cached directories each
time. Keep unchanged-event fast path regardless.
