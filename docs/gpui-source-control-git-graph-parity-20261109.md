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
| Source Control context menu | Stage, unstage, discard, open diff/original, copy path, open externally | Area-aware Git actions; immutable HEAD/Index tabs, external editor, content-verified Untracked Recycle Bin with two-click confirmation (no permanent-delete fallback) | Functional core complete; UI acceptance pending |
| Source Control layout | Flat/tree grouping, collapsible sections and folders, filtered counts, manual refresh | Native grouped/unified + flat/tree, section/folder collapse, accurate totals and refresh | P0 implemented; Windows UI acceptance pending |
| Source Control commits | Message validation, commit staged, amend / suggested message | Commit staged and two-step local Amend with exact HEAD/index snapshot, refuse upstream-reachable history rewrites, no force push | P1 Amend implemented; optional AI message suggestions remain |
| Git refresh | Timely repository / FS watcher updates, manual refresh reliable | notify 8.2 worktree + common Git-dir watcher; source-control updates on file changes, Git Graph ref history reload on external branch/Tag/HEAD changes, generation-safe single-flight refresh and manual fallback | P1 status and Graph invalidation implemented; sustained Windows UI/large-repo smoke pending |
| Git Graph ref list | Current ref first, favorites, local/remote/tag tabs; filters | All/Local/Remote/Tags tabs, case-insensitive scoped search, bounded 96-commit incremental Load More up to 512 without checkout | P1 bounded pagination implemented; full-repository server-side search remains |
| Git Graph branch right-click | Switch/worktree, compare, rename, pull, push, merge, rebase, archive, delete | History/Compare/Worktree/Switch/Merge prepare/Pull/Push, safe inactive rename plus current-branch rename only from owning live worktree | P1 owned rename + merged inactive local branch safe two-step deletion implemented; rebase/archival remain |
| Git Graph commit right-click | Preview, copy SHA/subject, branch/tag/worktree at commit, cherry-pick/revert/merge, resets etc. | Preview/copy, commit branch/tag/worktree creation; conflict-free Cherry-pick/Revert with explicit mainline 1/2 for two-parent merges and double-confirmation | P1 mainline implemented; octopus merge and destructive Reset actions pending |
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
- [x] Add dedicated in-memory immutable Git original tabs showing exact HEAD/Index blob with close/focus, no Save/Save As/draft/Undo, separate from editable NativeEditor and no worktree writes.
- [x] Add Open Worktree File in Alera and Open Externally (optional ALERA_EXTERNAL_EDITOR executable, otherwise OS-associated app); canonical path confinement and symlink safety.
- [x] Add guarded Git Commit Amend: prepare exact HEAD/index/message preview, second-click confirmation; reject stale index, conflicts, pushed/reachable upstream HEAD, merge commit and no-op. Never force-push.
- [x] Add two-step Untracked OS Trash/Recycle Bin workflow: limit 80 regular files/32 MiB, SHA content fingerprint, index/status/path checks and exact replay protection, refuse symlinks and permanent-delete fallback.
- [x] Add explicit Copy to editable scratch from an immutable Git original tab: creates a new NativeEditor scratch with ordinary Undo and Save As; original Git Blob and worktree file remain unchanged.
- [x] Coalesce manual refresh requests when a previous snapshot is in-flight; discard obsolete first result and refresh again; reset pending queue across workspace switches. Dedicated in-flight UI smoke still pending.
- [x] Add scoped notify filesystem watcher for worktree and common Git metadata with bounded event channels, 250ms quiet window/1400ms maximum delay, linked-worktree Git ref detection, and coalesced Graph refresh on external branch/Tag/HEAD changes.
- [ ] Verify sustained large-repository Windows watch throughput and recover from platform watcher disconnects; add optional reconciliation fallback.

## Phase 2 — Git Graph first-class Native functionality

- [x] Current branch and favorite refs sorted ahead of ordinary branches.
- [x] GPUI Kit popup menu for graph branches: show history, compare local branch, open existing worktree, switch branch, prepare merge, copy name.
- [x] GPUI Kit popup menu for graph commits: preview, copy hash/subject, create local branch or lightweight tag at selected immutable Commit SHA with two-step confirmation.
- [x] Add explicit file-level current-vs-selected branch comparison (merge-base..target), direction toggle, per-file committed patch and original blob preview. Arbitrary two-ref selection is still a later UI task.
- [x] Render bounded actual commit parent lanes with parent IDs, Unicode lane connectors, branch labels and scrolling; explicit fallback when full topology cannot be represented. Windows native visual acceptance remains pending.
- [x] Add in-memory case-insensitive Commit/branch/tag search with author:/sha:/message:/branch:/tag: prefixes over the bounded snapshot; preserve original graph lane indices.
- [x] Add All/Local/Remote/Tags UI tabs filtering only visible refs, preserving favorites and selected Git commit; Remote tab loads remote refs as needed, without checking out a branch.
- [x] Add bounded incremental Git History loading from 96 by 96 Commit rows up to 512, retaining ref/file scopes and topology indices; verified on 110-commit temporary repository.
- [x] Add guarded delete of merged inactive local branch only: no live worktree owner, no protected main/master/develop, no unmerged commits; two-click confirmation, remote branches untouched.
- [x] Allow checked-out branch rename only from its owning worktree via `git branch -m` with HEAD/commit revalidation; other linked worktrees must be opened first, never force overwrite another ref.
- [x] Add explicit mainline 1 or 2 for two-parent merge Cherry-pick/Revert with in-memory conflict-free preview and exact apply confirmation; refuse root, octopus and ambiguous parent selection.
- [ ] Add full-repository indexed search beyond the 512-row memory cap and optional virtualized list for very large histories.
- [x] Add guarded "Pull branch without switching" (non-checked-out fast-forward with CAS) and "Push selected branch" (exact tracked upstream, no force) to Git Graph right-click. Reuse existing Git service and two-step confirmation.
- [x] Add explicitly confirmed selected-branch Pull/Push actions to right-side branch controls (no checkout, no force).
- [ ] Consolidate selected branch actions into the dropdown itself and share UX for Merge-into-current without checkout.
- [x] Guarded rename of inactive local branch: GPUI Kit input, two-step confirmation, refuse checked-out worktree and existing destination; preserve HEAD and other worktrees.
- [x] Create branch and lightweight tag at exact selected commit; validate refs, refuse collision, never move HEAD/index/worktree; show tags in selectable Git Graph list.
- [x] Implement single-parent Cherry-pick/Revert with conflict-free in-memory preview, exact HEAD/source revalidation and two-step confirmation. Dirty worktree, merge/root commit, conflict and stale preview must be refused.
- [x] Create a new managed Worktree from an exact Commit SHA with new local branch and two-step confirmation; preserve current HEAD, index and files; refuse ref/folder collisions.
- [ ] Add guarded delete and merge-commit mainline Cherry-pick/Revert. Active-branch rename needs a separate explicit worktree-aware operation.
- [ ] Put reset/rebase/drop/force/destructive operations behind their own later explicit preview + confirmation; do not implement them as unconfirmed right-click actions.

## Tests / gates

1. Every worktree operation validates the exact repository/branch at action time, rejects stale graph state and locked/prunable paths.
2. Do not switch the current branch merely to compare, merge, pull, push or open an existing worktree.
3. Do not mutate Git from render or hover. Writes use background jobs, bounded progress, action-busy guard and explicit confirmation.
4. TDD on temporary Git repositories: changed index + WT copy, rename/delete, working-tree dirtiness, invalid path, multi-select, diverged branches, graph merge.
5. Run `cargo test --lib` focused tests and `cargo check --bin alera-gpui-desktop` after each milestone; verify Windows actual UI with a separate smoke gate.
6. Phase commits stay in this feature branch. Integrate into active GPUI integration branch only after clean build, review, and checks; never force-update or clobber another session's changes.

## Known implementation gap at this checkpoint

Native graph now uses exact `GitCommitRow.parent_ids` with a native GPUI Canvas renderer: stable palette IDs for lane movement, per-parent curved connectors, filled/outlined commit nodes, HEAD highlighting, and compact branch/ref pills. Its layout budget is 512 commits / 12 concurrent lanes, matching bounded incremental history loading; sparse whole-repository search intentionally shows only commits without invented graph edges. Commit preview and right-click actions still use the existing validated Git services.

Third-party evaluation (2026-11-18): reviewed rgitui's GPUI Git graph approach, but avoid importing its full application / its separately versioned GPUI runtime into Alera's pinned `gpui-kit = 0.9.0`. The new renderer uses the mature Canvas and `PathBuilder` primitives already provided by GPUI Kit and keeps the tested topology model as a UI-independent module.

Remaining acceptance work: perform a Windows native visual smoke test against Classic for long-lived branches, nested merges, 3+ parent merges, search filtering and virtualized history scrolling; inspect performance with 512 rows, DPI scaling and light/dark theme; re-evaluate third-party layout integration only if exact topology parity cannot be attained safely.

## 2026-11-19 follow-up verification

- Commit `9508dff3c` aligns Canvas commit rows to Classic's 28px height / 16px lane pitch and adds an octopus three-parent merge regression.
- Rust standalone topology tests: **7 PASS / 0 FAIL**, covering linear and merge history, unrelated tips, invalid parent edges, 512-row budget, >12-lane refusal, and three-parent merges.
- Added reusable real-Git topology smoke source: `tool/testing/gpui_git_graph_real_repo_smoke.rs`; compile with `rustc --edition=2021 tool/testing/gpui_git_graph_real_repo_smoke.rs -o desktop_gpui/target/debug/graph_smoke.exe`.
- Actual Alera `main`, 512 commits using `git log main -512 --topo-order --format='%H %P'`: **PASS**, 4 lanes, 17 merge commits and 1,108 edges.
- Actual Alera all refs, 512 commits using `git log --all -512 --topo-order --format='%H %P'`: **PASS**, 5 lanes, 12 merge commits and 1,263 edges.
- Rust 1.98 Windows `cargo check --offline --locked --bin alera-gpui-desktop -j 2`: **PASS** with only existing `dead_code` warnings for `menu_item` and `PanelRight`.
- **Full Classic GUI parity is NOT DONE**: targeted Windows Git Graph checks now pass, but the remaining Classic interaction and appearance differences listed below still need validation.

## 2026-11-19 real Windows GUI smoke (isolated Native preview)

- Windows Rust 1.98 `cargo build --offline --locked --bin alera-gpui-desktop -j 2`: **PASS**, Native debug executable generated; only the two pre-existing unused-code warnings.
- Launched the built Native EXE through the interactive Windows desktop with isolated `ALERA_GPUI_STATE_DIR` and `ALERA_RUNTIME_DIR`; intentionally no CLI/Host connection and no PTY actions. The initially attempted non-interactive process did not surface a window, so only the subsequent **visible interactive** launch counts as GUI evidence.
- Source Control snapshot and Graph Toggle: **PASS**, 80 branch entries (bounded), 96 initially loaded commits, stable colored GPUI Canvas nodes and parent lanes, and visible branch/tag ref pills; the current repository's unrelated tracked modifications were not altered.
- Resized the right sidebar to inspect actual lane geometry. Commit right-click popup and `Preview commit changes` read-only patch: **PASS**. No destructive menu item was selected.
- `Tags` ref-tab shows tag entries; `sha:9508dff3` filters Commit rows to the correct result, and clearing the query restores unfiltered rows: **PASS**.
- Incremental `Load More`: 96 -> 192 -> 384 -> **512 Commit rows**, with the 512 cap displayed: **PASS**. Mouse-wheel scroll on the Graph commit list shows older rows and colored lanes: **PASS**.
- **Baseline gaps before the follow-up below:** Classic's full main-area Graph/history view versus Native's max-220px right-sidebar commit viewport; actual multi-DPI and light/dark theme inspection; nested and 3+-parent merges on the GUI (headless topology tests are not pixel parity); all Branch/Commit actions and favorite/compare flows; sustained 512-row performance and long-lived ref navigation.

## 2026-11-19 full-height Native Git Graph follow-up

- Added a transient, full-height primary content projection `native_git_graph_center_ui.rs`, opened from Source Control's `Open full-height Git Graph` action. The existing 220px sidebar Graph and its Git services remain available.
- Uses the same bounded `GitHistorySnapshot`, true-parent layout, Canvas lane painter, search input, Commit context-menu controller, and confirmation gates. No new Git writers or Host/PTY calls.
- The mixed-pane layout stays mounted but hidden while expanded; closing the center Graph, selecting an existing mixed tab, opening an Editor, or switching Workspace returns to the normal workbench. This is **not yet** a durable movable/splittable Git History Tab like Classic.
- Initial native smoke found that a `hidden()` style applied before `flex()` left Terminal consuming space underneath. Corrected the style order and verified the Graph now fills the center height on Windows.
- Suppressed misleading unrelated Host/favorites warnings in the center title when local Git history already loaded successfully.
- Rust 1.98 Windows GPUI `cargo check` and `cargo build`: **PASS**, two unchanged dead-code warnings. Standalone topology unit tests: **7/7 PASS**. Format and `git diff --check`: **PASS**.
- Interactive Windows GUI in a separate state directory without CLI or Runtime Host: fullscreen graph with merged colored parent rails and ref labels, `sha:9508dff3` filter and clearing, incrementally loaded **96 to 512** commits, mouse-wheel navigation through older commits, Commit context popup, and Close returning to the preexisting Terminal pane: **PASS**. No destructive menu action performed.
- **Remaining for full Classic parity:** make Git History a persistent first-class movable/tabbed split-pane identity; add full Classic branch-perspective/merged-filter and comparison controls to the center header, verify editing unsaved buffers across opening/closing, multi-DPI/theme parity, and performance/virtualization beyond the bounded 512-row scope.
