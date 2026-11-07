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

## Current scope

- Native GPUI window with dark workbench layout.
- Bounded, read-only file browsing with explicit root containment; no shell spawning.
- Read-only UTF-8 file preview, limited to 256 KiB / 600 lines.
- Up to 24 read-only file preview tabs: reuse, activate, close and reload without writing the source.
- Explorer refresh button and folder-up navigation.
- Persistent read-only preview navigation and last-workspace recovery.
- Background, read-only Runtime Host status probe through the shared CLI.
- Reuses `alera-core::git::current_branch` for Git branch metadata.
- No Flutter, Dart VM or WebView in this executable.
- No writes to workspace files from this preview.

**Not implemented yet:** editable code editor/IME, file watches, runtime-owned
terminal/PTY display and tab state, multi-workspace selection UI, Git
Graph/diff/merge, Agent profiles, desktop tray, settings, debugging, plugins,
MCP and parity with Alera desktop. The preview state is *not* terminal or
unsaved-editor state. Future features should use existing Alera Rust
sidecar/runtime contracts, not be reimplemented directly in GPUI views.

## Verification

- Windows: `cargo check --all-targets` and debug `cargo build` passed with GPUI 0.2.2.
- Headless tests: 14/14 passed (navigation, persistence, path containment,
  file limits, tabs, reload and CLI status parsing) with
  `--no-default-features --lib`.
- A two-launch smoke run using an isolated `ALERA_GPUI_STATE_DIR`
  confirmed valid native Windows window handles, empty stderr, last-workspace
  persistence and correct workspace restoration when restarted from another
  working directory. Both test-owned processes were terminated afterward.
- This is **not** a visual/UI-automation pass, a release artifact or cross-platform binary parity claim.
- `.github/workflows/gpui-desktop.yml` defines separate experimental model and Windows build checks. It does not change Flutter release jobs.

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
