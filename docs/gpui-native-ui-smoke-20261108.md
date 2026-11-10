# Alera GPUI Native — isolated Windows UI smoke (2026-11-08)

Scope: integration worktree `integrate/gpui-p0p1p2-20261108` at
`66b0c86de`, plus a Windows Native startup fix. This smoke used a disposable
Git repository under `%TEMP%/alera-gpui-ui-smoke-20261108-1719`, separate
Native settings state, nonexistent test CLI and nonexistent isolated Runtime
directory. **No production Runtime Host, remote Git endpoint, Flutter Classic
or user project was operated.** The previous untracked
`.gpui-smoke-20261108/` remains untouched.

## Startup blocker reproduced and fixed

The initial latest native Windows `cargo build --locked --offline` passed.
Launching that EXE with a real GPUI window through `cargo run` failed before
first frame with `0xC00000FD STATUS_STACK_OVERFLOW`:
`thread 'main' has overflowed its stack`.

Adding a **Windows MSVC executable-only** linker stack reserve of 8 MiB in
`desktop_gpui/build.rs` allowed the same isolated Git workspace to open and
render. The resulting PE32+ optional header was inspected: `SizeOfStackReserve
= 8388608`. This reserves virtual address space; it does not eagerly allocate
8 MiB of committed memory. The isolated GUI session closed normally with
process exit code 0; no Native process remained after closing the window.
This is a functional startup fix, not a substitute for wider performance
measurements or memory diagnostics.

## Direct Windows pointer checks

Performed on the spare physical display with a DPI-aware nonactivating window
move; the primary display's existing playback was not controlled.

| UI path | Observed result |
| --- | --- |
| Launch and three-pane workbench | PASS: left Projects/Worktrees, center Terminal/Editor and right Explorer/Git rendered |
| No matching Alera CLI/Host | PASS: explanatory CLI unavailable message displayed; no Host started |
| Git Push first click | PASS: button became **Confirm Push**, warning showed exact upstream/no-force intention |
| Switch Push confirmation to Pull | PASS: old confirmation disarmed; **Confirm Pull** showed FF-only/no merge/rebase warning |
| Switch Pull confirmation to Stash | PASS: old confirmation disarmed; **Confirm Stash** warned tracked-only storage |
| Second click on Stash in clean Git fixture | PASS: explicit refusal, `No tracked changes to stash`; no stash or Git changes created |
| Explorer open `smoke-alpha.txt` | PASS: Native Editor switched into view with expected two lines |
| Native Settings top entry and tabs | PASS: Application, Terminal and Editor panels opened |
| Application Auto Fetch toggle | PASS: `On` → `Off` → `On`; Native-only settings file persisted |
| Git fixture before/after | PASS: `git status --short --branch` remained `## master`; `git stash list` empty |
| Closing experimental window | PASS: clean exit, no remaining Native process |

Additional observations: the narrow right-side Git panel is readable but
confirmation text wraps near its bottom; improve action/status layout after
core functionality. Agent Profile/MCP/Terminal operations intentionally
reported missing CLI or Host; this is not an E2E failure in the isolated
non-Host smoke.

## Not covered

- No actual remote Push/Pull, credential helper prompt, protected server, or
  real configured upstream was exercised through the GUI.
- No real Host terminal/PTY or Agent launch; no simultaneous dual-editor
  typing, autosave, drag or restart acceptance; no real Chinese IME
  composition, multiple-DPI migration or other OS builds.
- This smoke proves first-frame and basic UI event delivery only; GPUI stays
  `supported=false`. Do not merge to production `main`, push, tag or
  release based on these results.

## Re-run commands

```powershell
$env:CARGO_TARGET_DIR = 'D:\alera-build\alera-gpui-target'
cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --lib --locked --offline
cargo build --manifest-path desktop_gpui/Cargo.toml --locked --offline
# Run the native binary only with an isolated test workspace, state and Host.
```
