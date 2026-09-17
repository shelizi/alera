# T5 Production Terminal Worker / Render / Restore Profile Report

Date: 2026-09-17
Branch: `perf/terminal-production-profile`
T5 baseline: `f5e0cb2ffafd82e5a95d8c6fc8f0ae2fa022482c`
Latest `main` checked after measurement: `8696d64e0c0fc21154797844449eef0868ce2d6b`

## Scope

T5 is an evidence-only gate. It profiles the latest production xterm parser-worker / UI replica / renderer / restore paths and does **not** change production terminal behavior while T4 owns terminal production implementation.

The post-measurement main advance from `f5e0cb2f` to `8696d64e` contains A3 agent-overlay work plus S1 Rust cold-initialization profiling. Neither changed terminal source or terminal benchmark files, so the T5 measurements remain representative of the same terminal implementation.

## Benchmark additions

- `test/benchmarks/terminal_production_worker_profile_benchmark.dart`
  - Uses the shipped `TerminalXtermWorker` and `TerminalXtermReplicaTerminal` classes directly.
  - Five fresh-worker samples per scenario.
  - Separates worker/isolate roundtrip from UI replica apply time.
  - Records request count, input bytes, changed rows/cells, full repaint count, maximum buffer rows, RSS movement, and a deterministic logical delta-payload byte proxy.
  - The payload number is **not** the actual Dart isolate serialization byte count; the isolate API does not expose that value. It is intended for stable before/after comparison.

- `test/benchmarks/terminal_streaming_render_profile_benchmark.dart`
  - Uses `XtermTerminalRuntime(parserWorkerEnabled: true)` and the production session view under `flutter_tester`.
  - Five samples of sustained streaming output.
  - Records write/flush/frame rate, build/raster/total frame timings, jank count, and RSS movement.
  - This is useful comparative renderer evidence, but it is **not** a Windows desktop GPU benchmark.

- `integration_test/terminal_flush_cadence_benchmark.dart`
  - Updated to explicitly enable and assert the production parser worker and collect five comparable samples when run on a native desktop target.

- `integration_test/terminal_restore_benchmark.dart`
  - Updated to explicitly enable and assert the production parser worker so the existing snapshot replay benchmark measures the current production architecture on a native desktop target.

## Worker / delta / replica results

Command:

```text
flutter test test/benchmarks/terminal_production_worker_profile_benchmark.dart
```

All scenarios use five measured samples.

| Scenario | Wall median / p95 | Worker roundtrip median / p95 | Replica apply median / p95 | Full repaints | Logical delta proxy | Key observation |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| sustained compiler/log output | 284.23 / 295.37 ms | 213.52 / 225.42 ms | 18.23 / 23.76 ms | 0 | 32,910,232 B | regular streaming is worker-dominated, but no full repaint |
| bursty agent output | 82.01 / 97.54 ms | 67.18 / 76.72 ms | 4.83 / 7.54 ms | 0 | 10,133,282 B | replica apply remains small relative to worker roundtrip |
| full-screen TUI repaint | 40.96 / 45.33 ms | 28.70 / 32.35 ms | 2.61 / 4.21 ms | 0 | 6,003,852 B | current incremental path avoids full-repaint protocol here |
| synchronized-update bursts | 58.69 / 69.75 ms | 43.19 / 55.83 ms | 5.21 / 6.09 ms | 0 | 8,090,120 B | synchronized bursts do not trigger full repaint |
| deep scrollback + ongoing output | 39.08 / 44.54 ms | 30.20 / 37.52 ms | 1.83 / 2.65 ms | 0 | 5,768,490 B | 9,241-row buffer does not make normal streaming O(history) in this fixture |
| hidden terminal output | 9.06 / 12.77 ms | 5.64 / 8.75 ms | 0.11 / 0.57 ms | 0 | 0 B | hidden-output suppression is working: no UI delta payload |
| reveal after large hidden backlog | 1,262.06 / 1,376.90 ms | 1,114.01 / 1,249.15 ms | 35.22 / 69.82 ms | 1 | 92,072,420 B | one reveal materializes an 8,001-row full repaint; worker/delta work dominates |
| resize storm | 1,689.48 / 2,296.47 ms | 1,383.19 / 1,890.88 ms | 121.42 / 149.12 ms | 40 | 141,510,848 B | all 40 resizes produce full repaint; largest measured payload and latency |

Additional deterministic counts from the same run:

- sustained compiler/log output: 2,837 changed rows / 442,755 changed cells.
- reveal after hidden backlog: 8,001 changed rows / 1,238,175 changed cells; median RSS delta 41.20 MiB, p95 167.59 MiB.
- resize storm: 19,240 changed rows / 1,899,204 changed cells; median RSS delta 16.41 MiB, p95 44.08 MiB.
- hidden output: zero changed rows/cells and zero logical delta payload across all five samples.

### Interpretation

The old generic concern that every normal output update is effectively O(history) is **not supported** by this latest-production profile. Deep scrollback with ongoing output remained around 39 ms median and did not trigger full repaint.

The material worker-side cost is concentrated in structural events that currently force whole-buffer state transfer: reveal after a large hidden backlog and repeated resize. In both cases, UI replica apply is much smaller than worker/isolate roundtrip, so T6a (optimizing replica apply first) is not supported by this evidence.

The benchmark cannot split worker parse/mutation, delta construction, Dart object allocation, and isolate serialization into independent timings without adding instrumentation inside production worker code. T5 intentionally does not do that while T4 owns the terminal implementation. The combination of full-repaint count, logical payload size, and worker roundtrip is nevertheless sufficient to identify the protocol/full-state path as the primary measured target.

## Renderer / frame results

Command:

```text
flutter test test/benchmarks/terminal_streaming_render_profile_benchmark.dart
```

Five-sample `flutter_tester` result:

```text
writes/s median:        41.5   (p95 61.0)
flushes/s median:       14.3   (p95 18.3)
frames/s median:        31.5
build median / p95:     0.69 / 13.89 ms
raster median / p95:   12.91 / 79.51 ms
total median / p95:    15.46 / 109.22 ms
jank frames/sample:    39, 29, 27, 28, 39
RSS delta median / p95: 8.39 / 12.40 MiB
```

This shows a real long-tail rendering cost under sustained output in the production widget path: build work is usually small while raster/total-frame p95 is large. However, because `flutter_tester` is not the native Windows desktop compositor/GPU path, these values should be treated as a comparative signal rather than an absolute end-user latency claim.

## Restore evidence gap

The native production restore benchmark remains:

```text
flutter test integration_test/terminal_restore_benchmark.dart -d windows
```

This host can discover the Windows Flutter device, but native integration build stops before test execution because CMake reports:

```text
No CMAKE_CXX_COMPILER could be found.
```

Android cannot substitute for this measurement because this repository has no Android application target. A `flutter_tester` restore fallback was also investigated and rejected: the runtime correctly reports `Terminal sessions require a native desktop PTY path.` Therefore no fake restore number is reported.

The restore benchmark itself now explicitly enables/asserts the production parser worker, so the missing five-sample restore data can be collected unchanged on a machine with the normal Windows C++ desktop toolchain.

## T5 decision

The current evidence selects **T6c: refine structural delta / full-repaint triggers and reduce whole-buffer transfer**, after T4 is complete.

Priority within T6c should be:

1. Resize storms: coalesce or collapse intermediate structural resize snapshots so a burst does not produce one whole-buffer repaint per resize.
2. Reveal after hidden backlog: avoid generating/transferring a 92 MB-class logical full-state payload in one step where a bounded snapshot/delta strategy can preserve correctness.
3. Add stage-local instrumentation inside the worker during T6 so parse/mutation, delta construction, serialization/transit, and UI application can be measured independently before and after the protocol change.

**T6b renderer/dirty-row work is a secondary candidate**, because `flutter_tester` shows substantial raster tail jank. Re-run the native Windows render benchmark first, and re-check after T6c because reducing structural full-state traffic may also reduce renderer pressure.

**T6a replica-apply optimization is not selected first**: replica application was a minority of measured wall time even in the two worst scenarios.

**T6d restore redesign is not selected or rejected yet**: native restore timing is still an evidence gap due to this host's missing C++ desktop compiler. Collect the existing five-sample restore benchmark before deciding on direct worker snapshot hydration or another restore architecture change.

Do not start T6 production changes until T4 has completed/merged, per the coordination plan.

## Validation completed

Successful:

```text
flutter analyze test/benchmarks/terminal_production_worker_profile_benchmark.dart
flutter test test/benchmarks/terminal_production_worker_profile_benchmark.dart
flutter analyze test/benchmarks/terminal_streaming_render_profile_benchmark.dart
flutter test test/benchmarks/terminal_streaming_render_profile_benchmark.dart
flutter analyze integration_test/terminal_flush_cadence_benchmark.dart integration_test/terminal_restore_benchmark.dart
git diff --check
```

Native desktop attempts and limitations are documented above. No production terminal implementation file was changed by T5.
