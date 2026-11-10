# GPUI native Terminal creation + tab layout — 2026-11-08

Base: `feat/gpui-desktop-20261107` at `6e730623e645478839507125396a0775a823959d`. All changes are isolated to experimental Alera GPUI branches, never official dirty `main`.

## New Terminal through the existing Runtime Host

- `alera terminal --no-start --json workspaces`: uses authenticated Host RPC `workspace.listAll`, normalizes output to `{"items":[...]}`. The Host must already be running; no direct SQLite/token reads.
- `alera terminal --no-start --json create --workspace-id <id>`: requires explicit bounded Host Workspace ID and calls authenticated `terminal.create` RPC. Both new CLI subcommands fail closed without `--no-start`.
- The GPUI `+ New Terminal` action discovers the existing CLI, fetches registered Host workspaces, canonicalizes their paths, and requires **exactly one** to match the active GPUI project root. Unregistered or ambiguous projects are refused; no implicit workspace registration or arbitrary shell process.
- After `terminal.create`, the UI saves `{sessionId,workspaceId,tabId}` as a *pending* identity and lists existing Host sessions. It opens the newly created session **only when the authenticated list confirms all three IDs**. Existing `+ Attach existing` remains separate.
- `terminal_json_io` allowlists only the exact create-argument shape, no stdin; the standard CLI invocation always includes `--no-start`. Terminal Host still owns PTY creation, authentication, process lifetime and termination.

## Tab-layout recovery

- A separate local `terminal-layout-<workspace-hash>.json` under GPUI's existing state directory stores **at most eight** bounded `{handle,workspace_id,tab_id}` identities, selected handle and screen/transcript preference.
- It never stores PTY output, input history, credentials, tokens or commands. State files use the existing bounded 64 KiB atomic writes and validate canonical root, version, duplicate handles and active selection.
- Explicit tab activation, next/previous, reorder, close, close others/right and view-mode changes save the layout. Workspace switching saves the departing layout and loads the destination layout.
- After restart, recovery waits for an authenticated Host session list and requires the stored handle, tab ID **and** workspace ID to match. Stale or reused handles are never automatically attached. Waiting for the first Host response does not overwrite a saved layout with an empty view.

## Validation and unsupported boundary

Scope: unit/model tests and Windows native compile only; no actual GUI/IME/PTY E2E test, Host account changes, production push/tag/release.

Completed static validation: GPUI `cargo check --all-targets --locked --offline` passed; GPUI headless `cargo test --no-default-features --lib --locked --offline` **152 passed, 0 failed** (includes Terminal state validation and session layout tests); Alera CLI `cargo check --bin alera --locked --offline` passed on Windows. Scoped `rustfmt --check` and `git diff --check` passed. No live Host RPC requests were made as part of validation.

Isolated source commits: CLI `6ef660c92`; GPUI bridge `8ffbf07de`; view-only local state `88a3278ad`. The combined experimental integration commit is `c67c0b743`.

Real `terminal create` still depends on the matching new `alera` CLI being installed at the location discovered by GPUI and on the Host already having the workspace registered. Incompatible or missing versions fail with an error; no alternate process-start fallback.

Important limitation: this restores the *preview tabs* only, not full terminal buffer history; the Runtime Host retains the real scrollback and process lifetime. Newly created terminals are Host-owned, and closing GPUI tabs only detaches the preview.
