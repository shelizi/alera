# Windows shared-engine Terminal opt-in experiment (2026-10-23)

## Baseline

Windows Alera now starts in shared-engine mode by default for Editor tabs. Every other tab type still uses the legacy process-backed detached window. The supported Editor path and `--alera-legacy-detached-tabs` fallback remain unchanged.

## Experimental Terminal handoff

The Terminal path can be enabled **only in a separately compiled test build** with the Dart define `ALERA_EXPERIMENTAL_SHARED_TERMINAL_TABS=true`. Its default is `false`. The define does not override `--alera-legacy-detached-tabs`, which continues to use the original child process for every tab. Git Diff, Git History, PDF, and Markdown remain on the original child-process route even when this define is enabled.

The opt-in reuses the existing shared root ProviderScope, TerminalRuntime and durable session handle. The source tab is hidden and unmounted before the child tab surface may mount. Native child close waits for a teardown frame before restoring the source. This is intended to prevent simultaneous attachment of the same TerminalView GlobalKey, focus node and PTY. A Terminal widget's disposal releases its visibility lease; it must not call closeTab or terminate the durable PTY.

## Test and validation

Prepare the worktree with `tool/development/prepare_windows_build_storage.ps1` first. Fetch the exact checked-in `third_party/xterm` and `third_party/dart_terminal` submodules before running `flutter pub get --offline` on a new worktree.

```powershell
flutter test --no-pub test/unit/shared_engine_detached_window_lifecycle_test.dart test/unit/shared_engine_detached_backend_test.dart test/unit/detached_tab_entrypoint_compile_test.dart
flutter test --no-pub test/widget/terminal_surface_test.dart --plain-name 'Terminal view handoff keeps its PTY and output history'
```

First isolated-worktree results (Windows, 2026-10-23): shared-engine lifecycle, startup, fallback and Quit-guard unit tests **21/21 passed**; TerminalSurface widget tests **45/45 passed**; native terminal-runtime and detached-tab shortcut tests **145 passed, 2 platform skips**. Whole-project Flutter analyze exited 0 with no errors when legacy warnings and infos were nonfatal. The two pre-existing TUI resume tests initially failed because they assumed hiding an ordinary tab pauses host delivery. Their fixture now explicitly backgrounds the entire app to exercise the actual pause/resume path; both pass again. These are unit and widget validations, not native detached Terminal GUI acceptance tests.

Windows Native Runner CTest **3/3 passed**. The first isolated Windows Debug executable also compiled successfully using `build_alera_in_worktree_debug.cmd` with the experimental Terminal define and an isolated support directory; the standard `flutter build windows` frontend could generate configuration, but local CMake compiler discovery required the documented Visual Studio + Ninja fallback.

### Real Windows Terminal GUI acceptance (isolated Debug, 2026-10-23)

- Start/stop the isolated executable once to create its SQLite schema. `seed_shared_editor_test_workspace.py --configuration Debug --reset-test-file --tab-kind terminal` creates a disposable `E2E Terminal` tab in that profile. The script remains backward-compatible with its default Editor fixture.
- Windows E2E PID **64180** initially owned one top-level `Alera Dev` window, with an interactive Windows PowerShell prompt and native DPI **120 (125%)**.
- Right-click the E2E Terminal tab, choose **Move to New Window**: the OS reported two real HWNDs, `Alera Dev` and `Alera - E2E Terminal`, **both inside PID 64180**, not a second process. The child displayed the live PowerShell session and retained prior output.
- Real desktop keyboard input in the child executed `echo SHARED_TERMINAL_OK` and displayed the matching output. Injecting raw `WM_CHAR` events to the native child was unreliable and produced duplicate characters; don't equate those synthetic events with real keyboard behavior.
- Posting native `WM_CLOSE` to the exact E2E child HWND returned to one top-level window; the main Terminal tab restored the existing scrollback, including `SHARED_TERMINAL_OK`. Real desktop input then executed `echo RETURN_PTY_ALIVE` in the restored main Terminal, with matching output. Thus the PTY remained interactive after native close and reattachment in this run.
- The real GUI screenshots and temporary data are stored only in the ignored per-worktree `build/windows/shared_gui_ninja/diagnostics` and `e2e_support` directories. Other running Alera processes and regular user data were not targeted.
- **Still not accepted:** repeated long-running Terminal detach/return cycles, large ongoing PTY output, real drag-back mouse gestures, Zhuyin IME, mixed DPI, clipboard/mouse-selection, multiple simultaneous child terminals, packaged Release AOT, and memory/handle leak analysis. One working cycle does not establish these properties.

For an isolated GUI build, keep the E2E App ID and support directory settings from `ALERA_PRODUCTION_INTEGRATION_HANDOFF.md` and additionally supply `--dart-define=ALERA_EXPERIMENTAL_SHARED_TERMINAL_TABS=true` during the Flutter compilation step. Merely passing a command-line argument to an already built EXE does not enable Terminal mode. Do not run experimental GUI tests against the real Alera user profile or modify an installed build.

## Safety limitations before promoting

- This opt-in is **not** production-ready and must not be merged into a production default without further validation.
- Basic command input and scrollback preservation across one real Windows ConPTY/terminal-host detach and native close have passed. Continue checking resize, Ctrl+C, Ctrl+S, background output, mouse selection and clipboard before promotion.
- Verify Taiwanese Zhuyin IME composition, mixed-DPI cursor hit tests, drag-back from two screens, native close while the shell is producing output, and multiple simultaneous Terminal windows.
- Monitor sustained terminal output and native handles/memory over repeated cycles; short unit tests cannot establish leak freedom.
- The `window_manager` plugin is main-HWND-oriented; never call global `windowManager` for child view title or close. Use the existing view-scoped native multiview controller.
- Native plugin and Flutter lifecycle integration tests, packaged Release AOT GUI, and a fallback acceptance run are required before enabling this path for users.
