# Alera native GPUI — 2026-11-08 next functional increment

Base: `c58b5688f5153d27259fb9d55f1bd260fd430392` on clean experimental `feat/gpui-desktop-20261107`. Integration: `integrate/gpui-project-terminal-tabs-20261108`.

## Scope requested

- Continue **feature implementation** for left project → Worktree hierarchy, right Explorer copy/duplicate, Terminal session tabs and editor Tabs.
- **Defer manual Windows GUI, real PTY and end-to-end acceptance** until separately requested. Limited compile/static/unit tests are permitted to catch obvious regressions during integration.
- Keep existing Terminal/Editor/Explorer functioning; no unrelated architecture migration. Never change dirty production `main`, push, tag, release, or delete existing user worktrees.

## Parallel source ownership (Devin CLI, SWE-2 Flash)

| Lane | Isolated branch | Allowed files | Deliverable |
| --- | --- | --- | --- |
| Terminal | `feat/gpui-terminal-tabs-20261108` | `desktop_gpui/src/terminal.rs` | Local tab reorder, next/previous, close others/right without terminating Host sessions; preserve read ticket generations |
| Editor Tabs | `feat/gpui-editor-tabs-20261108` | `desktop_gpui/src/editor_tabs.rs`, `desktop_gpui/src/editor_ui.rs` | Reorder, cyclic navigation, close clean others/right; native Ctrl+Tab navigation, no dirty buffer loss |
| Projects | `feat/gpui-project-tree-20261108` | new `desktop_gpui/src/project_tree.rs` | One bounded catalog of pinned/recent/active projects with per-repo Git worktrees, dedup linked checkouts and retain per-project errors |
| Explorer | `feat/gpui-explorer-copy-20261108` | `desktop_gpui/src/explorer_ops.rs` | Bounded symlink-safe copy into folder and duplicate, never overwrite or modify source |
| GPUI UI | `integrate/gpui-project-terminal-tabs-20261108` | `desktop_gpui/src/main.rs` | Display nested project/worktree rows, Terminal attach-existing picker and tab controls; coordinator handles cross-lane API wiring |

## Contracts

- `TerminalDock` methods: `activate_next()`, `activate_previous()`, `move_tab(from,to)`, `close_others(keep)`, `close_to_right(keep)`. These operate only on **local preview tabs**; never kill or create Runtime Host PTYs.
- `ProjectTreeGroup`: public `root:PathBuf`, `label:String`, `is_saved:bool`, `is_active:bool`, `worktrees:Vec<WorktreeEntry>`, `error:Option<String>`; `collect_project_tree(saved,recent,active)`.
- `ExplorerFiles` copy operations are explicitly non-overwriting, source-preserving and bounded. Clipboard/UI integration must clear pending state on workspace switch.
- An existing Host session can be opened in a Terminal tab only after explicit user selection. Current Alera `terminal` CLI supports list/read/write/attach but not a safe generic standalone `create` action, so do **not** claim `+ Attach existing` creates a new PTY.
- Editor dirty draft rules, session persistence and generation-scoped async response dropping remain mandatory.

## Acceptance boundary

Deliver code and report source files/commits. Verify at most Rust formatting, targeted headless regression tests and a compile check; do not claim manually verified GUI/IME, real Terminal Host session creation, native drag/drop, production support or release.

## Completed code and review (2026-11-08)

- **Project hierarchy** `e1ce1ec1a`: the model groups saved/recent/active workspaces by Git primary worktree, including linked worktrees and non-Git folders. GPUI now renders an indented clickable Projects / Worktrees list; background loading and workspace-generation checks prevent slow Git operations in render.
- **Terminal tabs** `a340f8a4e`: local reorder, previous/next cycle, close others/right, stale ticket safeguards; the center area offers functional tab controls and an explicit **Attach existing Host session** picker. Closing a preview never kills an existing Runtime Host PTY. Focused terminal model tests: 19 passed.
- **Editor tabs** `07a4e0ac`: tab reorder/cycle; close other/right **clean** tabs while dirty documents and draft buffers stay open; native Ctrl+Tab / Ctrl+Shift+Tab and previous/next/close UI controls.
- **Explorer copy** `d8fe5e033`: no-overwrite copy into a workspace folder and duplicate sibling, recursive bounded copy with source-preserving cleanup, symlink and Git metadata protection. The coordinator further fixed an exclusive-create race so failed copies **cannot delete preexisting target contents**; the added regression passed in 11/11 focused Explorer tests.
- **Consolidation:** These four source commits were imported as narrow file changes; `desktop_gpui/src/main.rs`, `lib.rs`, and this handoff document were wired in the separate integration branch without modifying official `main`. Windows `cargo check --all-targets --locked --offline`, scoped `rustfmt --check`, and `git diff --check` passed. Final headless Rust model tests: **144 passed, 0 failed, 0 ignored**.

**Known intentional limits:** No manual Windows GUI or Traditional Chinese IME acceptance, no real Host PTY attach/new-PTY end-to-end test, no session/tab OS drag/drop, no production release. Alera's current `terminal` CLI lacks a suitable general safe-create subcommand, so `+ Attach existing` does not start a new shell. Some Devin noninteractive sessions timed out or refused confirmation; the coordinator finished their scoped code and verified the integrated source. Unused experimental branches must not be confused with production `main`.
