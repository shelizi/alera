# Explorer structural file-only Rust Delta fast path — 2026-10-20

## Problem and scope

The retained Rust Explorer snapshot now avoids whole-tree rebuilds for pure
metadata updates, but a batch of file additions, removals or reorderings inside
one loaded folder still used project_tree on every cached directory, followed
by full-projector comparison of every native node and entry binding.

This batch adds a **conservative file-only structural fast path** within the
existing versioned Explorer Session. It changes no FRB API or Dart code.

## Correctness and fallback

The optimization runs only if both previous and replacement children are
uniquely named *direct* ordinary files and the stored parent child-ID
sequence matches the old directory listing. Any directories, lazy placeholders,
symlink/other file kinds, implicit parents, malformed paths or unexpected
parent node layout automatically use the unchanged full project_tree routine.

For eligible updates, Rust changes only the loaded directory's metadata,
its parent childIds, added/removed file nodes and their binding entries.
All version numbers, new/removed node IDs, binding lists and the returned
directory Delta retain the exact reference projector behavior and ordering.
The parent node changes when a child is added, removed or reordered.

Golden tests compare all six Delta fields and the full retained projection
(directories, nodes and bindings) to project_tree and diff_projections
after every step of seven 125-file bulk add/remove/reorder rounds, both
at workspace root and in a nested loaded directory. Existing mixed
directory/placeholder/pruning and version-conflict tests remain intact.

## Reproducible Release Rust CPU benchmark

    cargo test --release --manifest-path rust/Cargo.toml profile_explorer_structural_add_remove --lib -- --ignored --nocapture

Each run initializes 100, 300 or 1,000 loaded child directories and changes
one directory by adding/removing 150 normal files. Four warmup iterations
followed by seven additions/seven removals, report median Rust native
Session update time. No disk I/O, FRB marshalling, Flutter tree conversion,
painting or Windows Release UI frame timings are included.

| Loaded child directories | Before add (us) | After add (us) | Before remove (us) | After remove (us) |
| --- | ---: | ---: | ---: | ---: |
| 100 | 678.6 | 178.1 | 624.7 | 73.6 |
| 300 | 1595.0 | 174.2 | 1679.0 | 84.8 |
| 1000 | 5712.2 | 189.5 | 6488.1 | 119.3 |

Old/new were different Release runs on the same Windows host, not a paired
single-process A/B. Relative speedups are synthetic and should not be
extrapolated to FPS. Structural changes with folders still take the
authoritative full projection route.

## Tests

- 276 passed, 0 failed, 4 ignored in the complete Rust library suite.
- 64 passed, 0 failed in full Explorer Flutter widget suite.
- No schema change and no need to regenerate FRB bindings.

## Follow-up

Even with this native optimization, Flutter's structural Delta merge rebuilds
maps from the existing projection and materializes a new Dart TreeNode for
every node during _buildTreeData. Profile/reuse existing Dart TreeNode
objects for unchanged nodes if that remains a significant portion of UI
build/paint time. The current work is a backend CPU optimization, not an
end-to-end measured Windows desktop Release frame improvement.