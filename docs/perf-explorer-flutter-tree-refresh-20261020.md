# Explorer Flutter TreeData rebuild avoidance — 2026-10-20

## Prior bottleneck

Even after Rust sent a metadata-only Explorer Delta and Flutter patched only the
affected entry maps, every watched directory refresh called `_rebuildTree()`.
This reconstructed a `tree.TreeNode` for every projected node, a new immutable
`TreeData` map, and forced `DirectoryTreeController.rebuild` to recalculate
flattened/visible rows plus preservation of selection/expansion state.

The `flutter_directory_tree` 1.0.0 widget caches its ListView, so a parent
`setState()` alone does not reliably refresh its itemBuilder's file metadata.

## Change

- `_treeStructureRevision` increments when a full native projection is applied.
- `_refreshDirectory` captures that revision before loading and calls
  `_rebuildTree` only when the revision changed (meaning node graph or bindings
  changed, or a stateless full-projection fallback ran).
- A metadata-only Delta changes entry indexes without replacing tree nodes and
  signals a `ValueNotifier<int>` only when directory metadata actually changes.
- `ValueListenableBuilder` wrapping each instantiated tree row re-reads the
  latest `_entryByNodeId` to repaint Git badges, symlink and protected state.
- An unchanged watcher result performs no tree rebuild and sends no metadata
  notification. Unmounted rows have no active row listener.
- Notifier is disposed with the Explorer widget; no new FRB schema or Rust API.

## Red/green tests

The `native metadata-only Delta refreshes row status and keeps bindings`
widget test was expanded to assert `identical(controller.data, originalTree)`.
It initially failed, confirming a new TreeData was created for Git metadata.

After optimizing, the complete Explorer widget suite passed 64 of 64 cases.
Regressions assert:

- Git modified badge appears and disappears after native metadata-only Delta,
  without changing controller TreeData identity.
- Added file still changes TreeData identity, and correct file appears.
- Unchanged watch events leave TreeData identity untouched.
- In a list of 180 files, a previously scrolled viewport keeps its pixel
  offset (within 0.5 logical pixels), while TreeData stays identical.
- Existing Explorer selection, multi-file operations, expansion, filter,
  native drag, menu actions and stale-session fallback tests continue passing.

## Claims and follow-up

This proves removal of an otherwise unconditional whole-tree Dart construction
and DirectoryTreeController.rebuild for metadata-only/no-op watched refreshes.
Only mounted rows receive metadata notifications; this is less work for large
workspaces, but does not quantify actual FPS or memory savings.

End-to-end Windows Release AOT profiling (frame p50/p95, Dart allocation peak)
under 100/300/1000 loaded directories is still needed to establish practical
desktop latency gains. Structural changes still rebuild the whole TreeData;
optimizing those separately requires measuring their frequency and cost.

## Run

    flutter test --no-pub test/widget/workspace_explorer_test.dart