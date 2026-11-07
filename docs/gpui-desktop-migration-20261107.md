# GPUI Native Desktop Migration (isolated feature worktree)

## Objective and invariants

Deliver a Windows/macOS/Linux native Rust desktop frontend for Alera using
GPUI, without Flutter or WebView at runtime. The existing Flutter desktop
build remains the supported release until feature parity, performance and
platform gates are satisfied. Do not migrate the mobile Flutter application.

- Branch: `feat/gpui-desktop-20261107`.
- Worktree: `.worktrees/feat-gpui-desktop-20261107` (separate from dirty `main`).
- GPUI crate: `desktop_gpui/` (standalone Cargo workspace/lockfile).
- Preserve the Alera dark visual design, Inter and JetBrains Mono fonts,
  keyboard and terminal-first behavior, security, and state persistence.
- Reuse `rust/alera-core` services and `rust/alera-cli` terminal/runtime host.
  Do not fork Git semantics, PTY ownership, credentials or MCP permissions.
- A GPUI renderer does not imply a higher FPS or lower CPU by itself.
  Measure before replacing production entrypoints.

## Implementation stages

| Phase | End-to-end deliverable | Verification gate |
| --- | --- | --- |
| 0: Scaffold | Native window, sidebars, file explorer, safe read-only preview, Git branch | Cargo check/test + Windows launch |
| 1: Runtime | Narrow client adapters for existing CLI/host protocol, workspace selection, session persistence | Protocol tests against real host, reconnect/restart |
| 2: Editor | GPU line virtualization, rope buffers, syntax highlighting, edits, save/conflict recovery, scratch autosave | 100k lines; Unicode and Traditional Chinese IME; crash recovery |
| 3: Terminal | Reuse existing PTY/session ownership, render scrollback and keyboard input, tabs/splits | Bulk output, Windows/macOS/Linux PTY smoke, no console flashes |
| 4: Git | Worktree explorer, staging, multi-select, diffs, Graph, merge/conflict UX | Status/merge parity, tests across linked worktrees |
| 5: Agents/MCP | Existing hooks, agent profiles, terminal attachment, cross-project navigation, guarded actions | Permissions and cross-workspace regression tests |
| 6: Desktop integration | Multiwindow drag/drop, clipboard, app menu, tray, deep links, accessibility, settings | OS-specific integration and UI automation |
| 7: Release readiness | Cross-platform packaging, CI, version/updater, logs, benchmark comparisons | No functionality regression, explicit review before switching default |

## Architectural boundaries

```text
GPUI Views (paint/layout only)
    ├── Workbench/Editor/Terminal/Git/Agent presentation
    │        ↓ actions + subscriptions
    └── Feature-scoped application owners/controllers
                 ↓ narrow capability traits
        Alera Rust Core + Runtime-Host Client
                 ↓ existing authenticated protocol
        Alera CLI / Terminal Host / SQLite RuntimeStore
```

GUI views must not own PTY processes, SQLite pool, Git credential handling,
connection reconnect policy, or process-spawn semantics. Reuse the existing
`alera_core::child_process` helpers and the `dunce` Windows path rule.
Preserve existing runtime protocol/security versions unless a separately
approved protocol migration requires a change.

## Performance comparison procedure

Measure identical datasets and workbenches in Flutter desktop and GPUI:
startup-to-first-frame, idle CPU/RSS, 10k/100k-line editor scroll, typing
latency (including IME), 100k-line diff scrolling, terminal heavy output,
large-workspace tree and search, multiwindow movement and DPI transitions.
Record p50/p95 frame time, CPU, RSS, allocation bursts and input-to-paint
latency on Windows and macOS first, then Linux. Compare after warm caches,
and repeat enough runs to distinguish noise. Do not assert superiority
without reproducible measurements.

## Verified progress (2026-11-07, Windows)

- Standalone GPUI Windows executable built and smoke-launched successfully
  with a nonzero native window handle and empty stderr; the smoke process
  was then stopped. No automated visual inspection or input UX test yet.
- `cargo check --all-targets` passed and 14 headless model tests passed
  after adding local preview-state persistence and CLI status parsing.
- GPUI Phase 3 first slice: terminal session list and manual reconnect using
  the existing Alera CLI, bounded incremental terminal text read with
  cursor/restart detection, eight local terminal preview tabs and ANSI/OSC/DCS
  filtering; 22 headless tests passed and Windows GPUI check succeeded.
  Output is a **read-only transcript**, not a VT emulator or writable PTY.
- The experimental Rust CLI has an opt-in `terminal --no-start` flag; GPUI
  always passes it, so listing or reading a session cannot start the Host
  when it is missing. Older CLIs reject the flag; this fails closed.
- Native workspace UI now has bounded read-only file tabs (open/reuse,
  activate/close/reload), folder navigation and Explorer refresh.
- Split the UI-independent model into a `lib.rs` test target with GPUI
  behind the default `native-ui` feature, so CI can test without GPU libs.
- Local read-only preview tabs, active file and Explorer directory are
  restored after restart, per workspace. Stored paths are relative and
  re-canonicalized; no file text, token, or terminal state is persisted.
- Phase 1 workspace-switch follow-up: GPUI native folder picker and a bounded
  recent-workspace list; destination previews are restored only after
  validation, and switching leaves the current workspace intact on failure.
  Old async terminal read/list results are rejected after a switch. The
  Host session list remains explicitly global, not project-filtered. The
  headless Rust suite expanded to 41 passing tests, including MRU bounds,
  tampered/stale paths, cross-workspace session restore and failure rollback.
- Phase 2 first editor slice: opted-in native file editing surface wired to
  GPUI 0.2.2 EntityInputHandler for UTF-16 IME composition. Unicode grapheme
  navigation, paste/cut, and explicit Save use a bounded pure Rust buffer.
  Every disk Save checks canonical workspace containment and original file
  contents; external changes fail closed. Committed unsaved edits persist to
  a separate plaintext local draft file with original disk snapshot, and
  are offered on next Edit. Uncommitted IME composition is not persisted.
  Closing/discarding has a two-click confirmation and dirty edits block
  workspace switching. 52 headless Rust tests passed. This is not full editor
  parity: render limited to 600 lines, file size 256 KiB, line-end mouse
  caret, no virtualized buffer, selection geometry/IME candidate rectangles,
  syntax services, undo, or real Traditional Chinese IME UI acceptance tests.
- Phase 2 editing follow-up: added bounded pure-Rust Undo/Redo snapshots
  (64 transactions, 4 MiB), with IME marked-text updates grouped into a
  single undo and final unmark committed to the local recovery draft. Added
  directional UTF-16 selection, grapheme-safe left/right and multiline
  up/down with retained column, shift-selection, Home/End and keyboard
  shortcuts. The GPUI editor highlights selections/newline markers and
  composition, shows line/column plus clickable Undo/Redo status actions,
  and accepts shift-click selection to a line end. 62 headless Rust tests
  pass, including Unicode/IME/draft/selection regressions. This remains
  experimental: no pixel-accurate mouse caret, scroll-aware IME candidate
  placement, virtualized text viewport, or verified Windows Bopomofo UX.
- Phase 2 geometry follow-up: introduced native GPUI \`ShapedLine\`
  metrics for each painted editor row. Mouse click, Shift-click and
  cross-line drag are mapped through actual glyph widths and snapped to
  complete Unicode grapheme boundaries; the caret is painted as a slim
  overlay rather than an inserted character that shifts the line. IME
  bounds lookup uses the same pixel geometry and GPUI scroll clipping,
  rejecting offscreen candidate anchors. Three pure Rust regressions cover
  combining characters, ZWJ emoji, and reversed multiline pointer drag;
  total 65 headless tests passed. Native Windows type check succeeded.
  Actual pointer event routing, drag autoscroll and real Bopomofo
  candidate window placement still need visual Windows acceptance testing.
- Phase 2 viewport follow-up: bound native editor scroll region to GPUI
  ScrollHandle. Keyboard navigation, edits and undo/redo request minimal
  vertical caret reveal for the first 600 rendered lines.
  While actually dragging with the left mouse button, a single 40ms timer
  applies a pure bounded scroll step (19px per tick) at the viewport's
  upper/lower 24px; mouse-up, release detected by move, scroll limits,
  and a ten-second lost-event fail-safe stop the loop. Two new pure Rust
  regressions cover clamping and bad viewports; 67 headless tests pass.
  Actual mouse/IME GUI behavior and navigation beyond the 600-line
  prototype render cap remain unverified.
- Phase 2 virtualized long-file follow-up: replaced the native editor's
  600-line render cap with GPUI 0.2.2 uniform_list. It only instantiates
  visible rows, reuses the same native glyph shaping/caret/IME bounds and
  per-row selection highlighting, and retains the ScrollHandle-based
  edge-drag behavior. A bounded UTF-8 byte-offset line index maps rows
  (including empty/trailing lines) without copying offscreen text.
  New Ctrl/Cmd document Home/End bindings and Shift variants reveal any
  caret row; file-backed EOF caret is revealed on first opening.
  71 headless tests passed, including 5,000 Unicode rows well beyond 600
  and long-document navigation with Undo. The editable document size cap
  remains 256 KiB; the separate read-only preview still renders 600 lines.
  Real GPU/window event routing, scroll/IME candidate positions and
  variable-height/bidirectional text need Windows GUI acceptance testing.
- Two-launch Windows smoke test confirmed a native window on each launch and
  recovery of the same workspace even when the second process started from
  a different working directory, using an isolated app-state directory.
- The GPUI sidebar has a background, bounded, read-only Check Host action
  using the existing Alera CLI and its authenticated runtime client. This
  does **not** establish an interactive PTY view or a full Runtime RPC client.
- Experimental CI added for Windows native compile and cross-platform model
  tests, plus the CLI no-start parser and isolated Windows live-Host
  integration regressions; CI jobs have not run
  remotely and macOS/Linux are **unverified**.
- Phase 3 follow-up: one GPUI-window-owned timer enables 1.5-second
  automatic cursor polling for the active preview and 15-second Session
  discovery, pause/resume, bounded failure backoff and stale asynchronous
  reply rejection by tab generation/cursor/session identity. 26 headless
  model tests passed.
- Phase 3 VT screen slice: added independent Rust `vt100` parser state for
  each Host-owned terminal preview, supporting cursor motion, erase, scrolling,
  alternate screen and Unicode, with a toggle back to the sanitized transcript.
  The initial screen rendered in monochrome at a fixed local 80x24 grid.
  33 initial headless tests passed; this does not yet support keyboard/IME, Host resize
  or an input-authorized terminal attachment lease. The isolated Host
  integration test additionally passes actual PTY output through this parser.
  Do not enable GPUI writes merely by calling the CLI's `terminal write`
  verb: that can inject text into an existing session even when GPUI only has
  a read-only view. First implement explicit user-scoped input consent,
  attachment/session identity verification, session lease and IME composition
  with controlled handoff to the existing Host.
- Phase 3 color follow-up: native GPUI StyledText runs now render vt100
  per-cell indexed ANSI 16/256 and RGB true-color foreground/background,
  reverse-video, bold, italic, underline, and visible cursor. The pure Rust
  snapshot is bounded to 100x240 cells and coalesces equal adjacent styles;
  wide Unicode continuation cells are suppressed. 37 headless tests pass.
  Windows native check/build/window smoke and isolated Host/PTY/CLI+GUI
  process integration passed. Visual color/glyph alignment automation is
  still required before parity.
- Real Windows Runtime Host/CLI/PTY end-to-end regression passed using a
  separate temporary runtime/profile and test-only credential; verified
  session creation, CLI no-start safety, list/read and incremental cursors.
  A concurrent native GPUI process stayed alive while using that existing
  Host; a separate Win32 smoke test confirmed a valid window handle and
  empty stderr. Test Host, PTY and UI processes were terminated. This is
  not a VT/input UI or visual-automation test.
- Open issues: model performs small bounded disk reads and tiny state-file
  saves on the UI thread; Explorer is limited to 400 entries per directory.
  GPUI has no persistent direct runtime-host socket client yet, no persisted
  terminal buffers or full editor sessions, PTY input/resize, or performance parity results.
  Automatic reads currently invoke the existing CLI subprocess, and are
  intentionally experimental until a persistent authenticated client and
  input/VT rendering are implemented.

The GPUI prototype remains **unsupported**. Terminal, editor editing/IME,
Diff, Agent and release behavior still depend on Flutter until the respective
gates pass. Do not merge this feature branch into `main` or publish it as
supported desktop builds before verification.
