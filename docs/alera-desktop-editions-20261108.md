# Two independently maintained Alera desktop editions (2026-11-08)

The user explicitly requested that the existing Flutter desktop and the new
GPUI desktop **remain two products that can be launched together**. Their
source may continue to share a repository and Rust Core/CLI, but shipping
identity, preview delivery and UI state must be independent.

| Contract | Classic (Flutter, unchanged) | Native (Rust GPUI) |
| --- | --- | --- |
| Windows EXE | `Alera.exe` | `AleraNative.exe` (packaged name) |
| Windows AppUserModelID | `dev.leynier.alera` | `dev.leynier.alera.native` |
| Development AppUserModelID | `dev.leynier.alera.dev` | same Native ID in preview builds |
| Windows single-instance lock | Classic `Win32SingleInstance`, unchanged | no Classic mutex or forwarding |
| UI state | original Classic app settings and SQLite stores | `%APPDATA%/AleraNative` for fresh installs |
| Legacy GPUI previews | not applicable | fallback `%APPDATA%/Alera/gpui-preview` if preferred Native directory is absent |
| Rust Core and CLI | existing `rust/` | reused `rust/`, CLI sidecar `alera.exe` |
| Runtime Host | existing authenticated Host | same shared Host via explicit no-autostart CLI access |
| Updates | unchanged signed Classic updater and release CI | manual `native-preview` ZIP, **no automatic updater** |
| Release readiness | preserve existing Classic policies | experimental `supported=false`; no production tag/release |

## Implementation and safety boundary

1. `desktop_gpui/src/edition.rs` defines the Native identity and separate state-root
   strategy. The Native entrypoint sets its own Windows process AppUserModelID
   **before creating the GPUI window**. Classic's Win32 app identity, mutex,
   updater/channel, installation path and executable are unchanged.
2. Fresh Native UI state, workbench pane layouts, projects, terminal preview
   layout and scratch drafts use a separate OS directory. Existing GPUI-only
   `Alera/gpui-preview` sessions are kept readable if the new directory does
   not yet exist; no automatic deletion/copy of sensitive unsaved drafts.
   `ALERA_GPUI_STATE_DIR` can supply a fully isolated test profile.
3. `tool/release/package_alera_native_windows.ps1` builds a ZIP containing
   an executable named `AleraNative.exe`, matching Alera CLI sidecar,
   `edition.json` and `README.txt`; it emits a SHA-256 checksum and refuses
   overwriting an existing archive. No installation registry, system service,
   Classic data directory, update manifest or real Host is modified.
4. `tool/release/workflows/alera-native-preview.yml` is the **manual-only
   workflow template** to be installed at `.github/workflows/alera-native-preview.yml`
   using the repository's protected-workflow approval process. The normal
   Git stage action rejected the protected path in this environment; no
   authorization was bypassed. Once installed, it will run its own Native
   and Rust CLI release compilation, tests and ZIP packaging. It uploads a
   short-lived GitHub Actions *artifact*, not a public Release. Existing
   Classic `release-cut.yml` and `fork-windows-release.yml` remain unchanged.
5. Classic and Native can run side by side in distinct Windows processes and
   taskbar groups. Opening the **same repository** is permitted, but the
   apps do not provide cross-process document locks; use separate worktrees
   for simultaneous edits. Both UI clients may see the **same Host-owned PTY**;
   do not type in the same active PTY concurrently until an explicit
   cross-frontend focus/lease model exists.

## Development and verification

Native model and UI checks:

```powershell
cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --lib --locked
cargo build --manifest-path desktop_gpui/Cargo.toml --locked
```

To build an opt-in archive from existing Windows binaries:

```powershell
./tool/release/package_alera_native_windows.ps1 `
  -NativeExe (Resolve-Path 'desktop_gpui/target/debug/alera-gpui-desktop.exe').Path `
  -CliExe (Resolve-Path 'rust/target/debug/alera.exe').Path `
  -OutputDir (Join-Path $env:TEMP 'alera-native-archive') `
  -Version '0.1.0-local'
```

The workflow's release-mode artifacts supersede debug binaries for regular
preview distribution. Packaging is not a substitute for Windows hands-on
dual-editor/IME/PTY acceptance. **No `main` merge, remote push, tag, public
release or Classic update is authorized by this split.**
