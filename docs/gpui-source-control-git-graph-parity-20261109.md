# Alera GPUI Source Control + Git Graph parity plan (2026-11-09)

## Scope and checkout

- Feature branch: `feat/gpui-git-sidebar-parity-20261109`.
- Reuse existing clean worktree: `.worktrees/fix-classic-realtime-watch-20261109` (the folder name is historical; the checked-out branch is the feature branch).
- Start point: `integrate/gpui-p0p1p2-20261108`. Do not modify the actively used integration worktree directly.
- Source of behavior: Classic's `workspace_git_diff_panel*.dart`, `workspace_git_history_ref_menus.dart`, `workspace_git_history_commit_menus.dart`, `workspace_git_history_surface*.dart`.
- UI must use GPUI Kit native popups/dropdowns/modals and preserve Classic interaction semantics. It need not duplicate Flutter pixel geometry.

## Parity matrix and priorities

| Feature | Classic behavior | Native baseline / progress | Target |
|---|---|---|---|
| Source Control status | Staged / Changes / Untracked / conflicts, correct file status | Area-aware snapshot and file filter; same path can occur twice | P0 completed in dedicated branch |
| Source Control selection | Ctrl/Cmd toggle, Shift range, batch stage / unstage / discard | Ctrl/Shift batch stage/unstage and confirmed tracked-only unstaged discard | P0 core complete; native UI acceptance pending |
| Source Control context menu | Stage, unstage, discard, open diff/original, copy path, open externally | GPUI Kit context menu: preview, stage, unstage, copy relative path, guarded discard tracked unstaged | P0 core complete; original and external editor P1 |
| Source Control layout | Flat/tree grouping, collapse sections, counters, filter, manual refresh | Filter and flattened area rows | P0 group headers/counters; P1 tree |
| Source Control commits | Message validation, commit staged, amend / suggested message | Commit staged supported | P1 amend and optional AI if configured |
| Git refresh | Timely repository / FS watcher updates, manual refresh reliable | Native refresh click and async snapshot | P0 bounded refresh on Git operations; P1 watcher |
| Git Graph ref list | Current ref first, favorites, local/remote/tag tabs; filters | Local+remote refs, favorites, current ordering adjusted | P0 ordering; P1 tags / search |
| Git Graph branch right-click | Switch/worktree, compare, rename, pull, push, merge, rebase, archive, delete | Native History/Compare both directions/Worktree/Switch/Merge prepare/Copy plus guarded selected Pull/Push | P1 implemented except rename; P2 rewrite operations |
| Git Graph commit right-click | Preview, copy SHA/subject, branch/tag/worktree at commit, cherry-pick/revert/merge, resets etc. | Native read-only preview + copy SHA/subject | P0 safe subset; P1 branch/tag/worktree; P2 destructive writes |
| Existing worktree navigation | For branches checked out elsewhere, offer open corresponding workspace instead of duplicate checkout | Native branch context menu identifies matching worktree and revalidates branch before switching | P0; finish with tests |
| Branch compare | Current vs selected with explicit direction and original-file opening | Structured merge-base-to-target file list; direction toggle; per-file committed diff/original in read-only preview | P1 implemented, Windows UI acceptance pending |
| Git Graph topology | Commit lanes/parents, merges, graph scrolling; inspect commit diff | Native exact parent SHA + bounded Unicode lanes and connectors, fallback for invalid history | P1 completed code, Native visual acceptance still pending |
| History scope | All branches, favorites, selected ref, file/folder history | Implemented as bounded Native history options | P0 keep; P1 persistence of view state |

## Phase 1 — P0 source-control day-to-day parity

- [x] Split staged/unstaged/untracked/conflicts, retain both areas for files changed in both index and working tree.
- [x] Search/filter Git files, branch-aware refresh, correct per-area stage/unstage actions.
- [x] Ctrl/Cmd + Shift multi-select; batch Stage/Unstage with path and index validation.
- [ ] Add per-area section headers and collapse/expand-all, rather than prefixing area labels to every filename.
- [x] Add targeted two-click guarded discard for tracked unstaged changes from index, preserving staged state and all untracked files; unsupported/untracked deletion intentionally refused. Untracked removal requires a separate, explicit trash workflow.
- [x] Add GPUI Kit right-click menu for one/multiple Git files: Stage, Unstage, confirmed tracked-only Discard, copy relative path and preview diff.
- [ ] Add open original Git blob from Source Control, external editor integration and dedicated untracked-trash confirmation.
- [ ] Verify manual refresh when a previous refresh is in flight; reconcile snapshot generation and watched file changes.

## Phase 2 — Git Graph first-class Native functionality

- [x] Current branch and favorite refs sorted ahead of ordinary branches.
- [x] GPUI Kit popup menu for graph branches: show history, compare local branch, open existing worktree, switch branch, prepare merge, copy name.
- [x] GPUI Kit popup menu for graph commits: preview, copy hash and subject.
- [x] Add explicit file-level current-vs-selected branch comparison (merge-base..target), direction toggle, per-file committed patch and original blob preview. Arbitrary two-ref selection is still a later UI task.
- [x] Render bounded actual commit parent lanes with parent IDs, Unicode lane connectors, branch labels and scrolling; explicit fallback when full topology cannot be represented. Windows native visual acceptance remains pending.
- [ ] Search and filter commits, tags, local/remote refs, favorites, per-file history.
- [x] Add guarded "Pull branch without switching" (non-checked-out fast-forward with CAS) and "Push selected branch" (exact tracked upstream, no force) to Git Graph right-click. Reuse existing Git service and two-step confirmation.
- [x] Add explicitly confirmed selected-branch Pull/Push actions to right-side branch controls (no checkout, no force).
- [ ] Consolidate selected branch actions into the dropdown itself and share UX for Merge-into-current without checkout.
- [ ] Add guarded rename/delete/ref creation, tag/branch-at-commit, cherry-pick/revert/worktree-at-commit.
- [ ] Put reset/rebase/drop/force/destructive operations behind their own later explicit preview + confirmation; do not implement them as unconfirmed right-click actions.

## Tests / gates

1. Every worktree operation validates the exact repository/branch at action time, rejects stale graph state and locked/prunable paths.
2. Do not switch the current branch merely to compare, merge, pull, push or open an existing worktree.
3. Do not mutate Git from render or hover. Writes use background jobs, bounded progress, action-busy guard and explicit confirmation.
4. TDD on temporary Git repositories: changed index + WT copy, rename/delete, working-tree dirtiness, invalid path, multi-select, diverged branches, graph merge.
5. Run `cargo test --lib` focused tests and `cargo check --bin alera-gpui-desktop` after each milestone; verify Windows actual UI with a separate smoke gate.
6. Phase commits stay in this feature branch. Integrate into active GPUI integration branch only after clean build, review, and checks; never force-update or clobber another session's changes.

## Known implementation gap at this checkpoint

Native graph now also carries `GitCommitRow.parent_ids` and renders bounded Unicode lanes instead of just the old ●/◆ markers. The current output is a functional textual rail layout, not a polished vector graph. The render is limited to 96 commits and 12 lanes; when topology cannot be represented, Native explicitly shows the simplified commit list. A Windows GPUI UI acceptance test, refined graph geometry/interaction, and file-filter history edge cases are still outstanding.
