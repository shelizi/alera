# Explorer Rust Snapshot / Delta integration — 2026-10-20

## Goal and scope

Reduce Dart <-> Rust Bridge payload sizes when the user refreshes one Explorer directory inside a large workspace, without changing Explorer behavior or losing state during refreshes, workspace switches and missing/stale native session handles.

Before this batch, every meaningful loaded-directory refresh sent the full Dart cache of every loaded directory to Rust via project_workspace_explorer_tree and received a complete native tree with all nodes/bindings. Earlier fast paths already skip identical watched directory metadata.

## Implementation

- A versioned Rust ExplorerProjectionSession registry stores the most recent projection for each mounted Explorer widget. There is a maximum of 64 active sessions, and stop explicitly releases each session.
- start_workspace_explorer_projection_session initially seeds Rust from the Dart directory cache. update_workspace_explorer_projection_session sends one changed directory and expected version, and returns a WorkspaceExplorerProjectionDelta:
  * changed and removed loaded directory records
  * changed and removed tree nodes
  * changed and removed entry bindings
  * new version number
- Rust rejects stale versions instead of silently applying out-of-order updates.
- Flutter serializes updates, checks widget generation and mounted state, merges the versioned Delta into its existing projection, reuses its normal index rebuilding, and releases retained Rust state on reset/dispose.
- If the new API is unavailable, a session is expired/stale, or native delta application fails, Flutter falls back to its existing full stateless Rust projection. It avoids repeated attempts during the same Explorer lifetime and re-enables the native route after projection reset.
- Root bootstrap for an initially empty directory is explicitly included in the first Delta so the client can render empty workspaces.
- FRB codegen updated the root Rust and Dart generated bridge surfaces. A private Git Diff SimilarityScratch accidentally triggered an unintended generated constructor; an explicit local Default implementation (instead of derived Default) prevents that private scratch type from appearing in the public bridge.

## Correctness

Rust:
- explorer_session tests: 4/4 passed, covering empty-root bootstrap, hundreds-of-directories compact delta, subtree removal and placeholder cleanup, stale-version rejection, and Git metadata-only changes.
- complete native library suite: 273 passed, 0 failed, 2 ignored.

Flutter:
- complete Explorer widget suite: 62 passed, including normal watch updates, unchanged metadata no-op, directory growth, sibling subtree preservation, stale native version/full fallback, workspace switches, selection and file management.
- Existing stateless API remains available for fallback and older native binaries.

## Performance measurement and limitations

A deterministic Rust fixture contains 300 loaded child directories, one root directory and over 600 full tree nodes. Changing only directory 123 returns exactly one changed directory, fewer than 10 changed nodes, one new entry binding and zero removed node IDs. This confirms the smaller *logical FFI result* but does not measure serialized payload bytes, AOT memory use or UI p50/p95.

The current Rust update still reconstructs and compares the complete retained tree internally; Flutter still reconstructs a complete in-memory projection and indexes from the compact Delta. The benefit is avoiding repeated whole-tree FFI request/response serialization rather than eliminating all O(total tree) CPU work. Further profiling is needed to justify local per-subtree projection and partial Flutter index updates.

No Windows Desktop Release application was published or pushed by this batch.

## Verification commands

    flutter_rust_bridge_codegen generate --config-file flutter_rust_bridge.yaml
    cargo test --manifest-path rust/Cargo.toml explorer_session --lib
    cargo test --manifest-path rust/Cargo.toml --lib
    flutter test --no-pub test/widget/workspace_explorer_test.dart

## Follow-ups

1. Measure realistic 100/300/1,000 loaded-directory FFI payload bytes, Dart heap peaks, and directory refresh p50/p95 in a Windows Release build.
2. Only if native CPU remains material, update the retained Rust tree incrementally instead of rebuilding all nodes on each directory update.
3. Only if Flutter CPU remains material, replace whole-Dart-projection index reconstruction with selective node/entry map patches.
