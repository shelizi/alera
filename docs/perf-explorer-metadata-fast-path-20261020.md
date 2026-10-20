# Explorer Rust metadata-only update fast path - 2026-10-20

## Scope

After the versioned Rust Snapshot/Delta migration, each meaningful directory
update previously cloned every cached directory, projected every tree node and
binding, compared full before/after trees, and updated the session. Even an
ordinary Git status or mtime change took work proportional to the entire
workspace's loaded file/directory count.

This batch changes only the existing Rust Explorer projection session
implementation. There is no FRB API/schema change and no generated Dart
changes.

## Fast path

ExplorerProjectionSession::update checks whether the replacement refers to an
already loaded directory (normalized path) and keeps the exact ordered sequence
of children with identical relative paths, names, file kinds and has-children
hints.

If so, the structure and binding graph is provably unchanged:
- Change only that directory's cached metadata entries.
- Return a Delta containing that one directory when metadata changed, or
  an empty Delta for an identical replacement.
- Bump the version exactly once, preserving conflict/replay behavior.
- Return no added/removed nodes or entry bindings, avoiding full-tree rebuild,
  old/new HashMap comparison, and large allocations.

If any structure-affecting field changed, the root has not yet been loaded, or
the normalized directory path is unknown, preserve the previous full native
projection and Delta comparison behavior. All other metadata is passed back
to Flutter via changedDirectories for UI updates.

## Exact CPU benchmark

Run from the repository root:

    cargo test --release --manifest-path rust/Cargo.toml profile_explorer_delta_native_cpu --lib -- --ignored --nocapture

Fixture: 100, 300, 1000 loaded child directories plus root, approximately
2*N+2 projected nodes. On each iteration update one directory with the same
two file paths and kinds but a changed file size, after a warmup structural
addition. Nine timed post-warmup iterations, medians in microseconds. The
benchmark also asserts returned Delta fields against the full Rust projector.

| Loaded directories | Before session median (us) | Fast-path median (us) |
| --- | ---: | ---: |
| 100 | 399.1 | 0.9 |
| 300 | 1,282.7 | 1.4 |
| 1,000 | 5,956.4 | 2.5 |

Whole-tree rebuild/Delta scans still cost approximately 0.3/0.05 ms for
100 directories and 4.3/0.6 ms for 1000 directories in the optimized build's
diagnostic benchmark. The optimized metadata path bypasses both.

The old/new benchmarks are separate Release test invocations on the same
host, each with 9 post-warmup runs. Ratios should not be treated as statistical
latency promises. The optimized path is near timer resolution for tiny
fixtures. CPU microbenchmarks exclude FRB marshalling, Dart projection index
work, Flutter widget/tree rebuild, disk I/O, Git refresh and complete Windows
desktop UI frame latency.

## Correctness

- Golden parity for original full projector versus metadata edits (Git status,
  modified time, file size, content token, hidden, ignored, symlink, protected).
- Structural correctness for child reordering, directory placeholder hint
  changes and child removal.
- Existing version mismatch, root bootstrap, subtree deletion, entry-binding
  changes and 300 loaded directories remain covered by native unit tests.
- Manual benchmark checks all six Delta result fields against the full
  reference on every update. This benchmark remains ignored by default.

## Next profile targets

1. Measure the Flutter _applyNativeProjectionDelta metadata update and
   _rebuildTree calls in the UI isolate; they still rebuild large in-memory
   maps/tree views even when Rust sends changedDirectories only.
2. Profile changed-directory structure updates in a Release desktop build;
   the fallback native projector remains O(all loaded nodes).
3. Only move Flutter tree/entry indexes to selective patches if empirical
   UI frame and heap data justify the extra state complexity.
