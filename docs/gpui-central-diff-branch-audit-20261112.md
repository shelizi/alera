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
5. Hunk Previous/Next navigation uses a pure bounds-checked helper and cached parsed virtual rows, rather than re-parsing the patch on every click. Invalid old-page indexes, absent hunks and numeric overflow cannot select an out-of-bounds row.

## Test and UI gates

- New Native center Diff tests cover virtualized 500+ rows and Hunk numbering; long Unicode lines/total source limits; Git Original precedence; non-wrapping/overflow-safe Hunk paging; and Changes/Untracked vs Staged/Conflicts preview routing.
- **Verified on integrated latest-main worktree:** GPUI Rust Library **492/492 PASS**; Native Binary **28/28 PASS** (including **7/7** Center Diff regressions); `cargo check --offline --locked --bin alera-gpui-desktop` PASS; `cargo fmt --all --check` PASS; Windows `cargo build --offline --locked --bin alera-gpui-desktop` PASS.
- **2026-11-12 follow-up TDD:** Added a malformed Hunk-selection regression (invalid index and `usize::MAX`). RED compilation failed because the guarded control helper was absent; GREEN passes after reusing overflow-safe bounds checks for the visible buttons and label. `git diff --check` PASS. This is a Native unit-level safety fix, **not** a real-GUI acceptance.
- **2026-11-12 cached-row follow-up TDD:** RED compilation failed with an unresolved `center_hunk_target` reference; GREEN passed after Hunk Previous/Next was routed through cached parsed rows. Regression covers 530 context lines, two Hunk targets, page-edge navigation and invalid selection. Full Native **28/28** and Library **492/492** pass; Cargo check, rustfmt check and Windows native build pass. This does **not** replace real GUI verification.
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
- **File-level cross-check of 14 historical non-equivalent branches + one alternate integration ref:** all touched files still exist on `main` except the historical `desktop_gpui/src/settings_menu_ui.rs`. Its old `render_settings_menu_preview` path is now replaced by `native_settings_ui.rs` + `native_settings_catalog_ui.rs` with the same `matching_sections_localized` catalog helper. This is evidence of a refactor, **not** a behavioral-equivalence proof. Preserve the old branch and follow up on any uncovered UI interaction before cleanup.
- **2026-11-12 Rust declaration-name cross-check (read-only):** for the 11 historical feature branches (excluding three Terminal test branches), extracted added Rust `fn` declarations from `git diff --unified=0 main...branch`, then looked for each declaration name in the same relative file on `main`. **141 declaration occurrences inspected; 137 names exist in the same file; four do not**, all in the removed `desktop_gpui/src/settings_menu_ui.rs` (`preview_control`, `preview_row`, `preview_group`, `render_settings_menu_preview`). They were replaced by the newer Native Settings modules. This is **name-presence evidence only**, not signature, body, integration, or behavior equivalence; repeated declarations between branches are counted separately.
- **Terminal test spot-check:** `main` contains `TerminalCreateFlow` and its timeout control, `CreatedHostTerminal::matches_live_session`, `select_registered_host_workspace`, and both named layout persistence regressions (`terminal_layout_rejects_same_host_tab_aliased_under_different_handles`, `terminal_layout_can_be_updated_repeatedly_without_losing_prior_good_state`). This covers key assertions from three historical Terminal test branches at source level; actual Host/PTY UI acceptance remains open.
- **Already patch-equivalent:** 19 branches including Git patch paging, expanded diff dialogs, Hunk gutters, Editor Find, Project Search, dirty-pane guard, offline settings, and pinned-tab tests. Their named refs need no redundant merge merely to appear as ancestors.

## Decision boundary

Keep this work on the isolated integration branch until safe interactive Windows UI acceptance demonstrates: opening a Changes row and Center; large patch scrolling and Hunk jumps; no stale page rendering after switching files/workspaces; original Git tab and dirty Editor recovery; 1024x768 layout; and no Host/PTY mutations. Once gates pass, compare latest `main` again and fast-forward only after a fresh clean-tree/overlap check.
