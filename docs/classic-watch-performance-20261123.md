# Alera Classic file-watch responsiveness — 2026-11-23

## Motivation

Classic already starts the two independent Rust engines and window/SQLite
bootstrap concurrently (see `docs/startup-performance-20261122.md`), but
startup latency and interaction latency are distinct. This change targets
redundant background work while using the Classic Flutter workbench.

## Changes

1. **Explorer notifications:** Merge watch batches within 120 ms, deduplicate
   directory paths, and allow only one Git/directory reload at a time. If
   updates arrive while a reload is running, coalesce a trailing reload
   instead of starting overlapping scans. Watcher reconnect/overflow performs
   an immediate reconciliation.
2. **Workspace safety:** Cancel scheduled directory reads when the workspace
   changes or Explorer disposes, reject late watcher reads by generation, and
   prevent a stale Git snapshot from replacing new workspace badges.
3. **Source Control state:** Skip emitting an identical Git snapshot from
   the native watcher. An explicit refresh still publishes the normal action
   and completion states; changed entries and metadata still notify listeners.
4. **Regression tests:** Verify burst deduplication, in-flight trailing
   batches, immediate watcher recovery, cancellation, unchanged notification
   suppression, manual refresh and watcher restart behavior.

## Verification

- `flutter pub get --offline` completed using the project's existing lock.
- `flutter test --no-pub` across the four focused watcher/Git test files:
  **29/29 passed**.
- Follow-up focused queue/diffing tests after lint cleanup:
  **11/11 passed**.
- Dart static analysis: run on changed source/tests before integration.
- `git diff --check` should remain clean.

## Required before merging into main or switching a running build

- Use the same real Windows workspace, project count, loaded Explorer folders
  and live Agent processes to compare Classic before/after with repeat runs.
  Measure process CPU, memory, file-system scan counts and UI frame delays
  while a build/formatter generates large event bursts.
- Verify Explorer tree badges and Quick Open results update after normal edits
  and external Git operations. Verify manual refresh and watcher recovery.
- Switch workspaces during a deliberately slow Git scan and verify no stale
  Explorer content or badges cross workspaces.
- Exercise long-running file churn: updates should converge after the burst
  without runaway CPU use.
- This change does **not** establish a measured percentage improvement.
  No live Alera or runtime Host process was restarted for these tests.
