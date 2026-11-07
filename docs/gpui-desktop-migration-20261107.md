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
- Native workspace UI now has bounded read-only file tabs (open/reuse,
  activate/close/reload), folder navigation and Explorer refresh.
- Split the UI-independent model into a `lib.rs` test target with GPUI
  behind the default `native-ui` feature, so CI can test without GPU libs.
- Local read-only preview tabs, active file and Explorer directory are
  restored after restart, per workspace. Stored paths are relative and
  re-canonicalized; no file text, token, or terminal state is persisted.
- Two-launch Windows smoke test confirmed a native window on each launch and
  recovery of the same workspace even when the second process started from
  a different working directory, using an isolated app-state directory.
- The GPUI sidebar has a background, bounded, read-only Check Host action
  using the existing Alera CLI and its authenticated runtime client. This
  does **not** establish an interactive PTY view or a full Runtime RPC client.
- Experimental CI added for Windows native compile and cross-platform model
  tests; CI jobs have not run remotely and macOS/Linux are **unverified**.
- Open issues: model performs small bounded disk reads and tiny state-file
  saves on the UI thread; Explorer is limited to 400 entries per directory.
  GPUI has no interactive runtime-host session client yet, no persisted
  terminal/editor buffers, and no performance parity results. The CLI status
  button has no live-host integration smoke test yet.

The GPUI prototype remains **unsupported**. Terminal, editor editing/IME,
Diff, Agent and release behavior still depend on Flutter until the respective
gates pass. Do not merge this feature branch into `main` or publish it as
supported desktop builds before verification.
