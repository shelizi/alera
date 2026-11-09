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
| Source Control context menu | Stage, unstage, discard, open diff/original, copy path, open externally | Area-aware Staged/Changes diff, read-only Git original at HEAD/Index in preview, Stage/Unstage/copy path/guarded discard | P0 core + original preview implemented; external editor and full file tab pending |
| Source Control layout | Flat/tree grouping, collapsible sections and folders, filtered counts, manual refresh | Native grouped/unified + flat/tree, section/folder collapse, accurate totals and refresh | P0 implemented; Windows UI acceptance pending |
| Source Control commits | Message validation, commit staged, amend / suggested message | Commit staged supported | P1 amend and optional AI if configured |
| Git refresh | Timely repository / FS watcher updates, manual refresh reliable | Async snapshot + coalesced extra read when refresh pressed during in-flight scan; guards old workspace generation | P0 manual refresh queue implemented; P1 filesystem watcher and Windows smoke pending |
| Git Graph ref list | Current ref first, favorites, local/remote/tag tabs; filters | Local/remote branches with favorites and selectable Git tags (lightweight + annotated), current ref sorted first | P1 tag listing implemented; filtering and dedicated tabs remain |
| Git Graph branch right-click | Switch/worktree, compare, rename, pull, push, merge, rebase, archive, delete | Native History/Compare both directions/Worktree/Switch/Merge prepare/Copy, selected Pull/Push and safe inactive-local-branch rename | P1 core implemented; active-branch rename and destructive actions remain later |
| Git Graph commit right-click | Preview, copy SHA/subject, branch/tag/worktree at commit, cherry-pick/revert/merge, resets etc. | Preview + copy SHA/subject, exact-commit branch creation or lightweight tag creation with explicit name/confirmation | P1 branch/tag complete; worktree-at-commit and destructive actions later |
| Existing worktree navigation | For branches checked out elsewhere, offer open corresponding workspace instead of duplicate checkout | Native branch context menu identifies matching worktree and revalidates branch before switching | P0; finish with tests |
| Branch compare | Current vs selected with explicit direction and original-file opening | Structured merge-base-to-target file list; direction toggle; per-file committed diff/original in read-only preview | P1 implemented, Windows UI acceptance pending |
| Git Graph topology | Commit lanes/parents, merges, graph scrolling; inspect commit diff | Native exact parent SHA + bounded Unicode lanes and connectors, fallback for invalid history | P1 completed code, Native visual acceptance still pending |
| History scope | All branches, favorites, selected ref, file/folder history | Implemented as bounded Native history options | P0 keep; P1 persistence of view state |

## Phase 1 — P0 source-control day-to-day parity

- [x] Split staged/unstaged/untracked/conflicts, retain both areas for files changed in both index and working tree.
- [x] Search/filter Git files, branch-aware refresh, correct per-area stage/unstage actions.
- [x] Ctrl/Cmd + Shift multi-select; batch Stage/Unstage with path and index validation.
- [x] Add area-group section headers, filtered/total counts, collapse/expand-all and flat/tree toggle, including collapsed folder rows in tree mode.
- [x] Add targeted two-click guarded discard for tracked unstaged changes from index, preserving staged state and all untracked files; unsupported/untracked deletion intentionally refused. Untracked removal requires a separate, explicit trash workflow.
- [x] Add GPUI Kit right-click menu for one/multiple Git files: Stage, Unstage, confirmed tracked-only Discard, copy relative path and preview diff.
- [x] Add Stage-specific diff preview (HEAD -> index) and Changes-specific diff preview (index -> worktree), plus original Git Blob (HEAD/Index) in Source Control read-only preview.
- [ ] Add full read-only original-file editor tab, external editor integration and dedicated untracked-trash confirmation.
- [x] Coalesce manual refresh requests when a previous snapshot is in-flight; discard obsolete first result and refresh again; reset pending queue across workspace switches. Dedicated in-flight UI smoke still pending.
- [ ] Implement low-latency FS/Git watcher refresh with event coalescing, bounded retry and freshness checks.

## Phase 2 — Git Graph first-class Native functionality

- [x] Current branch and favorite refs sorted ahead of ordinary branches.
- [x] GPUI Kit popup menu for graph branches: show history, compare local branch, open existing worktree, switch branch, prepare merge, copy name.
- [x] GPUI Kit popup menu for graph commits: preview, copy hash/subject, create local branch or lightweight tag at selected immutable Commit SHA with two-step confirmation.
- [x] Add explicit file-level current-vs-selected branch comparison (merge-base..target), direction toggle, per-file committed patch and original blob preview. Arbitrary two-ref selection is still a later UI task.
- [x] Render bounded actual commit parent lanes with parent IDs, Unicode lane connectors, branch labels and scrolling; explicit fallback when full topology cannot be represented. Windows native visual acceptance remains pending.
- [ ] Search and filter commits, tags, local/remote refs, favorites, per-file history.
- [x] Add guarded "Pull branch without switching" (non-checked-out fast-forward with CAS) and "Push selected branch" (exact tracked upstream, no force) to Git Graph right-click. Reuse existing Git service and two-step confirmation.
- [x] Add explicitly confirmed selected-branch Pull/Push actions to right-side branch controls (no checkout, no force).
- [ ] Consolidate selected branch actions into the dropdown itself and share UX for Merge-into-current without checkout.
- [x] Guarded rename of inactive local branch: GPUI Kit input, two-step confirmation, refuse checked-out worktree and existing destination; preserve HEAD and other worktrees.
- [x] Create branch and lightweight tag at exact selected commit; validate refs, refuse collision, never move HEAD/index/worktree; show tags in selectable Git Graph list.
- [ ] Add guarded delete, cherry-pick/revert and worktree-at-commit. Active-branch rename needs separate explicit worktree-aware operation.
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
