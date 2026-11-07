# Alera Native Desktop (GPUI)

This is an **experimental, independent Rust UI frontend**, not a replacement of the shipping Flutter desktop app. It is limited to Windows/macOS/Linux and does not affect the existing mobile application.

## Run

Use Rust stable and platform graphics development tools; for Windows, release builds of GPUI 0.2.2 may additionally require the Windows SDK `fxc.exe` shader compiler.

```sh
cargo run --manifest-path desktop_gpui/Cargo.toml -- <path-to-a-project>
```

```sh
cargo test --manifest-path desktop_gpui/Cargo.toml
cargo check --manifest-path desktop_gpui/Cargo.toml
```

The startup argument is optional; by default the frontend uses the current directory.

## Current scope

- Native GPUI window with dark workbench layout.
- Bounded, read-only file browsing with explicit root containment; no shell spawning.
- Read-only UTF-8 file preview, limited to 256 KiB / 600 lines.
- Reuses `alera-core::git::current_branch` for Git branch metadata.
- No Flutter, Dart VM or WebView in this executable.
- No writes to workspace files from this preview.

**Not implemented yet:** editable code editor/IME, file watches, terminal/PTY display, saved tabs/workspaces, Git Graph/diff/merge, Agent profiles, desktop tray, settings, debugging, plugins, MCP and parity with Alera desktop. Those features should use existing Alera Rust sidecar/runtime contracts, not be reimplemented directly in GPUI views.

## Architecture

```
GPUI presentation (desktop_gpui/src/main.rs)
       ↓
Read-only workbench model (desktop_gpui/src/model.rs)
       ↓
Alera's shared Rust core (rust/alera-core)
```

The new frontend intentionally uses a **separate Cargo workspace** so that production Flutter desktop, mobile and terminal-host builds keep their current dependency graph and lockfile. When the native UI matures, connect it to existing runtime-host APIs using narrow capability ports. Do not bypass existing runtime ownership, security or request conventions.

## Performance verification before broader migration

Measure 10k/100k file entries using virtualized rows, 100k-line text (scroll/paint), PTY output stress, Git diff redraw and startup/idle CPU + RSS. Include Windows DirectWrite, macOS IME, HiDPI/multi-monitor and Linux X11/Wayland coverage. This preview is not a performance claim.
