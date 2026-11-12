# GPUI Central Git Diff integration and historical branch audit (2026-11-12)

## Scope and source

- Target baseline: local Alera `main` at `f2b0ea5b9`. Only the isolated worktree `integrate/gpui-central-diff-into-main-20261112` is modified.
- Original central Diff source commit: `7de085dce` from `integrate/gpui-center-diff-20261110`, plus its acceptance-tracker commit `8bfb6581d`.
- The feature is **experimental**; neither compiling nor tests alone satisfy the real Windows visual/UI/Host acceptance gates. Keep `supported=false`. No Push, deployment or Host/PTY restart.

## Integrated behavior and safeguards

1. The right-side read-only Git file preview can open a **primary-pane Center Diff** through a bounded Git patch page. `native_center_git_diff_ui.rs` uses `gpui::uniform_list` to virtualize the rows, including pages with more than 400 lines.
2. Git Changes and Untracked rows feed the existing bounded page reader and expose the Center action. Staged and Conflicted rows retain their **exact-area** preview rather than silently substituting an unstaged patch. A separate staged-area central pager is not included in this slice.
3. The current Git Original snapshot takes presentation priority. Opening Center selects the temporary central surface while keeping the original tab/snapshot and any dirty Editor state. Closing Center does not save, discard or replace a document or Host PTY.
4. Generation, source-ticket and relative-file checks reject stale asynchronous patch responses; no Stage, Commit or Reset is invoked. The renderer caps each patch page at 32 KiB and visible rows at 4,096 characters, with longer rows labeled clipped.
5. Hunk Previous/Next navigation uses a pure bounds-checked helper. Invalid old-page indexes, absent hunks and numeric overflow cannot select an out-of-bounds row.

## Test and UI gates

- New Native center Diff tests cover virtualized 500+ rows and Hunk numbering; long Unicode lines/total source limits; Git Original precedence; non-wrapping/overflow-safe Hunk paging; and Changes/Untracked vs Staged/Conflicts preview routing.
- **Verified on integrated latest-main worktree:** GPUI Rust Library **492/492 PASS**; Native Binary **26/26 PASS** (including **5/5** Center Diff regressions); `cargo check --offline --locked --bin alera-gpui-desktop` PASS; `cargo fmt --all --check` PASS; Windows `cargo build --offline --locked --bin alera-gpui-desktop` PASS.
- **Real launch gate blocked:** the compiled Windows EXE lives in the shared Cargo target outside the configured workspace; the remote execution policy rejected its direct launch with `Path escapes the configured workspace`. Do not bypass the policy. The 1024x768 desktop was also covered by an unrelated Windows Firewall modal, which was left untouched.
- Therefore a successful binary build is **not** equivalent to a real GUI acceptance. No click, Hunk scroll, split-pane/Dirty/Undo or DPI smoke was claimed. `supported=false` and **no main merge** remain in effect.
- The Windows display was available at 1024x768, but at inspection time an unrelated **Windows Defender Firewall modal** covered the interactive desktop. Do **not** approve, cancel, or dismiss that modal on the user's behalf. Therefore actual click/scroll/resize/zoom UX is **NOT VERIFIED**; leave the real-GUI gate open.
- A disposable nested Git fixture (700 lines, two changed hunks) and empty isolated state directory remain **untracked inside this integration worktree only**. Direct Windows launch was denied, and guarded recursive cleanup was also refused by the repository protection policy (`PROTECTED_REPOSITORY_ASSET` for nested `.git`). Do not force-clear them or include them in any commit; clean only via an authorized safe method after a real GUI session.

## Historical GPUI branch coverage (local refs, no Fetch)

`git branch --no-merged main` yielded **36 named GPUI local branches**. Patch-equivalence checks showed **19** branches with no unique nonmerge patch vs main and **17** branches with unique patches, including two center-Diff refs for one feature and one alternate integrated History Sync line.

- **Central Diff: actual missing module** `desktop_gpui/src/native_center_git_diff_ui.rs` was absent from `main` before this integration. The two source refs are `feat/gpui-git-center-view-20261110` and `integrate/gpui-center-diff-20261110`; integrate once, not twice.
- **Historical Editor / Explorer / Projects / Worktree:** `main` already has `editor_tabs.rs`, `explorer_ops.rs`, `project_tree.rs`, and `worktree_manager.rs`. Nonidentical historical patches still require behavior-by-behavior comparison, not blind cherry-picking or deletion.
- **Terminal / Host tests:** `main` already contains `terminal_bootstrap.rs` and related guarded terminal layout and identity tests. Old test commits should be checked for missing assertions rather than replayed wholesale.
- **i18n / shared settings:** `main` already contains `native_i18n.rs`, settings catalog, `history_sync_bridge.rs`, and Native History Sync UI; exact historical commit hashes differ following merge/reconciliation.
- **Already patch-equivalent:** 19 branches including Git patch paging, expanded diff dialogs, Hunk gutters, Editor Find, Project Search, dirty-pane guard, offline settings, and pinned-tab tests. Their named refs need no redundant merge merely to appear as ancestors.

## Decision boundary

Keep this work on the isolated integration branch until safe interactive Windows UI acceptance demonstrates: opening a Changes row and Center; large patch scrolling and Hunk jumps; no stale page rendering after switching files/workspaces; original Git tab and dirty Editor recovery; 1024x768 layout; and no Host/PTY mutations. Once gates pass, compare latest `main` again and fast-forward only after a fresh clean-tree/overlap check.
