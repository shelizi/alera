# Alera startup responsiveness — 2026-11-22

## Problem and baseline

Startup responsiveness is affected by synchronous initial work and a burst of
independent background I/O. No valid **before/after Windows GUI cold-start
timings** were recorded during this code-only change, so percentage savings
must not be inferred from compile or unit-test durations.

## Changes

- **GPUI:** Reopen authenticated Host Terminal inventory immediately. Schedule
  saved file-backed Editor hydration 150 ms after window creation rather than
  synchronously in the initial Workbench constructor. Preserve pane layout
  while Editor buffers are pending, and cancel stale restoration after workspace
  switches. Scratch tabs retain their existing restoration contract.
- **GPUI:** Stage project catalog, Git status and worktree metadata after
  180 ms. Stage Agent Profiles, installed Agent discovery, favorites and MCP UI
  after a further 220 ms. All delayed jobs verify workspace generation and
  still use their existing cancellation/refresh gates. No new polling loop.
- **GPUI:** Git remote URL discovery now runs on a background executor after the
  first paint rather than spawning a Windows Git process before window creation.
  Opening the PR panel still performs its explicit fresh lookup.
- **Classic:** The two independent Rust engines and window/SQLite bootstrap
  run concurrently via `initializePrimaryStartupDependencies` rather than in
  three sequential waits. They must all settle before `runApp`; if an engine
  fails after SQLite was opened, the window database is closed before throwing.
- **Instrumentation:** Existing `--dart-define=ALERA_PERF_TRACE=true` now
  reports separately measured Classic engine/window/terminal phases. Opt-in
  `ALERA_GPUI_STARTUP_TRACE=1` prints stages
  `workspace_model_initialized`, `preview_session_restored`,
  `settings_and_cli_discovered`, `fonts_and_theme_loaded`, and
  `native_window_created`. Native window creation is **not** a reliable
  first-paint timestamp; the Flutter first-frame marker is separate.
  No private workspace paths, prompt or transcript data are logged.

## Acceptance checklist

Run against the same workspace and saved layout, once from a cold Windows
process and once from a warm launch; repeat three times per revision:

1. Measure process start to visible window, to usable Editor input, and to
   attached existing Agent output. Compare medians, not best-case runs.
2. Restore a workspace with 20 file Editor tabs, pinned/secondary split tabs,
   and at least one running Claude/Codex PTY. Ensure preserved uncommitted
   edits, selected tabs, undo/redo, no unexpected Agent spawn or PTY reset.
3. Switch workspace during the first 150 ms and ensure no old worktree Editor
   opens in the new one. Verify transient Host inventory cannot consume
   pending split-pane layout.
4. Open Git, Pull Requests and Agent Profiles after startup and verify their
   eventually-loaded data; test explicit refresh and Host-offline errors.
5. Compare startup read/write I/O and sustained CPU/RAM after five minutes.
   The new scheduling must not create extra periodic background jobs.

## Status

GPUI full binary regression tests: **84/84 PASS** after the final
noncritical Git remote lookup deferral; final compilation also passes. Classic main-window
startup concurrency, error cleanup, detached Tab and Workbench gate regression
suite: **9/9 PASS** using exact locally pinned Flutter submodules. New Classic
helper/tests are statically valid; the existing `main.dart` Riverpod lint emits
three preexisting `missing_provider_scope` warnings in its multiwindow bootstrap.
Physical GUI timing and user-visible verification remain pending; do not infer
a percentage reduction. No production Host restart, app deployment or main
merge is part of this change.
