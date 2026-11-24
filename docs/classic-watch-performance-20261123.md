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
4. **Quick Open:** Search-relevant watch events use a separate coalescing
   queue with a 120-ms quiet period and a 750-ms maximum wait. It schedules
   one immediate backend refresh per grouped batch, rather than continually
   resetting the backend debounce during a sustained build. Collapsed folders
   still reindex without reloading their Explorer tree rows.
5. **Watcher teardown:** Cancel stale streams with a 300-ms bounded wait and
   stop the native watch handle with a 500-ms bounded wait. Workspace
   generation checks prevent a late old restart from reinitializing the wrong
   tree. Native cleanup is best-effort if the platform does not respond.
6. **Isolated old I/O:** On Workspace replacement, the watcher queue retires
   its in-flight generation immediately. A blocked old Git read cannot delay
   the next Workspace's refresh, and its late completion cannot clear the new
   generation's in-flight state.
7. **Native projection isolation:** Reset the native projection serialization
   chain when rebuilding a different projection. An outstanding old native
   request does not delay the new workspace, and late native projection
   failures cannot disable the new workspace's native session.
8. **Regression tests:** Verify burst deduplication, bounded progress under
   sustained file changes, in-flight trailing batches, immediate watcher
   recovery, cancellation, unchanged notification suppression, manual refresh,
   rapid workspace switching, old Git/projection stalls, and watcher restart.

## Verification

- `flutter pub get --offline` completed using the project's existing lock.
- `flutter test --no-pub` across the four focused watcher/Git test files:
  **32/32 passed** on 2026-11-24, including a RED-to-GREEN regression for
  an old in-flight scan blocking the new workspace after `clear()`.
- Classic adjacent file service, editor watcher and Source Control tests:
  **41/41 passed**.
- Complete Explorer Widget suite **92/92 passed**, including native projection
  cancellation and a stalled previous workspace. A blocked native projection
  reproduced a `pumpAndSettle` timeout before the fix; it now passes and
  verifies the next workspace stays on the native delta path after old failure.
- The previous baseline on `main` independently reproduced an Explorer
  `pumpAndSettle` timeout when switching workspaces. A bounded native watch
  teardown fixed that timeout in the isolated candidate.
- Dart static analysis of the two modified production source files and
  the three changed test files completed in two smaller groups:
  **No issues found** (both exit 0).
- Formatter and Git diff check passed. The earlier six-file Dart analyzer
  request hit its 180-second execution limit without reporting diagnostics,
  but subsequent scoped analyzer runs passed; the former timeout is not
  counted as a clean result.
- The deterministic synthetic A/B benchmark still reports 200 -> 1 requests
  for an immediate burst and 300 -> 2 for a sustained 5-ms stream.
- No physical Windows GUI A/B was performed.

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

Windows build-storage `-CheckOnly` found this isolated checkout's
`.dart_tool` remains a regular directory under Dropbox, not the required
external build-storage junction. The remote execution boundary does not grant
write access to the designated `D:\alera-build` scratch drive. Do not compile
an unredirected Flutter native bundle under Dropbox or reuse unrelated EXEs
as a measured candidate. An explicitly authorized outside-Dropbox build
workspace or CI runner is required for real paired Windows GUI measurements.

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
