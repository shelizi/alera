# Native Git Graph bidirectional Diff — hunk/line gutter QA (2026-11-19)

## Scope

- Builds on the already merged `feat/gpui-git-graph-bidiff-20261119` branch-tip-to-branch-tip comparator. It **does not replace** its API, committed-tree semantics, remote ref support, or Source Control sidebar.
- Adds old/new line-number gutters to the central virtualized Patch view, using the existing `native_action_dialog_ui::parse_git_diff_rows` parser. Rows remain pre-parsed in the background, clipped to 4,096 characters per line and 8,192 visible rows; Git's original 48 KiB preview cap is unchanged.
- Adds Previous/Next Hunk controls using a bounded hunk ordinal and visible-row offset. File change, direction change and closing a comparison reset the hunk ordinal. Renames with no content changes have no fake hunks.

## Windows acceptance on a disposable divergent Git fixture

- Isolated frozen Native Windows Debug EXE and no Host/CLI; fixture retained the dirty `shared.txt` and untracked file on its `main` worktree throughout the test.
- `main -> topic` branch-tip comparison reported **4 changed file statuses** (deleted main-only, renamed old→new, modified shared, added topic-only); reversing `topic -> main` reported the **same 4 file identities with inverse statuses and signed patch lines**. This is intentionally different from the older merge-base-to-target projection that gave 3 vs 2 files.
- Selecting `shared.txt` shows old `2` next to `-MAIN_ONLY_CHANGE` and new `2` next to `+TOPIC_ONLY_CHANGE`, with preserved context old/new numbers. No uncommitted secret appears in these committed-tip patches.
- The Next Hunk control changed `Hunk 0/1` to `Hunk 1/1` and navigated the virtual row. Choosing another file reset to `0/1` for that file; reversing direction also reset selection.
- Focused tests: parser old/new gutter coordinates across two hunks, navigation at both ends without wrap, rename-only zero-hunk case, multi-byte line clipping, and 8,192-row visible cap: **2/2 PASS**. Full Native GUI binary suite: **64/64 PASS**. Rust 1.98 `cargo check` and Windows Debug `cargo build`: **PASS** (only two preexisting unused-code warnings).

## Remaining

Long-running 512-row graph stress, extreme narrow split/high-DPI theme, live Host/PTY mixed-pane acceptance. The central Compare view still uses the previously defined full branch-tip semantics; compare only immutable committed trees and never mutate the repository from this panel.
