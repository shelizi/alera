# Alera GPUI Windows smoke follow-up (2026-11-08)

Experimental fix worktree: `.worktrees/fix-gpui-smoke-navigation-terminal-20261108`.
Base: `5a8786644` from the clean P0-P2 integration branch. Production `main`, Flutter, release tags and existing Runtime Host sessions are out of scope.

## Reproduced issues

1. Clicking a workspace content-search match opened or focused the matching editor tab without moving the caret to its reported line/column. In a real isolated Windows GPUI window, `README.md:2:7` left the caret at line 8.
2. With `ALERA_CLI_PATH` deliberately pointing to a nonexistent binary, clicking `+ New Terminal` wrote a local error, but the no-session center surface did not show it.

## Fixes

- `EditorDocument::navigate_to_content_hit`: one-based line/scalar column to UTF-8 byte offset, snapped to a Unicode grapheme boundary. Bad line/column, stale lines and IME preedit fail without moving the caret. No document writes or Undo history changes.
- Native editor confirms the opened file's identity, updates caret/scroll/visible geometry and focuses keyboard input after a successful search result click.
- Terminal's no-session center area renders its actionable status, including the missing CLI explanation.

## Evidence

- Windows `cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --lib --locked --offline search_hit_navigation`: **2 passed, 0 failed**.
- Windows `cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --lib --locked --offline`: **178 passed, 0 failed**.
- Windows `cargo build --manifest-path desktop_gpui/Cargo.toml --locked --offline`: **PASS**.
- Windows real GPUI GUI, temporary test workspace under `%TEMP%\alera-gpui-smoke-20261108-test1` with separately isolated state/Runtime and invalid CLI:
  - `+ New Terminal` shows **Alera CLI missing; cannot create Host terminal** in the center.
  - Search `unique_search_731`, click `README.md:2:7`; the editor reports **Ln 2 / 8 Col 7**, correcting the previously observed line 8.
- Targeted `rustfmt` on only the three changed Rust files and `git diff --check`: **PASS**.

## Additional Windows functional smoke

- In the same disposable Git repository, GPUI `Git Worktrees → + New` created branch
  `smoke-ui-wt-20261108` in `.worktrees/smoke-ui-wt-20261108` and switched to the
  new worktree. Selecting `main` returned to the primary checkout and restored
  its file previews. The actual `git worktree list --porcelain` confirmed both
  roots and their correct branch references.
- After closing the test GPUI window, the new linked checkout had an empty
  `git status --porcelain --untracked-files=all`. It was removed using normal
  `git worktree remove` (without `--force`), scoped to the disposable TEMP Git
  repository. The test branch remains; no production worktree was touched.
- An independently compiled matching CLI, using
  `cargo build --manifest-path rust/alera-cli/Cargo.toml --bin alera --locked --offline`,
  **PASS** on Windows.
- Opt-in real-Host test with `ALERA_GPUI_TEST_CLI` and `ALERA_GPUI_TEST_APP`
  both set to this branch's debug binaries:
  `cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features
  --test live_runtime_host --locked --offline -- --ignored --nocapture`:
  **1 passed, 0 failed**, finished in 43.65s. The test created an independent
  temporary authenticated Host, verified that `--no-start` does not create a
  missing Host, created its own real PTY fixture, verified CLI list/read and
  cursor progression, rendered the transcript with the VT parser, and
  smoke-launched the native GPUI binary against that isolated Host.
  The test shut down its own Host; a later process check found
  **0 experimental CLI/GPUI processes** remaining.

## Registered Host terminal create, input and split follow-up

- Created a **separate test Git repository** under this fix worktree in
  `.gpui-terminal-e2e-20261108/workspace` and started a dedicated Runtime Host
  with a different control file, HOME, state directory and disposable project /
  workspace registration. No existing Host or user workspace was registered.
- The first actual GPUI `+ New Terminal` click reproduced
  `CLI create response attachment has no workspaceId`. **The Host nevertheless
  created a running PTY**, confirmed by authenticated CLI session listing.
  Blindly clicking New Terminal again could therefore create duplicates.
- Captured the real Host reply: `tab.id`, `tab.workspaceId` and
  `tab.payload.terminalSessionId` corroborate
  `attachment.sessionId`. The Host intentionally does **not** include
  `attachment.workspaceId` or `attachment.tabId`.
- The native adapter now accepts this real shape and the verified legacy shape,
  while refusing workspace, tab or session mismatches and replies without
  corroborating identity. Malformed replies now tell the user to
  **Attach existing before retrying**, because the RPC might have created a PTY.
- Added a unit reproduction using the actual Host shape: **failed before fix,
  passed after fix**. Full native Rust library regression: **179 passed,
  0 failed**. Windows GUI `cargo build --locked --offline`: **PASS**.
- Direct CLI in the separate test Host successfully created a running PTY,
  submitted input, requested **120×36** Resize and read incremental VT output.
  A real `echo` command produced the expected execution marker.
- Expanded opt-in `tests/live_runtime_host.rs` to register a disposable
  Workspace, call **RuntimeCliBridge.create_terminal** against a real Host,
  confirm the exact live session triple, run a command whose result is checked
  by reading a file inside the temporary workspace, resize, create a second
  independent PTY, and test split/swap/close in `TerminalPaneLayout`.
  **1 test passed, 0 failed** with the experimental CLI.
- The post-fix native GUI click test remained **blocked** by an unrelated
  Windows Security firewall dialog for Coding Tools MCP. The dialog was not
  accepted or rejected. No claim is made that the corrected `+ New Terminal`
  path, GUI raw keystrokes or GUI split controls passed by clicking.
- After testing, the dedicated GPUI and Host processes were terminated only
  after verifying their executable paths and test runtime arguments.
  **0 experimental GPUI/CLI processes remained**, and the corrected Windows
  executable rebuilt successfully once the test GUI released its file lock.
  The nested disposable Git test fixture under
  `.gpui-terminal-e2e-20261108/` remains **untracked in the fix worktree**
  because the MCP `protect_repository_metadata` policy rejected recursive
  removal of its `.git` directory. We did not bypass that safeguard.

## Still unverified

This smoke is not a blanket release gate. The live-Host fixture verifies
Host-backed PTY creation, write/resize/read, confirmation and the split model,
**not** the corrected `+ New Terminal` GUI click path after the firewall
dialog blocked input, nor GUI attach/raw keyboard, resize or split controls.
Microsoft Zhuyin/TSF, OS Explorer drag/drop,
macOS/Linux GUI and performance parity also remain separate checks.
GPUI remains `supported=false`. No production deployment, push, tag
or release is authorized.
