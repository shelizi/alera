# GPUI Terminal TDD closure — 2026-11-08

Base: `feat/gpui-desktop-20261107` at `e7070735c66b92954d927ee9033ebeb8a92447d1`.
Work was isolated in `test/gpui-terminal-identity-20261108`, `test/gpui-terminal-persistence-20261108`, and `integrate/gpui-terminal-tdd-20261108`. Official dirty `main` was not touched.

## RED before implementation

1. `terminal_list_must_reject_empty_or_control_laden_host_identity`: **failed as expected** because the authenticated Host session list parser accepted an empty `workspaceId` (and only checked ID byte length). Other malformed workspace/tab IDs would also have been accepted.
2. `terminal_layout_rejects_same_host_tab_aliased_under_different_handles`: **failed as expected** because saved layouts rejected duplicate handles but allowed different handles pointing at the same `(workspace_id, tab_id)`.
3. Pure API contract tests were written first for exactly one canonical Host workspace match, all three Host session identity fields, and saved preview tab order; initial test compilation correctly failed because those pure methods did not exist.

## GREEN implementation

- `parse_terminal_list` validates `workspaceId` and `tabId` against the same bounded, nonempty Host ID syntax used by terminal creation.
- `validate_terminal_layout` also rejects duplicate `(workspace_id, tab_id)` pairs, preventing the same Host tab being restored under two handles.
- `select_registered_host_workspace(root, registered)` selects exactly one canonical Host project, refuses relative workspace paths, unrelated/absent paths and ambiguous duplicate matches; no shell/Host creation.
- `CreatedHostTerminal::matches_live_session` checks session handle, Workspace ID and Tab ID as one unit.
- `TerminalDock::restore_previews_from_host_layout` recreates local UI preview tabs in saved order only after a Host list confirms each triple identity. It will not replace tabs the user has already opened or start/terminate any Host PTY.
- `desktop_gpui/src/main.rs` now delegates to these unit-tested model methods instead of duplicating the security-sensitive checks in GPUI callbacks.

## Validation boundary

Only native Rust model tests, source format checks and Windows `cargo check --all-targets --locked --offline`. No live Host requests, real PTY creation, macOS/Windows manual GUI or IME acceptance, push, tag or release.

Verified green results: `terminal::tests` **27/27 passed** on the isolated Host identity lane; `session::tests` **19/19 passed** on the isolated persistence lane; the fully integrated `cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --lib --locked --offline` **158 passed / 0 failed**. Integrated Windows `cargo check --manifest-path desktop_gpui/Cargo.toml --all-targets --locked --offline` passed.

Isolated RED→GREEN source commits: `2b7adf8d8` (Host session IDs and recovery) and `b93df0904` (layout alias validation).

The existing warning about future Rust compatibility in upstream `proc-macro-error2 v2.0.1` was not changed in this focused feature patch.
