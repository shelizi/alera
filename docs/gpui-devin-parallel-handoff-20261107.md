# Alera GPUI: Devin SWE-2 Flash parallel implementation handoff

Date: 2026-11-07. Base: `54fb0dfabd47c90e7d6d379a490600a8a66d5e9c`, a clean experimental GPUI-only integration branch. Product goal is limited to **Explorer file/folder operations, left Projects and Git Worktrees management**. Terminal/basic Editor already exist; avoid unrelated feature creep.

## Global contract for every Devin subagent

1. You are a **coding** subagent, not a reviewer-only assistant. Inspect the named files, make concrete edits, add small regression tests, run per-file `rustfmt` / `git diff --check` where possible, and report changed files and any unresolved risks. Do not end with a plan alone.
2. **Never touch official `main`, Flutter/mobile apps, production Runtime Host, real user worktrees, credentials, or release/publish settings. Never run `git worktree remove` against a real checkout; unit tests must use temporary test repos.**
3. Each agent owns an independent worktree. Touch ONLY your lane's authorized files. **Do not edit `desktop_gpui/src/lib.rs` or `Cargo.toml`**; the modules are already declared. If you need a cross-lane change, document it in your final report instead of editing another lane's file. Do not commit, push, or merge; coordinator will cherry-pick or manually merge reviewed changes.
4. The existing model APIs and GPUI UI have been compiled on Windows, with `118 / 118` Rust headless model tests passing at baseline; do not regress these. Favor minimal, testable fixes over rewrites and avoid introducing dependencies.
5. Avoid running concurrent heavy Cargo builds in multiple worktrees: previous Rust tests blocked on shared Cargo target/cache locks. Prefer `rustfmt --edition 2021 --config skip_children=true --check <authorized files>` and `git diff --check`. The coordinator owns the combined `cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --lib --locked --offline`, `cargo check --manifest-path desktop_gpui/Cargo.toml --all-targets --locked --offline`, and release gate.
6. Output a terse, factual completion report: changed files, test/format result, behavior covered, remaining risks. Implement now; avoid questions.

## Lane A — Explorer correctness and filesystem safety

**Allowed files: `desktop_gpui/src/explorer_ops.rs`, `desktop_gpui/src/model.rs` ONLY.**

Read `ExplorerFiles::{existing, folder, destination, create_file, create_folder, rename, move_into, delete, audit_tree}` and `WorkbenchModel::forget_path_and_children`.

Required work:

- Fix `forget_path_and_children` active tab index bug. Example: tabs [A,B,C], active C (index 2), deleting A must keep C active (now index 1); deleting active B should choose adjacent surviving tab; deleting a directory should remove only previews below it and reset directory only when necessary. Cover these cases with temp-workspace model tests.
- Audit filesystem path constraints: refuse symlinks/junction escapes, `.git` and `.worktrees` at every directory level, workspace root deletion, overwrite, moving a parent into itself, destination outside workspace, Windows reserved names / trailing dots. Close real, testable gaps with minimal changes. Do not implement shell commands or unrestricted recursive deletes.
- Prevent silent overwrite or cross-filesystem unsafe fallback. Existing APIs must preserve the original on any failed operation.
- Add small, deterministic unit tests with `tempfile`, no real user files and no new dependency.
- Do not edit `main.rs`; report any necessary GUI correction to the UI integrator.

## Lane B — Safe Git Worktree manager hardening

**Allowed file: `desktop_gpui/src/worktree_manager.rs` ONLY.**

Read `WorktreeManager::{discover, list, create, remove}`, `validate_branch` and the existing isolated temp Git-repo tests.

Required work:

- Verify correct primary worktree discovery when active GPUI workspace is itself a **linked worktree**. `list --porcelain` entries must be parsed reliably; never misidentify a linked checkout as primary. Bound record count and output bytes.
- Enforce refusal of delete when `target` is primary/current/locked/prunable/dirty/has untracked files, escapes managed `<primary>/.worktrees`, or is not a registered linked checkout. No `--force`, no `git branch -D`, no trashing actual source files outside the Git command. Error messages should be actionable, not opaque.
- Validate branch names more strictly. Avoid folder collisions from `/` -> `--` mapping and path traversal; fail closed on invalid names. No UI changes.
- Protect against hanging Git child processes (bounded timeout if implementable with standard library, no extra deps). Prefer local commands `git -C ...` with args vector and no shell, `GIT_TERMINAL_PROMPT=0`. Do not degrade Windows no-console behavior.
- Expand tests with temporary Git repos: linked worktree discovering/listing, refusal of dirty/locked/unregistered, successful clean create/remove. Keep test runs bounded.

## Lane C — Saved Projects registry and session integrity

**Allowed file: `desktop_gpui/src/session.rs` ONLY.**

Read `PreviewSessionStore::{saved_projects,save_project,unsave_project,recent_workspaces,switch_workspace,save,restore}`.

Required work:

- Ensure pinned Saved Projects are **independent** of the 12-item MRU. Keep up to 32 unique canonical valid roots, stable order newest pin first, discard missing paths without letting poisoned index paths alter files.
- `unsave_project` must ONLY update GPUI-local metadata. It must not delete a project, worktree, editor draft or persisted per-workspace session; add a regression.
- Ensure switching a workspace updates MRU and current hint without silently erasing pinned project list, and that an invalid destination refuses switch without mutating active project state.
- Add deterministic tests for multiple project roots, dedup/cap, symlink/aliased invalid paths, failed switches, and pin/unpin. Avoid unnecessary schema migrations or feature creep.
- Do not edit `main.rs`; report any UI bugs you notice in final output.

## Lane D — Native GPUI UI functionality and consistency

**Allowed file: `desktop_gpui/src/main.rs` ONLY.** This is the ONLY lane permitted to touch `main.rs`.

Existing UI at base `54fb0df`: `begin_explorer_action`, `apply_explorer_action`, `switch_workspace`, `pin_current_project`, `unpin_project`, `refresh_worktrees`, `create_worktree`, `remove_worktree`, render method contains right Explorer actions and left Saved projects/Worktree lists.

Required work:

- Verify and fix UI workflow: selection from tree/list, New File/Folder created in selected folder (or current browsed folder), Rename and Move using source selection, Delete with two-step explicit confirmation, state reset on new selection/workspace switch. Preserve user-visible errors and refresh Explorer tree/index/Git.
- `ExplorerAction::Move` currently uses a workspace-relative string, so make sure empty/traversal/absolute destination is safely rejected by model. Do not allow silent overwrite. If there is a dirty editor, no destructive mutation.
- Avoid resetting **unrelated** clean editor tabs just because a different file was moved/removed. Coordinate with Lane A's `forget_path_and_children` behavior; since only you can edit `main.rs`, make any UI-level fix here without editing `model.rs`.
- Ensure left Saved Projects and Recent Workspaces can open/close/pin/unpin independently and never mutate actual project files on unpin. Add useful tooltips/labels within existing visual layout if needed. Keep center Terminal/Editor unchanged.
- Worktree list/create/switch/remove must be visible in left sidebar and refresh after a successful operation; refuse active worktree deletion, block dirty or locked worktrees, retain branch after worktree removal, require double click confirmation. **No automatic deletion or preview side effects.**
- Fix stale async response hazards: a completion from a previous workspace or switched target may never reset/change current worktree UI incorrectly. Avoid multiple concurrent mutate calls and accidental mismatch when switching workspace.
- Keep source compilable with GPUI 0.2.2, no new crates, and don't touch other lanes' files. You can run `rustfmt --edition 2021 --config skip_children=true desktop_gpui/src/main.rs` and `git diff --check`; coordinator will run full suite.

## Coordinator integration, NOT subagent scope

1. Collect four lane outcomes; review diffs and overlap.
2. Commit/cherry-pick narrow changes from independent worktrees into integration branch.
3. Run combined 118+ headless model tests, Windows `cargo check --all-targets`, `cargo fmt --check`, `git diff --check`. Fix all failures before proceeding.
4. Perform isolated GPUI GUI smoke if available. Never call this supported or production-ready from headless checks alone.
5. Fast-forward only experimental `feat/gpui-desktop-20261107` when validation passes; never merge official `main`, push, tag or release without explicit further instruction.
