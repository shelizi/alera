# Flutter Explorer structural Delta: reuse unchanged TreeNode objects

Date: 2026-10-20
Branch: local main

## Bottleneck

Rust's versioned Explorer Delta emits only changed/removed tree nodes and file
bindings. Flutter previously merged the native node collections correctly but
each call to _buildTreeData reconstructed a Dart TreeNode for every projected
node, even when an unchanged file or sibling subtree was not part of the Delta.

## Change

- Retain an ID -> Dart TreeNode cache alongside the current native projection.
- For a structural native Delta, remove only its removedNodeIds and convert
  only delta.changedNodes. Other immutable Dart TreeNode objects are reused.
- _buildTreeData still produces a fresh immutable TreeData map for structural
  changes (the DirectoryTreeController must flatten/reconcile structural
  node changes) but its unchanged objects are shared across snapshots.
- On first projection, full stateless fallback, or workspace reset, seed or
  clear the cache from the authoritative full native projection.
- Metadata-only Deltas continue using the previously verified path that
  keeps TreeData identity and repaints only mounted rows.

## TDD and verification

The structural native Delta Widget test initially failed after asserting
referential identity for path:src/main.dart across a sibling file addition.
After this optimization it passes, while path:src is a newly created node
because its childIds changed. A separate regression confirms that updating
one expanded directory preserves the Dart TreeNode for an unchanged sibling
and removes deleted node IDs from the new TreeData.
A 150-file batch add/remove widget test additionally verifies exact node
counts, deleted-node pruning, identity reuse for persistent files, and no
stateless native projection fallback.

- Full Flutter Explorer widget suite: 65 passed, zero failures.
- Rust native full suite (previous independent batch): 276 passed, zero
  failures, four ignored; file-only 150-file structural benchmark verified.
- No FRB API or generated binding changes.

## Measurement scope and limitations

Node allocation avoidance is established by object-identity regressions,
not by a measured UI memory/latency ratio. Flutter still copies a map to
create a new immutable TreeData, and DirectoryTreeController.rebuild still
recomputes the flattened hierarchy when the structure actually changes.

Native Release CPU benchmarks from the previous batch isolate backend
updates only. End-to-end Windows Release AOT frame p50/p95 and allocation
peaks have NOT been measured for this Flutter change. Such a benchmark
requires an isolated running desktop fixture with frame timings, separate
from production user settings.

## Reproduce

    flutter test --no-pub test/widget/workspace_explorer_test.dart

Future profiling: quantify TreeData map copies and visible-node flattening
with 100/300/1000 directories, then consider structural patches to the
controller only if measurable Release UI frame regressions remain.