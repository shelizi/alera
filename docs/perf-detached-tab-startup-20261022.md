# Windows detached Tab: drag feedback and startup (2026-10-22)

## Root causes

1. Flutter's `Draggable.feedback` cannot paint outside the native Alera HWND.
   Once the pointer crosses the window boundary, the tab ghost disappears,
   even though the drag continues.
2. The detached process originally awaited logging, two Rust libraries,
   Ghostty initialization, SQLite and window-manager setup before `runApp`.
   The Windows runner only shows a new HWND on the **first Flutter frame**,
   so the entire initialization time looked like an unresponsive drag.
3. The parent only notified tab listeners when the child sent `READY`, not
   when launching. The original tab looked unchanged while waiting.
4. The existing 45-second READY timeout could expire after a slow child had
   already painted its own window, creating an orphaned late child.

## Implementation

- A small, native topmost, non-activating, click-through Windows preview
  follows the cursor outside the Alera HWND. It carries the tab title and
  explains that releasing will open a window. Inside Alera, the existing
  Flutter drag preview and tab-strip drop behavior remain unchanged.
  The preview is hidden/stopped on drag end or runner teardown.
  Preview positioning clamps to the nearest monitor's work area.
- The child receives its one-time authenticated descriptor, initializes
  window management, and renders an `Opening tab...` Flutter scene **before**
  Rust/SQLite initialization. The actual widget is rendered only after those
  engines are ready. `READY` still means the actual content's first frame
  completed, so the parent never hides the original tab prematurely.
- Database startup overlaps Rust initialization. CodeForge Rust is skipped for
  Terminal, Markdown, PDF and Git History windows; it is still initialized
  for editor and Git Diff.
- The parent notifies pending-state changes immediately; the source chip shows
  a progress indicator while launching. After the child sends `LOADING` on
  its first frame, a bounded 2-minute initialization deadline replaces the
  normal 45-second handshake deadline. `FAILED` ends a bad initialization
  promptly, keeping the original tab visible.

## Verification

The targeted Flutter test command is:

```sh
flutter test --no-pub test/unit/detached_tab_window_service_test.dart \
  test/unit/detached_tab_drag_preview_test.dart \
  test/unit/detached_tab_entrypoint_compile_test.dart \
  test/widget/detached_tab_startup_test.dart \
  test/widget/detached_tab_save_shortcuts_test.dart \
  test/unit/detached_tab_child_compile_test.dart \
  test/unit/workbench_tab_strip_drop_test.dart
```

Result: **28 passed, 0 failed**. A pre-fix regression reproduced that pending
detached launches did not notify listeners. Flutter tests validate Dart,
protocol and widgets, but cannot validate the Win32 preview itself.

`flutter build windows --debug --no-pub` did not reach source compilation.
The installed VS 2026/CMake `CompilerIdCXX` probe fails in preexisting MSBuild
`MSB4018` / `Microsoft.Build.Utilities.FileTracker` path normalization.
The C++ preview still needs a successful native build and real desktop smoke
test before merging this branch to `main`.

The Dart analyzer invocation timed out; it has not been counted as passed.

## Manual checks / performance measurements

1. Start with one normal editor tab. Drag across the Alera HWND boundary to
   another monitor (including a different resolution and negative-origin
   monitor); the native bubble should remain visible and should not intercept
   mouse input. Drag back and reorder/split normally; the ghost must vanish.
2. Release outside the HWND. Confirm the original tab displays its spinner,
   the new HWND paints `Opening tab...` promptly, and the original tab is
   hidden only after the actual content appears.
3. Repeat with terminal, Markdown, PDF, editor, Git Diff and Git History tabs.
   Check Ctrl+S, drag-back, close/reopen and a deliberately failed launch. The
   parent's original tab must stay recoverable on failure.
4. In an environment where Windows native builds work, enable
   `--dart-define=ALERA_PERF_TRACE=true` and compare child trace stamps
   `detached_preview_first_frame`, `detached_alera_rust_init`,
   `detached_code_forge_rust_init`, `detached_database_open` and
   `detached_content_first_frame`. Also time multiple actual Windows
   cold/warm window launches. **No runtime speedup is claimed without those
   measurements.**
