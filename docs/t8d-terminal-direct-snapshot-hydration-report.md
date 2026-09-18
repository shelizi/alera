# T8d Direct Worker Snapshot Hydration Report

Date: 2026-09-18

## Scope

T8d is the single terminal architecture cut selected by T7 after the native Windows restore gate measured a 9.90 s median / 12.44 s p95 snapshot restore. The goal was to remove the frame-budgeted 16-64 KiB restore replay loop and hydrate a rebuilt terminal directly from parser-worker state without mixing in the deferred T8a/T8c renderer/materialization work.

Base commit:

- e59bf29ad3cce7b14dc2a1273ec5bb466bc6fdfe (merge: close T7 native terminal gate)

Branch/worktree:

- perf/terminal-t8d-snapshot-hydration
- .worktrees/terminal-t8d-snapshot-hydration

## Implementation

The parser worker now owns the snapshot rebuild path:

1. reset the parser-worker generation and detach the previous replica;
2. parse the complete snapshot once inside the worker-owned xterm;
3. serialize one packed full-buffer delta;
4. hydrate the fresh UI replica directly from that packed buffer;
5. preserve restore progress, pointer suspension, interaction-mode reset, and snapshot-before-live ordering.

The old frame-budgeted restore queue remains the fallback for non-worker paths. T8d does not introduce a second parser implementation and does not move normal live output out of the existing ordered pump.

Two lifecycle races exposed by the faster restore path were also fixed:

- snapshot replacement invalidates a stale async pump write immediately, while late completion from the old generation cannot clear or mutate the new generation's in-flight state;
- parser-worker commands re-check generation/terminal identity before worker startup, after startup, and on error, so commands invalidated by snapshot replacement become no-ops instead of starting stale workers.

The direct path also publishes a completed restore-progress state before clearing the overlay, preserving the existing progress/benchmark contract.

## Regression coverage

Focused coverage includes:

- direct packed full-buffer snapshot hydration;
- 512 KiB worker snapshot hydration;
- fresh parser generation after snapshot replacement;
- snapshot-before-live ordering;
- interaction-mode reset;
- pointer suspension/release;
- stale in-flight output invalidation;
- stale parser-worker command invalidation;
- existing restore fallback, remint, overflow, reveal, exit, and durable-session behavior.

Final validation:

- flutter test --no-pub test/unit/terminal_runtime_native_test.dart
  - PASS: 135 tests
  - 2 platform skips
- flutter test test/unit/terminal_xterm_worker_test.dart
  - PASS: 20/20
- focused dart analyze on all T8d production/test files
  - PASS
- git diff --check --ignore-submodules=all
  - PASS

A prior parallel Flutter-test attempt hit a generated build/test_cache PathExistsException; after deleting only that generated cache and rerunning the runtime suite serially, all tests passed. This was a test-runner cache collision, not a product regression.

## Native Windows five-sample result

Command:

flutter test --no-pub integration_test/terminal_restore_benchmark.dart -d windows

T7 baseline:

- framework-ready median: 9.90 s
- framework-ready p95: 12.44 s
- <= 3 s target: 0/5

T8d:

- framework post-frame median: 3337.04 ms
- p95: 5904.17 ms
- max: 5904.17 ms
- MAD: 1623.02 ms
- <= 3 s target: 1/5
- throughput: 0.73 MiB/s
- flush counts: 1, 1, 2, 1, 2
- 222 measured frames, 74 slow frames
- build median/p95: 0.34 / 83.16 ms
- raster median/p95: 1.08 / 34.14 ms

Relative to T7, the median restore path is about 66% lower and p95 about 53% lower.

## Interpretation

T8d succeeds at its architectural objective: the snapshot is no longer replayed through the UI-side frame-budgeted restore queue, and the native Windows five-sample result is materially better.

The 3 s target is not fully closed. Only 1/5 samples are within target and the remaining variance is still large. That remaining cost should not be hidden by extending T8d into T8a/T8c. The next terminal step should first re-profile the post-T8d path and identify whether the remaining time is worker parse/full-buffer serialization, UI packed materialization/replica apply, or renderer work before selecting another architecture cut.

## Boundary

T8d deliberately does not:

- redesign the packed row/cell representation;
- introduce a dirty-row renderer seam;
- replace the live-output pump;
- loosen snapshot/live ordering;
- relax the native benchmark assertions to manufacture a pass.

The benchmark retains failure-only diagnostics for pending live output, pump in-flight state, parser revision, and flush counts to make any future recurrence diagnosable.