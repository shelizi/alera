# Alera Classic file-watch responsiveness — 2026-11-23

## Motivation

Classic already starts the two independent Rust engines and window/SQLite
bootstrap concurrently (see `docs/startup-performance-20261122.md`), but
startup latency and interaction latency are distinct. This change targets
redundant background work while using the Classic Flutter workbench.

## Changes

1. **Explorer notifications:** Merge watch batches within 120 ms, deduplicate
   directory paths, and allow only one Git/directory reload at a time. Bound
   continuous event storms with a 750-ms maximum wait so the tree remains
   responsive during long builds and formatters. Events arriving during a
   running read coalesce into one trailing reload. Watcher reconnect/overflow
   performs an immediate reconciliation.
2. **Workspace safety:** Cancel scheduled directory reads when the workspace
   changes or Explorer disposes. Reject late directory listings, Git snapshots
   and watcher reads by projection/workspace generation, preventing old results
   from replacing new workspace badges or file rows.
3. **Source Control state:** Skip emitting an identical Git snapshot from
   the native watcher. An explicit refresh still publishes the normal action
   and completion states; changed entries and metadata still notify listeners.
4. **Quick Open:** One watcher batch schedules one debounced index refresh
   rather than triggering an immediate index refresh for each changed folder.
   Explicit file operations still refresh immediately while filtering.
5. **Regression tests:** Verify burst deduplication, bounded progress under
   sustained file changes, in-flight trailing batches, immediate watcher
   recovery, cancellation, unchanged notification suppression, manual refresh
   and watcher restart behavior.

## Verification

- `flutter pub get --offline` completed using the project's existing lock.
- `flutter test --no-pub` across the four focused watcher/Git test files:
  **31/31 passed** on 2026-11-24 after adding continuous-event tests.
- Previous focused queue/diffing tests: **11/11 passed**.
- Dart static analysis of changed source/tests: **no errors or warnings**;
  rerun after final changes.
- `git diff --check` should remain clean.

## Reproducible synthetic scheduling comparison (2026-11-24)

Run `dart run tool/development/benchmark_classic_explorer_watch.dart`.
This directly compares the previously immediate per-event drain model with
the new debounced and bounded refresh queue, using fake time and successful
in-memory asynchronous reads on the same event input.

| Event input | Previous scan requests | Candidate scan requests | Previous directory reads | Candidate reads |
| --- | ---: | ---: | ---: | ---: |
| 200 immediate events | 200 | 1 | 400 | 2 |
| 300 events 5 ms apart | 300 | 2 | 600 | 4 |

**Important:** These are deterministic counts of simulated backend requests,
not measurements of Windows physical file I/O, CPU, latency, memory or FPS.
The pre-change model assumes scans complete between event deliveries. No
percentage of real user-visible speed-up should be inferred.

Windows read-only check: 29 `alera.exe` processes were present, but the only
process exposing an application window was the running GPUI PID 50612. No
Classic GUI candidate or matched baseline/current binary pair was available
for real A/B acceptance. No live process was restarted or terminated.

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
