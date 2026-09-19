# T8e P4 Native RSS / Reveal Profile Results

Date: 2026-09-19
Branch: `perf/terminal-t8e-p4-native-profile`
Baseline: `1f16ccaebfacd0822893548174582565bdad9887`

## Scope

P4 measures the existing T8e P2 soft eviction against the T8e P3 hard
parser-worker eviction. It does not change the terminal architecture.

The direct parser-worker and runtime-level profiles used five fresh
`flutter test` processes per mode. Each process hydrated four terminal
parser workers/sessions with 1000 snapshot rows at 256000 bytes per
worker/session. Runtime-level measurements waited 500 ms after eviction and
used five reveal samples.

The direct-worker and runtime-level values below are medians across the five
fresh-process results for each mode. Reveal values are the median of each
process's reported median/p95.

After resolving the Windows build-host environment blocker, one native Windows
desktop-process sample per mode was also run with the same four-session,
1000-row workload and five reveal samples. Treat those native values as a
directional cross-check, not a five-process median.

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

## Native Windows desktop-process profile

The native integration benchmark was successfully built and executed with:

`integration_test/terminal_eviction_memory_benchmark.dart`

One fresh native Windows run was captured per mode:

| Metric | Soft | Hard |
| --- | ---: | ---: |
| Baseline RSS | 297,193,472 B | 292,089,856 B |
| Hydrated RSS delta | 53,981,184 B (51.48 MiB) | 60,174,336 B (57.39 MiB) |
| Evicted RSS delta | 56,213,504 B (53.61 MiB) | 73,072,640 B (69.69 MiB) |
| RSS reclaimed after eviction | -2,232,320 B (-2.13 MiB) | -12,898,304 B (-12.30 MiB) |
| Reclaimed fraction of hydrated delta | -4.14% | -21.43% |
| Hard-evicted sessions | 0 | 4 |
| Reveal median | 217.966 ms | 235.237 ms |
| Reveal p95 | 1011.591 ms | 866.137 ms |

The native hard sample successfully closed all four eligible parser workers,
but RSS increased by another 12.30 MiB after eviction. The soft sample also
increased after eviction, by 2.13 MiB. This single-pair native result is
directionally consistent with both five-process `flutter_tester` surfaces:
hard eviction does not produce an observable process-RSS release and shows a
larger post-eviction RSS footprint than soft eviction.

## Interpretation

The current evidence does not support claiming process RSS savings from P3
hard parser-worker eviction. Both measurement surfaces show allocator/VM RSS
retention after worker isolate teardown, the runtime-level profile shows a
larger post-eviction RSS footprint at the median, and the native Windows
desktop-process cross-check shows the same direction.

This does not imply that the worker objects are still logically live. The hard
samples confirm the workers/sessions were eligible and closed. It means that
closing the isolates is not translating into stable process-RSS release under
these fresh-process `flutter_tester` workloads.

P3 should therefore remain a correctness-preserving mechanism, not be
described as an RSS optimization based on this P4 evidence. Before attempting
another terminal architecture cut, investigate Dart VM/isolate heap release
and native allocator retention rather than assuming isolate teardown will
reduce process RSS.

## Native Windows build-host root cause

The prior native-build blocker was not an Alera compile failure, a Visual
Studio installation problem, or an F-drive VS problem. The MCP child-process
environment was missing the standard Windows `SystemDrive` variable.

Under that environment:

- `ProgramData=C:\ProgramData` was present.
- The 32-bit and 64-bit registry values for `Common AppData` were correct.
- `.NET Framework Environment.GetFolderPath(CommonApplicationData)` returned
  an empty string.
- `SHGetFolderPath` / `SHGetKnownFolderPath` failed verification.
- With `DONT_VERIFY`, both APIs returned the unexpanded
  `%SystemDrive%\ProgramData` path.
- `SystemDrive` itself was empty.

That caused `Microsoft.Build.Utilities.FileTracker` to reach
`Path.GetPathRoot` with an invalid/empty common-application-data path.

Setting only `SystemDrive=C:` for the child build process fixed the same
VS18 `ZERO_CHECK.vcxproj` that previously failed, and then allowed the full
`flutter test -d windows` native benchmark to build and run successfully.
No global registry or Visual Studio changes are required.

## Validation

- `cargo test -p alera-core workspace_files::quick_open -- --nocapture`:
  13/13 passed.
- Profiling-gate focused Flutter test: 1/1 passed.
- `cargo build --locked -p alera-cli --profile dev` with
  `CARGO_TARGET_DIR=C:\\c\\cli`: passed; pre-existing warnings only.
- Direct-worker profile: 5 soft + 5 hard fresh processes, all passed.
- Runtime-level profile: 5 soft + 5 hard fresh processes, all passed.
- Native Windows soft profile with `SystemDrive=C:`: built
  `build\windows\x64\runner\Debug\alera-dev.exe` and passed 1/1.
- Native Windows hard profile with `SystemDrive=C:`: built the same native
  target and passed 1/1; all four eligible sessions were hard-evicted.
- Direct VS18 `ZERO_CHECK.vcxproj` with `SystemDrive=C:`: passed, proving
  the prior FileTracker failure was caused by the stripped environment.

## Decision

P4 concludes that P3 hard eviction has no demonstrated RSS advantage in either
the repeated fresh-process `flutter_tester` evidence or the native Windows
desktop-process cross-check. The native run also confirms that closing all four
eligible parser workers does not translate into process RSS release.

No new terminal architecture phase should be justified by expected RSS
savings from P3 hard eviction alone. Future work should profile allocator/VM
retention and only proceed with another memory architecture cut when it has a
separate, measurable target.
