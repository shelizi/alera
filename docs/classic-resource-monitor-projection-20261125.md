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
