# Classic Terminal input trace allocation — 2026-11-24

## Context

The PTY input path called `traceTerminalInput` at three sites for every
input, rejected write, and deferred-startup buffer flush. The trace string
included `data.codeUnits`, which materializes/prints a UTF-16 unit list.
The current bundled xterm input trace sink is a no-op unless the build
explicitly sets `ALERA_E2E_SUPPORT_DIR`, but the string interpolation ran
*before* the sink was called even in normal builds.

For a large Unicode paste the unnecessary per-input code-unit list can be
much larger than the original text. The input still needs UTF-8 encoding to
reach the PTY; only the diagnostic conversion is redundant.

## Change

- Add one compile-time `String.fromEnvironment('ALERA_E2E_SUPPORT_DIR')`
  constant to gate all three diagnostic string constructions.
- Preserve the existing exact diagnostic text and xterm trace sink when the
  E2E support directory is supplied. No change to regular terminal output,
  UTF-8 PTY writes, startup buffering, focus or input scheduling.
- Add a Unicode paste regression using the real terminal session controller
  and an injected PTY stub: thousands of multi-byte characters must still
  reach the PTY byte-for-byte unchanged.
- Leave the bundled xterm Git submodule pinned; no third-party edits.

## Verification

- `flutter test --no-pub test/unit/terminal_runtime_native_test.dart`:
  **144 passed, 2 skipped, 0 failed**, including the new large Unicode PTY
  paste regression and existing buffered input, output and parser-worker tests.
- `flutter test --no-pub test/widget/terminal_surface_test.dart`:
  **64/64 passed**, including Terminal/Composer input, tab handoff and code
  copy-overlay regressions.
- `dart format --output=none --set-exit-if-changed`:
  2 Dart files, 0 changes; `git diff --check`: passed.
- Targeted static analyzer not rerun here: previous standalone analyses of
  large Workbench `part` files repeatedly timed out without diagnostics.
  Successful Flutter tests compile the modified source but do not substitute
  for a clean full-project analyzer result.
- This is a source-level elimination of disabled logging work, **not** a
  measured Windows GUI CPU, latency or memory improvement.
