# Alera Native Desktop (GPUI)

This is an **experimental, independent Rust UI frontend**, not a replacement of the shipping Flutter desktop app. It is limited to Windows/macOS/Linux and does not affect the existing mobile application.

## Run

Use the Rust 1.98 toolchain and platform graphics development tools; for Windows, native GPUI builds may additionally require the Windows SDK `fxc.exe` shader compiler.

```sh
cargo run --manifest-path desktop_gpui/Cargo.toml -- <path-to-a-project>
```

```sh
cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --lib --locked
cargo check --manifest-path desktop_gpui/Cargo.toml
cargo build --manifest-path desktop_gpui/Cargo.toml
```

Fast, headless model tests (no GPUI renderer or graphical environment):

```sh
cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --lib
```

The startup argument is optional. Without a path, the frontend restores the
most recently opened workspace when possible, otherwise uses the current
directory. Preview tab order, active file and the Explorer directory survive
restart, but file contents are always re-read from disk and remain read-only.
The Explorer sidebar also has **Recent workspaces** and **+ Open**. The latter
uses GPUI's native operating-system folder picker, without installing an
additional dialog dependency. Up to 12 existing canonical workspaces are
remembered, newest first. Switching restores the destination's own preview
tabs and Explorer directory, and does not change the source project.

On workspace changes, the GPUI frontend discards its read-only local terminal
preview tabs (not Host-owned PTYs) and ignores outstanding read/list results
from the previous workspace. The Runtime Host session list remains global
and is explicitly marked **all workspaces**: this is not a workspace-scoped
Host session-ownership or permission migration.

State lives in `%APPDATA%\Alera\gpui-preview` on Windows, the
Application Support directory on macOS, or XDG State on Linux. It does not
modify the project or the production RuntimeStore.

For isolated smoke tests, set `ALERA_GPUI_STATE_DIR` to an absolute
directory so no existing user state is touched.

The **Check Host** button reads the status of the *existing* Alera Runtime
Host through the shipping `alera` CLI, not through a new host implementation.
Place `alera.exe` next to the GPUI executable, or set `ALERA_CLI_PATH` to
the absolute path of the installed CLI. `ALERA_RUNTIME_DIR` is honored in
the same way as the CLI. The probe is asynchronous, bounded to 5 seconds,
and never starts a host or handles RPC tokens itself.

The terminal sidebar now lists **existing** Runtime Host sessions via the
experimental Rust CLI's `terminal --no-start --json list`. Click a session
to open a local preview tab and retrieve the latest 8 KiB of output, or click
`Read more` to read the next chunk. `Reconnect` refreshes the list while
preserving local preview tabs. Output is rendered as a bounded plain-text
transcript (48 KiB), with ANSI, OSC and DCS escape sequences removed. It
also offers a **VT screen** rendering mode backed by the Rust `vt100`
parser. The VT screen is the default; click `Text log` / `VT screen` to
switch between the current 80-column, 24-row terminal screen and the
sanitized historical text transcript. The parser understands cursor movement,
clearing, scrolling, alternate-screen applications and Unicode. Its screen
state is kept independently per preview tab, with a bounded 240-row
scrollback. Terminal control strings are interpreted locally instead of
being sent to the GPUI text widget.

The default VT screen now renders **ANSI 16-color, 256-color, and 24-bit RGB
foreground/background** attributes through native GPUI styled text ranges.
It also renders reverse video, bold, italic, underline, and an explicit
visible cursor cell. Bounded adjacent runs are merged to avoid allocating
an individual GPUI view for each character. Wide Unicode glyphs do not
duplicate their terminal continuation cells. The dark-theme default
foreground/background and xterm-indexed palette are used for unset colors.

This is still a **read-only native VT screen**, not yet feature-complete:
clickable selections, copy/paste, Chinese IME input, keyboard control,
accessibility, and Host PTY resizing/attach leases have not been migrated.
Local VT preview dimensions are not advertised to the existing Host, so
programs should not assume the displayed panel matches their allocated
PTY size. Colored cells and workspace switching passed 41 model tests, Windows native check/build
and isolated Host + GUI process smoke tests; native pixel-level color and
glyph placement parity still needs visual UI testing.

An active terminal preview automatically checks for new output every 1.5
seconds by default. The sidebar also refreshes the Session list every 15
seconds. Only a single polling timer runs per GPUI window. Click `Auto: Off`
to pause automatic transcript updates without closing the existing
terminal; `Read more` remains available manually. Transient read failures
use capped exponential backoff, and responses from closed/reopened tabs or
replaced Host session identities are discarded.

For the live-host Windows integration regression, build the CLI from this
branch and run:

```powershell
$env:ALERA_GPUI_TEST_CLI = (Resolve-Path 'rust/target/debug/alera.exe').Path
# Optional: also smoke-run the GPUI .exe against the isolated test Host.
# $env:ALERA_GPUI_TEST_APP = (Resolve-Path 'desktop_gpui/target/debug/alera-gpui-desktop.exe').Path
cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --test live_runtime_host -- --ignored --nocapture
```

This test creates its **own** temporary Runtime Host and short-lived PTY
fixture. It does not use your Alera production Runtime directory or
credentials. It verifies the authenticated CLI list/read/cursor behavior
and the `--no-start` safety guard before and after Host termination.
When `ALERA_GPUI_TEST_APP` is set, it additionally launches the real native
GPUI executable against that isolated Host for a process-survival smoke test;
this is not a GUI click, terminal input or pixel-level rendering validation.

**Important:** For terminal list/read you must use the Alera CLI built from
this experimental branch, which adds the fail-closed `--no-start` option.
An older CLI lacking that flag will return an error and GPUI will not
start the Host as a fallback. The usual CLI default remains unchanged.

## Experimental native text editing (Phase 2, first slice)

Open a UTF-8 file from Explorer and click **Edit** above the preview.
The GPUI editor accepts native keyboard input through GPUI's
`EntityInputHandler`, including UTF-16 IME marked text and committed text.
It supports Unicode-grapheme Left/Right and Backspace/Delete, Enter,
Select All, clipboard copy/cut/paste, and explicit **Save** or Ctrl/Cmd+S.
Native editing now also supports **Undo** (Ctrl/Cmd+Z) and **Redo**
(Ctrl+Y / Ctrl+Shift+Z / Cmd+Shift+Z), with clickable status-bar actions.
Up/Down preserve a preferred Unicode-grapheme column across shorter lines;
Shift+arrows and Shift+Home/End extend selections in either direction.
Selected text and selected newline boundaries have visual highlighting,
and active IME composition is underlined. Click and Shift+click now use
GPUI's native font shaping to set or extend a selection at the nearest
displayed grapheme boundary, and dragging within the editor selects
across lines (including backwards). The caret is painted as a narrow
overlay rather than inserted into the text, so later glyphs do not jump.
Keyboard navigation, editing, and Undo/Redo now request minimal
vertical scrolling to keep the active caret line in the viewport.
While the left mouse button is held, dragging at the scroll region's
upper/lower edge continuously scrolls by at most one 19px row per 40ms
tick. The scroll loop stops on mouse release, at the file's scroll
boundaries, or after a 10-second fail-safe if the OS loses mouse-up.
Undo history is in-memory, limited to 64 steps and 4 MiB, and is not
recovered after a restart.
IME composition changes count as **one** undo transaction, not separate
changes for each candidate/preedit update.
Only `Save` writes back to the original source file. It validates workspace
containment and compares the current disk contents to the original snapshot;
external edits cause a conflict instead of silently being overwritten.

File-backed unsaved edits are recovered on **Edit** after an app restart.
Committed edits are written atomically to the experimental local
`gpui-preview/editor-drafts` state directory. The draft includes the
unmodified original text to recognize external edits even across restarts.
**These drafts are local plaintext, not encrypted**; do not use this
experimental editor for secrets. The draft is cleared after a successful
source save, or after clicking **Discard** and **Confirm discard**. An unsaved
editor blocks switching workspace or opening a different file until saved
or explicitly discarded. IME preedit is not written to drafts.

This is a **limited editor surface**, not the shipping IDE editor:
documents are capped at 256 KiB and only the first 600 lines are drawn.
GPUI now derives pointer hit-tests and IME candidate rectangles from the
same font shaper and scroll clipping bounds used to paint each line. An
offscreen caret does **not** yield a fabricated candidate rectangle.
**This geometry and the new auto-scroll have passed compile/unit checks,
not real mouse/IME acceptance tests.** Dragging across the editor window
boundary, keyboard navigation beyond the first 600 rendered lines,
line wrapping, BiDi layout, syntax highlighting, completion, line
virtualization, and verified Traditional Chinese IME
candidate behavior are still pending. Editing and draft persistence run
on the GPUI thread, which requires profiling before full-scale adoption.

## Current scope

- Native GPUI window with dark workbench layout.
- Bounded, read-only file browsing with explicit root containment; no shell spawning.
- Read-only UTF-8 file preview, limited to 256 KiB / 600 lines.
- Opt-in native GPUI file editor with guarded disk saves, Unicode/IME input
  interface and local crash-recovery drafts (experimental, not production).
- Up to 24 read-only file preview tabs: reuse, activate, close and reload without writing the source.
- Explorer refresh button and folder-up navigation.
- Persistent read-only preview navigation and last-workspace recovery.
- Native folder selection and a bounded recent-workspace picker, with
  per-workspace read-only tab restoration and stale terminal reply rejection.
- Background, read-only Runtime Host status probe through the shared CLI.
- Host-owned terminal session list, eight local transcript preview tabs,
  cursor-based incremental reads, VT screen with ANSI cursor/erase/alternate
  screen handling, ANSI RGB/indexed colored cells, cursor highlight,
  Text log fallback and explicit manual reconnection.
- Reuses `alera-core::git::current_branch` for Git branch metadata.
- No Flutter, Dart VM or WebView in this executable.
- Read-only preview and terminal surfaces never write files; only explicit
  native editor **Save** may update its selected workspace text file.

**Not implemented yet:** full code editor and tested IME interaction, file watches, full
interactive terminal/PTY rendering and input, persistent terminal tabs,
complete multi-workspace workflow, Git
Graph/diff/merge, Agent profiles, desktop tray, settings, debugging, plugins,
MCP and parity with Alera desktop. Preview navigation and local editor
recovery are independent of Terminal/Host state. Future features should use existing Alera Rust
sidecar/runtime contracts, not be reimplemented directly in GPUI views.

## Verification

- Windows: `cargo check --all-targets` and debug `cargo build` passed with GPUI 0.2.2.
- Headless tests: 67/67 passed (navigation, workspace switching and recent
  workspace history, UTF-16 and Unicode grapheme edits, bounded undo/redo,
  shaped pointer grapheme snapping, cross-line reverse drag selection,
  clamped mouse-edge auto-scroll and invalid scroll viewport rejection,
  multi-line selection, IME composition transaction handling, draft recovery and
  conflict-safe file saves, persistence, path containment,
  file limits, tabs, reload, terminal cursor, auto-poll backoff, stale
  response handling, VT100 screen, ANSI 16/256/RGB colors and inverse,
  wide Unicode, alternate screen, ANSI filtering
  and CLI parsing) with
  `--no-default-features --lib`.
- An opt-in Windows test passed against a real isolated Host and CLI: Host
  startup, session creation, authenticated list, incremental cursor reads,
  and fail-closed `--no-start` when the Host is absent or terminated.
- The same isolated Host regression passed with the freshly built GPUI
  executable running concurrently, without unexpected process exit or Host
  replacement; both test-owned processes were stopped after verification.
- The updated GPUI executable was also smoke-launched independently:
  nonzero Win32 window handle, empty stderr, test process cleaned up.
- A two-launch smoke run using an isolated `ALERA_GPUI_STATE_DIR`
  confirmed valid native Windows window handles, empty stderr, last-workspace
  persistence and correct workspace restoration when restarted from another
  working directory. Both test-owned processes were terminated afterward.
- This is **not** a visual/UI-automation pass, a release artifact or cross-platform binary parity claim.
- `.github/workflows/gpui-desktop.yml` defines separate experimental model and Windows build checks. It does not change Flutter release jobs.
- The experimental CI includes a Windows CLI parser check and isolated
  live-Host integration regression. CI jobs have not run remotely.

## Architecture

```
GPUI presentation (desktop_gpui/src/main.rs, native-ui feature)
       ↓
Read-only workbench model and preview session (desktop_gpui/src/lib.rs)
       ↓
Alera's shared Rust core (rust/alera-core)
       +
Read-only status via installed Alera CLI -> existing authenticated Runtime Host
```

The new frontend intentionally uses a **separate Cargo workspace** so that production Flutter desktop, mobile and terminal-host builds keep their current dependency graph and lockfile. When the native UI matures, connect it to existing runtime-host APIs using narrow capability ports. Do not bypass existing runtime ownership, security or request conventions.

## Performance verification before broader migration

Measure 10k/100k file entries using virtualized rows, 100k-line text (scroll/paint), PTY output stress, Git diff redraw and startup/idle CPU + RSS. Include Windows DirectWrite, macOS IME, HiDPI/multi-monitor and Linux X11/Wayland coverage. This preview is not a performance claim.
