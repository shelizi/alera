# Alera GPUI parallel migration integration — 2026-11-07

## Scope and isolation

- Integration branch: `integrate/gpui-parallel-20261107`; independent worktree `.worktrees/integrate-gpui-parallel-20261107`.
- Base: `cbed615abedea6d02174dfbd93fa24d65166fc40`.
- Preserve the concurrent dirty `feat/gpui-desktop-20261107`, official `main`, Flutter/mobile, production CLI/Runtime Host/PTY ownership and user files.
- No merge into official branches, push, tag or release in this operation. Experimental, unsupported GPUI frontend only.

## Parallel feature slices

1. Explorer tree: `5918566d8` — bounded lazy, read-only nested tree, expand/collapse, reveal, symlink/path traversal restrictions; optional native GPUI Tree mode.
2. Editor Find/Replace: source `5896742e0`, integration `2fbf6e55c` — Unicode grapheme-safe literal matching, wrap navigation, case modes, replace-all through existing undo/safety; model only, no search bar or keyboard shortcuts yet.
3. Project content search: source `5ee6a3b5a`, integration `61978c4c1` — read-only and bounded project text search, match line/Unicode column/snippet, skips generated directories, binary/oversized files and symlinks; model only, not connected to GUI yet.
4. Scratch recovery: source `9a9125aa7`, integration `91a7f0955` — persistent separate unsaved scratch identities, local plaintext state, restore per workspace, discard and Save As cleanup, fail-closed handling.
5. Integration-only native wiring: restore scratch tabs at application startup and after switching to another workspace; preserve malformed local recovery files and surface a warning instead of overwriting them.

## Verification

- Windows `cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --lib --locked --offline`: **106 passed; 0 failed; 0 ignored** on integrated changes, using `D:\alera-build\alera-gpui-model-target`.
- Windows `cargo check --manifest-path desktop_gpui/Cargo.toml --all-targets --locked --offline`: **PASS** on integrated changes, using `D:\alera-build\alera-gpui-target`.
- `workspace_tree::tests`: **2/2 PASS** prior to integration, and included in the 106 combined tests.
- `rustfmt --edition 2021 --config skip_children=true desktop_gpui/src/main.rs` and `git diff --check`: run locally after native startup hook.
- Dependencies report a future-incompatibility notice for `proc-macro-error2 v2.0.1`; the current Rust build check exits successfully.

## Not verified — must not be promoted to PASS

- Real Windows GPUI clicks, text editing and clipboard behavior beyond process-alive smoke: **BLOCKED / NOT RUN**.
- Microsoft Zhuyin IME actual candidate window, multi-DPI, large-file pointer behavior: **NOT RUN**.
- Full native project search UI, editor find/replace UI/highlighting/shortcuts: **NOT IMPLEMENTED**.
- Production PTY interactive lease, Git writer/merge/Agent/MCP parity, macOS/Linux GUI, signed packaging: **NOT IMPLEMENTED / NOT RUN**.
- Native GUI runtime build after this integration, OS-level crash recovery and actual user installation: **NOT RUN**; only `cargo check` and model tests are validated.

## Next implementation

1. Add GPUI editor search field, match highlight and find-next/previous/replace controls using `EditorSearch`; ensure active tab semantics and IME composition guard.
2. Add background content search sidebar using `ProjectSearch` with generation-based stale-response handling; opening matches through canonical Workspace path guard.
3. Run isolated Windows GUI acceptance for restored scratch tabs, Explorer Tree, Save As and Zhuyin before declaring desktop parity; keep release gates closed.
