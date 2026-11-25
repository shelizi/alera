## Classic Resource Monitor snapshot projection (2026-11-25)

Initial read-only process inventory found the Classic UI process holding approximately 1.30 GiB of private memory. Interval CPU/I/O sampling results were unavailable in tool output, so this is not a measured attribution of the main application bottleneck. The change targets repeat work in the resource snapshot path, without changing polling cadence or the operating-system process refresh.

- Borrow the existing chronological memory-history ring for serialization, preserving its 60-point limit and stale-entry eviction. Warm history keys need no newly allocated String and no intermediate copied Vec<u64>.
- Assemble session and host/app JSON rows once, moving the finished Values into the top-level snapshot. App absence still serializes as null. No wire schema, process attribution, CPU normalization, sampling policy or cache release change.
- Differential tests compare to the original owned-history and reserialization path through wraparound, stale eviction and absent app; pointer identity checks confirm that the JSON assembly transfers the existing session, history and host allocations. Previously emitted snapshots remain immutable.

The isolated benchmark harness uses the actual changed history, process-tree and snapshot-payload modules, with Rust 1.98.0 and opt-level=3, linking existing optimized serde 1.0.229, serde_json 1.0.151 and chrono 0.4.45 artifacts read-only. All 17 ordinary harness tests passed. Each benchmark pass constructs and drops 100 synthetic snapshots with full 60-sample histories; three repetitions of six interleaved passes per workload (nine before, nine after), for 5,400 measured snapshot projections. Medians of the nine pass timings per version:

| Sessions | Before range (ms/100) | Before median | After range (ms/100) | After median | Reduction |
|---:|---:|---:|---:|---:|---:|
| 16 | 4.963-6.963 | 5.436 | 2.266-2.595 | 2.379 | 56.2% |
| 100 | 33.266-45.420 | 39.291 | 13.314-17.930 | 16.200 | 58.8% |
| 256 | 73.653-86.097 | 78.862 | 32.144-36.407 | 32.733 | 58.5% |

Every after pass was faster than every before pass for its workload. At 100 sessions, the median improvement is 0.231 ms per snapshot, not 23 ms per snapshot. This measures the history/projection portion, excluding process-table collection, RPC transport, and UI rendering. No live CPU, I/O, heap allocation counts, or process-wide RAM reduction are claimed.

Integrated validation: rebuilt the actual CLI test target using pinned Rust 1.98.0 and unique cfg alera_resource_projection_verification_20261125; executable alera-2fdac0391fb52903.exe. All 30 ordinary resource tests, eight resource-request tests and eight demand-driven-ticker tests passed (46 passed, zero failed, one opt-in benchmark ignored). Non-test cargo check passed without warnings. No complete CLI suite rerun or live app switch. Main was not changed.

## Windows host metrics refresh (2026-11-25)

The pinned sysinfo 0.39.6 Windows implementation initializes and updates PDH counters for global/per-logical-CPU usage when refresh_cpu_usage is called. ResourceSampler does not emit these values. Its per-process CPU accounting instead uses GetProcessTimes, GetSystemTimes and the cached logical CPU count. refresh_memory also reads swap/commit information, which this payload does not use. On Windows only, request physical RAM through MemoryRefreshKind::nothing().with_ram(), and initialize the logical CPU list once with CpuRefreshKind::nothing() when empty. Per-process CPU/memory refresh, PID identity, process-tree attribution, warm-up threshold, reset/idle behavior and JSON fields remain unchanged. Non-Windows refresh is unchanged because accounting mechanisms differ. No sysinfo dependency or system configuration change.

The actual new host_metrics and existing process_tree modules were compiled in a standalone Rust 1.98.0 opt-level=3 harness, using the existing optimized sysinfo 0.39.6 system-feature library read-only. Eight ordinary tests passed; the new Windows test checks equal logical core counts and total RAM, valid available RAM, stable test-process identity, positive measured working set and positive process CPU after bounded local work and the existing warm-up period. It does not assert exact instantaneous CPU equality on a shared host.

Three independent benchmark repetitions each measured five new-System passes per version and ten warm passes per version after three priming sweeps. Order alternated before/after; warm sweeps retained their own System and waited at least the library minimum CPU update interval. 90 measured sweeps. The benchmark reads the real Windows process table and computes the test process subtree, but excludes resource history/JSON, transport and UI. Pooled median and observed range in milliseconds:

| Scenario/phase | Before range | Before median | After range | After median |
|---|---:|---:|---:|---:|
| Cold host metrics | 0.650-426.050 | 0.933 | 0.015-0.026 | 0.016 |
| Cold process refresh | 67.689-135.192 | 99.323 | 80.848-121.512 | 95.893 |
| Cold index/attribution | 0.075-0.108 | 0.084 | 0.063-0.107 | 0.084 |
| Cold measured sweep total | 68.449-516.110 | 107.027 | 80.942-121.628 | 96.014 |
| Warm host metrics | 0.763-15.256 | 1.151 | 0.003-0.019 | 0.004 |
| Warm process refresh | 103.132-389.011 | 147.554 | 87.656-350.257 | 149.001 |
| Warm index/attribution | 0.081-0.241 | 0.089 | 0.078-0.136 | 0.090 |
| Warm measured sweep total | 103.989-392.327 | 149.977 | 87.784-350.382 | 149.094 |

Host metrics median decreased 98.3% cold and 99.7% warm. Every measured minimal-host pass was faster than every original-host pass within each scenario. The first original host-metrics call in each benchmark process took 382.653, 340.114, and 426.050 ms, reflecting cold PDH initialization. Subsequent original cold passes were much faster. Cold overall median decreased 10.3%; warm overall median decreased only 0.6%, with substantial overlap and system load noise. Process-table refresh remains the dominant warm cost. These are elapsed wall-clock times, not CPU time, physical disk I/O or application-wide RAM improvements. The process-tree arithmetic is already negligible for the measured single-root fixture. No live application profiling or broad optimization claim.

Integrated validation: Rust 1.98.0 unique cfg alera_resource_host_metrics_verification_20261125, test executable D:/c/t/debug/deps/alera-97e410de947c4053.exe. All 31 ordinary resource tests, eight resource RPC and eight demand-driven ticker tests passed (47 total, two opt-in resource benchmarks ignored). Non-test cargo check passed without warnings; no complete CLI/workspace suite rerun or full release build. The Windows-specific change was validated on Windows; Unix code is unchanged, but Unix builds were not run. No running application was replaced.
