# Alera GPUI — P0 → P2 implementation and TDD closure (2026-11-08)

## Baseline and independent milestones

Source worktree: `.worktrees/feat-gpui-desktop-20261107`, baseline `18232c081`.
Integration branch: `integrate/gpui-p0p1p2-20261108`.

1. **P0** `8614d466e`: native Terminal raw-key encoder and bounded input model; authenticated `alera terminal --no-start resize --handle/--cols/--rows` routing to the existing Host lease-aware `resize` RPC; two-pane preview model and independent Host list/read ticket checks.
2. **P1** `c32f4568f`: git2-backed Stage/Unstage/Commit/existing local branch checkout/explicit Fetch with exact Git worktree-root checks, no shell and fixture tests.
3. **P2** `d1c8f8e7c`: Explorer copy/cut/paste and bounded native external file/folder import; safe stable Editor document/scratch drag identity and native drag/drop plus move-left/right controls.
4. **Native GUI integration** `bc4f089c0`: three-pane GPUI controls for all the above; terminal 1.5-second shared polling timer, independent split read, drag-to-reorder/cross-pane assignment; Git action background state; Explorer ExternalPaths drop/copy/cut/paste; per-project tree expand/collapse.

Devin CLI SWE-2 Flash agents were launched in separate worktrees for P0 Host, P0 panes, P1 Git writer, P1 history, P2 Explorer and P2 editor tabs. Noninteractive tool-approval failures and timeouts prevented those agents from producing code changes; coordinator implemented and validated the source without bypassing tool permissions.

## Implemented native workflow scope

**P0 Terminal**
- Click inside the VT screen/text transcript to focus a true native GPUI key-down route. Arrow/Home/End/Delete/Page/Backspace/Enter/Tab, UTF-8 text and common Ctrl control codes travel as bounded ordered raw writes through the authenticated existing Host; no implicit Enter, no Host spawn.
- Original IME-aware command bar remains for composed Chinese text. Raw-key CLI writes are serialized with it; no OS-specific live PTY/IME validation has been performed.
- Explicit 80×24 and 120×36 Host viewport presets call a bounded, authenticated Resize RPC; a mobile client holding the viewport driver can defer the actual resize.
- Attach two existing Host sessions to side-by-side/stacked native split preview panes. The second pane gets its own Host read updates. Close/swap, Terminal tab native drag reorder and drag onto the second-pane header do **not** terminate Host PTYs.

**P1 Git**
- Background-only operations from the right Git panel: per-file Stage and Unstage, staged-only Commit with configured Git signature, safe clean-worktree switch to an existing local branch, and explicit remote Fetch. All actions check current workspace generation and reject unsaved editor buffers.
- Git metadata, path traversal, symlinks, wrong worktree root and dirty branch checkout are refused. No merge, rebase, destructive reset, push, Auto Fetch scheduling or full Git Graph in this milestone.

**P2 file / Tab / project interactions**
- Right Explorer native Copy / Cut / Paste with local app-scoped clipboard. Copy never overwrites; failed Cut preserves the source. Cross-workspace paste must be separately authorized.
- Actual GPUI `ExternalPaths` OS file-manager drop into Explorer: external file or folder is copied in a background task (up to eight selected top-level paths, 32 MiB per file, 64 MiB per import operation, 4096 entries), no overwrites or Git control directories/symlinks, external source unchanged.
- Native editor Tab drag uses stable workspace path/scratch ID and preserves unsaved/Undo/Save identity; Terminal Tab drag uses session ID + workspace generation. Left project/worktree rows can expand or collapse their children.

## Verification and release boundary

- GPUI pure Rust model tests: **176/176 PASS** for final integrated source, including Git index writer, Explorer copy/external imports, raw Terminal keyboard, split and dirty Tab lifecycle.
- Windows native `cargo check --manifest-path desktop_gpui/Cargo.toml --all-targets --locked --offline`: **PASS**.
- Windows paired Alera CLI `cargo check --manifest-path rust/alera-cli/Cargo.toml --bin alera --locked --offline`: **PASS**.
- `rustfmt --edition 2021 --config skip_children=true --check` and `git diff --check`: **PASS** before milestone commits. Targeted CLI Resize parsing test runs separately.
- **NOT RUN**: actual GUI mouse drag/drop, Microsoft Zhuyin/TSF, real Host PTY keyboard/resizing/split, Windows explorer native file drop, macOS/Linux app, performance benchmarks, full Git remote with credentials. A passing model test/compiler does not certify those behaviors.

Remaining caveats inside the milestone: Resize is user-selectable presets, **not automatic on window resize**; second Terminal split currently shows a text preview rather than full styled VT grid; local Explorer file clipboard is not cross-application OS file clipboard; external import is guarded/bounded but not formally proven race-free against adversarial reparse-point swaps. Full IDE parity (Git Graph/Merge/Agent/MCP/LSP, multiwindow and signed releases) remains a separate later product stage.

Experimental GPUI remains **`supported=false`**. Official dirty `main`, production Flutter desktop, live Runtime Host, remote branches, tags, installer and releases were untouched. No push, tag or release.
