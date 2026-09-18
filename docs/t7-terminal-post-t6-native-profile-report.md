# T7 Post-T6 Native Windows Terminal Profile Report

Date: 2026-09-18
Branch: `perf/terminal-post-t6-native-profile`
Terminal architecture baseline: `c7947b42b623147465e1534844d340b3912d7152`
Native verification checkout: `6f4342cda57752836075d49370e4117feac09a72`
Status: **COMPLETE — T8d selected**

## Scope and baseline equivalence

T7 is the post-T6 evidence gate. It does not change production terminal behavior.

The native Windows rerun was executed from the repository root under `@home-node`, because that MCP process inherits the complete Windows environment and can build Flutter desktop targets successfully. The root checkout was at `6f4342cd`; comparison against `c7947b42` shows that only A3 validation docs/tests changed between those commits:

- `docs/a3-agent-overlay-unchanged-fastpath-handoff.md`
- `docs/rust-heavy-operation-parallel-work-plan.md`
- `integration_test/agent_runtime_overlay_benchmark.dart`

No terminal production file changed, so the native terminal measurements are representative of the T7 terminal baseline.

## Environment finding

The earlier `@home-rust` failure was not an incomplete Visual Studio/MSVC/Windows SDK installation. Under `@home-node` the same machine successfully built and launched the native Windows app.

`@home-node` exposes the standard Windows environment variables that were missing from the `@home-rust` child process, including `CommonProgramFiles`, `CommonProgramW6432`, `ALLUSERSPROFILE`, `APPDATA`, and `LOCALAPPDATA`.

Therefore the previous `Microsoft.Build.Utilities.FileTracker.InitializeCommonApplicationDataPaths()` illegal-path failure is classified as an `@home-rust` MCP process-environment problem, not a project/toolchain installation problem.

## Benchmark synchronization repair

The first successful native launch exposed a stale assumption in `integration_test/terminal_restore_benchmark.dart`.

The benchmark originally stopped restore timing correctly when `restoreProgress` became null, but then assumed one post-frame callback plus a fixed 50 ms delay was enough for the following 1 MiB live backlog marker to reach the emulator. Since `1f6d4b1d` serialized asynchronous parser-worker output applies, restore completion and subsequent live-output application are separate ordered steps.

The benchmark-only fix keeps the restore latency boundary unchanged, records that live backlog is still pending at restore-ready, then waits up to three seconds for the live marker before validating snapshot/live ordering. No production terminal code is changed.

## Required run status

| Required run | Samples | Result |
| --- | ---: | --- |
| `flutter test integration_test/terminal_restore_benchmark.dart -d windows` | 5 | **PASS** after benchmark synchronization repair |
| `flutter test integration_test/terminal_flush_cadence_benchmark.dart -d windows` | 5 | **PASS** |
| `flutter test test/benchmarks/terminal_production_worker_profile_benchmark.dart` | 5/scenario | **PASS** |
| `flutter test test/benchmarks/terminal_reveal_pipeline_profile_benchmark.dart` | 5/path | **PASS** |
| `flutter test test/benchmarks/terminal_streaming_render_profile_benchmark.dart` | 5 | **PASS** |

## Native Windows restore

Five measured samples:

```text
snapshot:                      2,560,000 bytes
ESC characters:                 420,000
live backlog:                 1,048,576 bytes
accepted median:                   1.72 ms
first chunk median:              145.69 ms
restore-ready median:          9,899.17 ms
restore-ready p95:            12,438.39 ms
restore-ready max:            12,438.39 ms
MAD:                            1,014.85 ms
3-second target:                     0 / 5
throughput:                         0.25 MiB/s
flushes/sample:               43, 43, 45, 43, 48
frames:                            1,381
slow frames:                       1,327
build median/p95:              0.28 / 29.20 ms
raster median/p95:           35.47 / 54.44 ms
```

This answers the first T7 question decisively: native snapshot restore is a material user-visible bottleneck. Median restore is about 9.9 seconds, more than 3x the 3-second target, and all five samples miss the target.

## Native Windows streaming/render cadence

Five measured samples:

```text
writes/s median / p95:        20.2 / 22.9
flushes/s median / p95:        8.0 / 9.3
frames/s median:              15.9
build median / p95:           6.25 / 103.22 ms
raster median / p95:         58.61 / 84.32 ms
total frame median / p95:    97.23 / 170.32 ms
jank frames/sample:           51, 39, 48, 54, 41
RSS delta median / p95:       7.07 / 22.40 MiB
```

The native Windows renderer therefore confirms the portable `flutter_tester` raster-tail signal. Rendering is materially slow, but its frame-scale cost is still far smaller than the multi-second restore replay.

## Portable worker / reveal evidence

The post-T6 portable evidence remains:

- production hidden-backlog reveal wall median: **640.63 ms**;
- packed reveal end-to-end median: **431.22 ms**;
- UI packed decode/materialization: **226.55 ms**;
- replica apply: **156.86 ms**;
- worker materialization: **53.81 ms**;
- isolate transfer/scheduling: **0.82 ms**.

This still shows a useful T8a opportunity inside reveal, and native renderer evidence also justifies future renderer work. Neither is the largest user-visible bottleneck found by T7.

## T7 interpretation

1. **Is native restore a material user-visible bottleneck?**
   Yes. Median **9.90 s**, p95 **12.44 s**, target hit **0/5**.

2. **Does native Windows confirm the raster-tail signal?**
   Yes. Native raster median/p95 is **58.61/84.32 ms**, with total-frame median/p95 **97.23/170.32 ms** and 39–54 jank frames per sample.

3. **What dominates packed reveal?**
   Portable packed reveal remains UI decode/materialization (**226.55 ms**) > replica apply (**156.86 ms**) > worker materialization (**53.81 ms**) >> isolate transfer (**0.82 ms**).

4. **Which single T8 cut is justified?**
   **T8d — direct worker snapshot hydration / restore redesign.** The native restore path is an order of magnitude more expensive than the remaining packed reveal stages and much larger than a single renderer frame tail.

## Decision

T7 is complete.

The single next terminal architecture cut selected by the documented decision tree is:

> **T8d: direct worker snapshot hydration / restore redesign**

T8a and T8c remain valid later opportunities, but should not be mixed into the first T8 implementation. T8d should preserve ordered live output, pointer/input catch-up semantics, restore-progress behavior, and the benchmark's separate restore-ready versus live-backlog assertions.

No production terminal implementation file was changed by T7.
