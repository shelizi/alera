# Alera GPUI Native — Terminal viewport + copy slice (2026-11-08)

Scope: experimental Native only, worktree
`integrate/gpui-p0p1p2-20261108`. Host continues to own every PTY.
No change to the Flutter application, production Host, or existing
Terminal tab/session identity rules.

## Automatic PTY Resize

- Observe actual GPUI window viewport size on render, subtract the fixed
  left/right sidebars and terminal controls, and divide remaining space for
  the current mixed Workbench split and Terminal split. Compute bounded
  dimensions with JetBrains Mono font size (columns 20–240, rows 5–100).
- Only propose for **visible, live, running Host terminal identities**.
  Every request holds the full local `TerminalTabContextTarget` identity,
  not merely a pane index or opaque handle. Includes the primary and, if
  present, secondary Host terminal pane.
- Debounce rapid window/font/split changes for **300 ms**; dispatch through
  the single existing Workbench poll (up to 1.5 seconds later) and the
  allowlisted `terminal resize --no-start` CLI. No direct PTY writes and
  no new Host process or duplicate PTY creation.
- Send a unique dimension/Host identity at most once; do not hammer a
  failed Host request. An explicitly clicked `80×24` or `120×36`
  manual preset wins until the viewport or font really changes.
- If another (e.g. mobile) Host client owns the resize lease, the Host can
  defer the resize. A requested viewport size is **not** proof that the
  backend PTY has already changed.
- The estimate is based on GPUI viewport geometry, not a pixel-perfect
  shaped text row measurement. Exact per-pane clipping/ConPTY redraw still
  needs follow-up Windows GUI + Host acceptance.

## Selection / clipboard

- Native **VT screen** allows selecting whole visible terminal rows by
  mouse down and dragging over subsequent rows. Selected rows are
  highlighted in the view. This is intentionally **whole-row, not
  arbitrary character or rectangular selection**.
- Use **Copy selected rows** or `Ctrl+Shift+C` to copy the selected rows;
  **Copy visible** copies the visible VT screen, or the sanitized bounded
  transcript when Text log mode is enabled.
- Clipboard content is plain UTF-8, never raw Host/PTY ANSI or OSC52
  escapes. A selection snapshots rendered VT rows and is valid only for
  its exact Host tab identity and output cursor. If the Host redraws or
  the tab changes, selected-only copy is refused, rather than copying
  a different session's new content.
- Existing `Ctrl+C` remains a PTY interrupt. Raw PTY keyboard input,
  composition via the separate command bar, and Host session lifecycle
  are unchanged.
- Text copying is capped at 128 KiB; trailing blank cells are trimmed
  from selected rows.
- Current scope: selection/copy toolbar actions target the primary
  terminal pane; selecting arbitrary characters, scrollback, and separate
  second-pane mouse selection remain future work.

## Automated verification / exclusions

Rust model tests cover:

- Viewport estimation, minimum/cap dimensions, malformed input.
- Coalescing of rapid window resizing; no duplicate RPC for unchanged
  dimensions; session identity reuse and hidden pane invalidation.
- Explicit manual resize precedence until real geometry change.
- VT whole-row Unicode selection, copying, bounds, and Host tab/output
  cursor invalidation.

The Native GPUI compilation check verifies the toolbar/mouse/keyboard
bindings. The session's primary target file and previous untracked
`.gpui-smoke-20261108/` are not modified by these tests.

Latest automated results: **268/268** library Rust tests passed,
`cargo check --manifest-path desktop_gpui/Cargo.toml --locked --offline`
passed for the Native GPUI binary, and `rustfmt --check` plus
`git diff --check` passed. The only toolchain warning was the
existing future-incompatibility notice for `proc-macro-error2 v2.0.1`.

**Still unverified:** Real Host/ConPTY viewport response on window drag,
terminal TUI apps, precise font metrics/character selection, real mouse
drag across a scrolling terminal and two-pane copy. Keep
`supported=false`; do not merge/push/tag/release as a result of these
model and compilation checks alone.
