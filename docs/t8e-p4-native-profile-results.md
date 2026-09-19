# T8e P4 Native RSS / Reveal Profile Results

Date: 2026-09-19
Branch: `perf/terminal-t8e-p4-native-profile`
Baseline: `1f16ccaebfacd0822893548174582565bdad9887`

## Scope

P4 measures the existing T8e P2 soft eviction against the T8e P3 hard
parser-worker eviction. It does not change the terminal architecture.

Each reported mode used five fresh `flutter test` processes. Each process
hydrated four terminal parser workers/sessions with 1000 snapshot rows at
256000 bytes per worker/session. Runtime-level measurements waited 500 ms
after eviction and used five reveal samples.

The values below are medians across the five fresh-process results for each
mode. Reveal values are the median of each process's reported median/p95.

## Direct parser-worker profile

| Metric | Soft | Hard |
| --- | ---: | ---: |
| Hydrated RSS delta | 18,812,928 B (17.94 MiB) | 19,460,096 B (18.56 MiB) |
| Evicted RSS delta | 18,812,928 B (17.94 MiB) | 24,604,672 B (23.46 MiB) |
| RSS reclaimed after eviction | 0 B | -5,099,520 B (-4.86 MiB) |
| Reclaimed fraction of hydrated delta | 0.0% | -25.92% |
| Reveal median | 69.448 ms | 68.057 ms |
| Reveal p95 | 89.281 ms | 96.341 ms |

Hard eviction closed all four eligible workers in every hard sample. Despite
that, RSS did not fall. The hard-mode process instead retained about 4.86 MiB
more RSS at the median after eviction. One hard sample also produced a
738.806 ms reveal p95, while the other hard samples stayed below 97 ms.

## Runtime-level flutter_tester profile

| Metric | Soft | Hard |
| --- | ---: | ---: |
| Hydrated RSS delta | 62,717,952 B (59.81 MiB) | 66,420,736 B (63.34 MiB) |
| Evicted RSS delta | 62,914,560 B (60.00 MiB) | 77,066,240 B (73.50 MiB) |
| RSS reclaimed after eviction | -196,608 B (-0.19 MiB) | -9,969,664 B (-9.51 MiB) |
| Reclaimed fraction of hydrated delta | -0.28% | -13.72% |
| Reveal median | 17.741 ms | 23.280 ms |
| Reveal p95 | 30.020 ms | 33.688 ms |

All four sessions were hard-evicted in every hard sample. Three of the five
hard samples increased RSS by roughly 9.5-10.2 MiB after eviction, while two
samples reclaimed about 0.66-1.09 MiB. The cross-process median therefore
shows no stable RSS benefit from P3 hard eviction in this environment.

One hard runtime sample was also a severe tail-latency outlier:
`834.344 ms` reveal median and `939.382 ms` p95. The remaining hard
samples reported reveal medians between 20.480 ms and 23.398 ms.

## Interpretation

The current evidence does not support claiming process RSS savings from P3
hard parser-worker eviction. Both measurement surfaces show allocator/VM RSS
retention after worker isolate teardown, and the runtime-level profile shows a
larger post-eviction RSS footprint at the median.

This does not imply that the worker objects are still logically live. The hard
samples confirm the workers/sessions were eligible and closed. It means that
closing the isolates is not translating into stable process-RSS release under
these fresh-process `flutter_tester` workloads.

P3 should therefore remain a correctness-preserving mechanism, not be
described as an RSS optimization based on this P4 evidence. Before attempting
another terminal architecture cut, investigate Dart VM/isolate heap release,
native allocator retention, and whether a native Windows desktop process shows
different release behavior.

## Native Windows status

The native integration benchmark exists at
`integration_test/terminal_eviction_memory_benchmark.dart`, but the MCP
service process cannot currently reach the benchmark body because Visual C++
MSBuild FileTracker initialization sees a broken .NET Framework
`CommonApplicationData` environment. The Rust sidecar itself builds
successfully.

The next native check should be run from a normal interactive Windows
Terminal/PowerShell session under the logged-in desktop user. Do not change
the global registry; the relevant registry values were already verified as
correct during P4 investigation.

## Validation

- `cargo test -p alera-core workspace_files::quick_open -- --nocapture`:
  13/13 passed.
- Profiling-gate focused Flutter test: 1/1 passed.
- `cargo build --locked -p alera-cli --profile dev` with
  `CARGO_TARGET_DIR=C:\\c\\cli`: passed; pre-existing warnings only.
- Direct-worker profile: 5 soft + 5 hard fresh processes, all passed.
- Runtime-level profile: 5 soft + 5 hard fresh processes, all passed.

## Decision

P4 concludes that P3 hard eviction has no demonstrated RSS advantage in the
available fresh-process `flutter_tester` evidence and introduces additional
reveal-tail risk. No new terminal architecture phase should be started from
this result alone. Native Windows desktop measurements remain the final
environment-specific follow-up.
