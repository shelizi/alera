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

## Reproducible Windows A/B measurement

Use tool/development/measure_windows_startup.ps1 to alternate three baseline/candidate launches with an isolated ALERA_GPUI_STATE_DIR for each. An optional **sanitized** StateTemplateDir is copied anew per trial; no production state is read or modified by default. It records binary SHA-256 hashes, per-run top-level window visibility, Windows message-loop responsiveness, CPU, working/private memory and process read/write bytes in JSON under a ScratchRoot outside Dropbox.

Build exact revisions f3d55c7ab (baseline) and d2e35a645 (candidate) with the same Rust toolchain, Cargo profile, dependencies and Windows SDK. From each checkout run tool/development/prepare_windows_build_storage.ps1 in the same PowerShell session before cargo build --manifest-path desktop_gpui/Cargo.toml --locked -j 4. Find the configured target directory using cargo metadata --manifest-path desktop_gpui/Cargo.toml --no-deps --format-version 1. Copy the baseline EXE to a distinct location on the approved local build-storage drive before building the candidate.

~~~powershell
$baseline = 'D:\alera-build\startup-benchmark-20261122\baseline.exe'
$candidate = 'D:\alera-build\startup-benchmark-20261122\candidate.exe'
$workspace = 'E:\example-workspace'
$stateTemplate = 'D:\alera-build\startup-benchmark-20261122\sanitized-state'
pwsh -File tool/development/measure_windows_startup.ps1 -BaselineExe $baseline -CandidateExe $candidate -WorkspacePath $workspace -ScratchRoot 'D:\alera-build\startup-benchmark-20261122' -StateTemplateDir $stateTemplate -Runs 3 -ObservationSeconds 300
~~~

The script closes only test-launched GPUI processes, never existing Agent/Host processes. Do not run alongside another interactive Alera Native instance. MainWindowHandle and Process.Responding are **proxies, not actual first paint or usable Editor/Agent readiness**. Manually verify 20 restored file tabs, split/pinned tabs, Editor input, stale-workspace suppression and native Claude/Codex historical dialogue. Process-cold launches are not reboot-level cold starts.

**Environment blocker (2026-11-22):** MCP policy prohibits subprocess writes outside its Workspace, whereas this project's Windows build-storage policy requires compilation outside Dropbox. The exact version-matched executable pair could not be built here (WORKSPACE_PATH_PROTECTED). The PowerShell script parser and read-only PlanOnly flow passed; GUI before/after measurements have **not** been performed. Do not claim acceleration or merge based solely on static/unit tests.

A separate three-second sample found 20 older alera.exe CLI/Runtime Host sidecar processes consuming approximately 767 MB combined working set and 2.1% of one CPU core, not 20 GUI instances. Keep them running until ownership is established, and record them as background-load context during later trials.

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
