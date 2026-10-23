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

For an isolated GUI build, keep the E2E App ID and support directory settings from `ALERA_PRODUCTION_INTEGRATION_HANDOFF.md` and additionally supply `--dart-define=ALERA_EXPERIMENTAL_SHARED_TERMINAL_TABS=true` during the Flutter compilation step. Merely passing a command-line argument to an already built EXE does not enable Terminal mode. Do not run experimental GUI tests against the real Alera user profile or modify an installed build.

## Safety limitations before promoting

- This opt-in is **not** production-ready and must not be merged into a production default without further validation.
- Verify with real Windows ConPTY/terminal-host sessions that command input, resize, Ctrl+C, Ctrl+S, background output, scrollback, mouse selection and clipboard continue to work across detach and return.
- Verify Taiwanese Zhuyin IME composition, mixed-DPI cursor hit tests, drag-back from two screens, native close while the shell is producing output, and multiple simultaneous Terminal windows.
- Monitor sustained terminal output and native handles/memory over repeated cycles; short unit tests cannot establish leak freedom.
- The `window_manager` plugin is main-HWND-oriented; never call global `windowManager` for child view title or close. Use the existing view-scoped native multiview controller.
- Native plugin and Flutter lifecycle integration tests, packaged Release AOT GUI, and a fallback acceptance run are required before enabling this path for users.
