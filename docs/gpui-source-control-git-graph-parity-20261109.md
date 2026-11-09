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
| Source Control selection | Ctrl/Cmd toggle, Shift range, batch stage / unstage / discard | Ctrl/Shift and batch stage/unstage implemented | P0 completed except discard |
| Source Control context menu | Stage, unstage, discard, open diff/original, copy path, open externally | One-file stage/unstage and diff preview | P0 context menu; P1 external editor |
| Source Control layout | Flat/tree grouping, collapse sections, counters, filter, manual refresh | Filter and flattened area rows | P0 group headers/counters; P1 tree |
| Source Control commits | Message validation, commit staged, amend / suggested message | Commit staged supported | P1 amend and optional AI if configured |
| Git refresh | Timely repository / FS watcher updates, manual refresh reliable | Native refresh click and async snapshot | P0 bounded refresh on Git operations; P1 watcher |
| Git Graph ref list | Current ref first, favorites, local/remote/tag tabs; filters | Local+remote refs, favorites, current ordering adjusted | P0 ordering; P1 tags / search |
| Git Graph branch right-click | Switch or open checked-out worktree, compare, rename, pull, push, merge, rebase, archive, delete | Native context menu first safe subset: History/Compare/Worktree/Switch/Merge prepare/Copy | P0 safe subset; P1 pull/push/rename; P2 history rewriting |
| Git Graph commit right-click | Preview, copy SHA/subject, branch/tag/worktree at commit, cherry-pick/revert/merge, resets etc. | Native read-only preview + copy SHA/subject | P0 safe subset; P1 branch/tag/worktree; P2 destructive writes |
| Existing worktree navigation | For branches checked out elsewhere, offer open corresponding workspace instead of duplicate checkout | Native branch context menu identifies matching worktree and revalidates branch before switching | P0; finish with tests |
| Branch compare | Current vs selected with explicit direction and original-file opening | Read-only local compare and `compare_git_refs` infrastructure | P1 diff file list and compare dialog |
| Git Graph topology | Commit lanes/parents, merges, graph scrolling; inspect commit diff | Native only linear rows with merge marker; no true lanes | P1 draw topology with parent OIDs, bounded graph, line connectors |
| History scope | All branches, favorites, selected ref, file/folder history | Implemented as bounded Native history options | P0 keep; P1 persistence of view state |

## Phase 1 — P0 source-control day-to-day parity

- [x] Split staged/unstaged/untracked/conflicts, retain both areas for files changed in both index and working tree.
- [x] Search/filter Git files, branch-aware refresh, correct per-area stage/unstage actions.
- [x] Ctrl/Cmd + Shift multi-select; batch Stage/Unstage with path and index validation.
- [ ] Add per-area section headers and collapse/expand-all, rather than prefixing area labels to every filename.
- [ ] Add guarded discard for unstaged tracked changes and untracked files; explicit targeted confirmation, no silent `reset --hard`.
- [ ] Add context menu for one/multiple Git files: stage, unstage, discard, copy path, open diff/original.
- [ ] Verify manual refresh when a previous refresh is in flight; reconcile snapshot generation and watched file changes.

## Phase 2 — Git Graph first-class Native functionality

- [x] Current branch and favorite refs sorted ahead of ordinary branches.
- [x] GPUI Kit popup menu for graph branches: show history, compare local branch, open existing worktree, switch branch, prepare merge, copy name.
- [x] GPUI Kit popup menu for graph commits: preview, copy hash and subject.
- [ ] Add explicit file-level branch comparison (merge-base..HEAD direction explained), open original file from diff, compare two selected refs.
- [ ] Render actual commit parent lanes, not a misleading linear list. Include merge edges, branch labels, scroll bounds and filters.
- [ ] Search and filter commits, tags, local/remote refs, favorites, per-file history.
- [ ] Add "Pull branch without switching", "Push selected branch", "Merge branch into current without switching" to Graph and bottom branch switcher. Reuse one secure Git service, do not interpolate ref names into shell.
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

Native graph's `GitCommitRow.parents` currently carries a count, not parent edges. The row bullets (●/◆) are **not** a complete Git graph. A true topology requires collecting parent SHA IDs and assigning stable bounded lanes.
