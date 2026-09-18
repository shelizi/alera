# Post-T8d Native Terminal Restore Profile

Date: 2026-09-18

## Scope

This profile starts from T8d commit `b8144755` and measures the remaining native Windows terminal snapshot restore cost after direct parser-worker snapshot hydration removed the old UI replay loop.

The profiling instrumentation is opt-in. Production `XtermTerminalRuntime` keeps `snapshotHydrationProfilingEnabled = false` and therefore continues to use the original T8d `hydrateSnapshotBufferDelta` path. The Windows benchmark explicitly enables profiling.

## Measurement breakdown

The benchmark now records:

- parser-worker queue/startup;
- xterm snapshot parse;
- packed full-buffer materialization;
- isolate transfer/scheduling;
- UI-side packed delta decode;
- UI replica apply;
- total hydration;
- post-hydration/framework-frame tail.

The benchmark ordering assertion was also corrected. Final buffer text offsets are not a valid command-order oracle because cursor/scrollback state can place later live output before the snapshot marker in the final visible buffer. The benchmark now uses per-run unique live markers plus parser-worker revision progression to prove that the snapshot revision completed before the live-output revision.

## Clean native Windows results

Workload:

- snapshot: 2,560,000 bytes;
- 420,000 ESC characters;
- live backlog: 1,048,576 bytes;
- five measured runs per invocation.

### Clean run A

- framework post-frame median: **1606.06 ms**
- p95/max: **2161.52 ms**
- 3 s target: **5/5**
- worker parse median: **945.69 ms**
- packed materialize median: **244.02 ms**
- transfer/scheduling median: **3.48 ms**
- decode median: **102.51 ms**
- UI apply median: **63.82 ms**
- total hydration median: **1580.50 ms**
- post-hydration median: **11.19 ms**

### Clean run B

- framework post-frame median: **1595.37 ms**
- p95/max: **2212.17 ms**
- 3 s target: **5/5**
- worker parse median: **921.31 ms**
- packed materialize median: **69.13 ms**
- transfer/scheduling median: **3.65 ms**
- decode median: **395.85 ms**
- UI apply median: **194.98 ms**
- total hydration median: **1527.79 ms**
- post-hydration median: **137.89 ms**

Across these two clean runs, all ten measured samples completed inside the 3 second restore target.

## Interpretation

The dominant stable phase is now **worker-side xterm snapshot parsing**, about 0.92-0.95 seconds median in both clean runs.

Packed materialization, UI decode, and UI apply move substantially between runs, which indicates scheduler/GC/cache sensitivity, but none is as consistently dominant as worker parsing. Renderer/post-frame work is no longer the primary bottleneck.

Therefore the next terminal architecture cut should not be T8a, T8b, or T8c yet.

## Selected next cut: T8e

**T8e - worker snapshot parse fast path / structured retained snapshot**

Goal: avoid reparsing the full ANSI snapshot through xterm when a retained/structured snapshot can restore equivalent terminal state more directly.

Investigate in this order:

1. define the minimum structured terminal state needed to reconstruct scrollback, viewport rows, cursor, modes, title/hyperlink state, and revision;
2. determine whether the durable sidecar or worker can persist/transport that state without round-tripping through ANSI text;
3. prototype direct worker model hydration from structured rows/state;
4. preserve the current ANSI snapshot path as compatibility/fallback;
5. compare five-sample native Windows restore latency, memory, payload size, and correctness against T8d;
6. only revisit T8a/T8b/T8c if profiling after T8e shows materialization/apply/render becoming dominant.

Correctness gates must preserve:

- snapshot/live command ordering;
- alternate-screen/main-buffer semantics;
- cursor and scrollback state;
- interaction-mode reset behavior;
- hyperlinks/styles/combining characters/wide cells;
- stale generation cancellation;
- pointer-input suspension until hydration completes;
- fallback compatibility with existing textual snapshots.

## Host-load outlier

A later profiling invocation on the same code path produced **7849.44 ms median / 11745.02 ms p95**, with worker parse at **4814.10 ms**. The host was simultaneously showing abnormal build/system load, and a subsequent confirmation attempt remained in Windows build for more than 468 seconds without reaching the test and was cancelled.

That run is retained as contention evidence but is **not used as the clean baseline**. It does show that full ANSI parsing remains CPU-sensitive under contention, reinforcing T8e's direction.

## Validation

Passed before final documentation:

- `flutter test --no-pub test/unit/terminal_runtime_native_test.dart` - 135 passed, 2 Windows-expected skips;
- `flutter test --no-pub test/unit/terminal_xterm_worker_test.dart` - 20 passed;
- focused production-path direct snapshot hydration test;
- focused profiled worker hydration test;
- targeted `dart analyze` - clean;
- `git diff --check --ignore-submodules=all` - clean.

## Decision

T8d successfully changed the restore architecture enough that clean native Windows measurements now satisfy the 3 second gate. The next worthwhile optimization target is **T8e worker snapshot parse elimination/reduction**, not renderer work and not UI replay.