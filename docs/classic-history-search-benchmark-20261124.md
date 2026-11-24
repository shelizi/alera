# Classic Agent history search benchmark (2026-11-24)

## Workload and reproduction

The opt-in test `agent_session_history_service::benchmarks::history_search_phase_benchmark` measures the real on-demand search pipeline. It creates only synthetic temporary data: 256 Codex/Claude transcripts (9,574,546 bytes on the measured run), two provider accounts, eight registered Workspaces and 128 unrelated directory entries. It excludes fixture creation and the forced index-close operation from measured request latency. Cold means a fresh application preview cache and newly created SQLite index, not a flushed OS page cache. The warm and index-reopen scenarios retain the current preview cache; the changed-file scenario modifies one transcript before each measured request.

All timing and extra counters are test-only. The benchmark is ignored in ordinary test runs and has correctness assertions, not timing thresholds. Run it alone in the checkout. On Windows, prepare the local build storage first in the same shell:

```powershell
& .\tool\development\prepare_windows_build_storage.ps1
cargo test --manifest-path rust/Cargo.toml --locked --offline -p alera-cli --bin alera agent_session_history_service::benchmarks::history_search_phase_benchmark -- --ignored --exact --nocapture --test-threads=1
```

When multiple worktrees share a Cargo target, verify the selected executable includes the benchmark; if the artifact is stale, use `cargo rustc --manifest-path rust/Cargo.toml --locked --offline -p alera-cli --bin alera --profile test -- --cfg alera_history_benchmark_verification_20261124` and run the emitted test executable directly. A distinct per-checkout verification cfg can prevent accidental reuse. For release-profile data, run the equivalent Cargo test with `--release`.

## Initial Windows Debug baseline

Measured on Intel Core i5-14500 (14 cores, 20 logical processors), shared developer host, Debug/unoptimized Rust test executable, after the seven commits ending at `47ce198bf`. This is a diagnostic baseline, not a before/after comparison against the pre-optimization version. P95 is the nearest-rank percentile; with seven samples it is the maximum. Other host processes were not suspended. No physical disk or total allocator/RSS measurement is implied by the process I/O and buffer-start counters.

| Scenario | Samples | Total p50 (ms) | Total p95 (ms) | Scan p50 (ms) | Index open/validate p50 (ms) | Keys p50 (ms) | Upsert p50 (ms) | FTS p50 (ms) | Rank p50 (ms) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Cold | 7 | 140.694 | 1990.951 | 91.387 | 13.621 | 1.602 | 21.406 | 5.403 | 4.872 |
| Warm | 21 | 88.646 | 148.793 | 55.368 | 0.271 | 1.805 | 14.590 | 6.000 | 5.961 |
| Index reopen | 7 | 97.232 | 134.509 | 49.787 | 4.063 | 1.818 | 14.627 | 5.737 | 4.825 |
| One file changed | 7 | 91.769 | 102.636 | 64.882 | 0.314 | 1.787 | 12.584 | 5.310 | 4.859 |

Each phase is independently summarized; the medians do not necessarily sum to the total median. Cold scan p95 was 1945.438 ms, so the large cold tail was in the scanning phase rather than SQLite; the underlying OS or scheduling cause was not established.

| Scenario | Parsed | Reused | SQL write statements | JSON buffer starts | Process read ops | Process write ops | Process other ops | Process read bytes | Process write bytes | Process CPU p50 (ms) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Cold | 256 | 0 | 4 | 256 | 261 | 206 | 5144 | 2101264 | 420772 | 125.000 |
| Warm | 0 | 256 | 0 | 1 | 0 | 0 | 4340 | 0 | 0 | 93.750 |
| Index reopen | 0 | 256 | 0 | 1 | 74 | 0 | 4360 | 299108 | 0 | 93.750 |
| One file changed | 1 | 255 | 3 | 2 | 1 | 16 | 4343 | 8192 | 32960 | 78.125 |

All counters in the table are per-request medians. Windows process I/O counters cover the entire test process (including cached I/O and other operations); they do not measure physical drive transfers or identify every metadata call. CPU time covers all threads and can exceed elapsed time. JSON buffer starts measure the tested serialization storage path only, not total allocation count or RSS.

## Follow-up priorities

- Prioritize native scan and metadata validation: scan p50 was about 62% of warm total p50; warm search had zero counted read/write operations, but 4,340 other I/O operations. Inspect the Windows handle metadata queries and measure any change against this fixture.
- Preserve file identity, canonical root containment, symlink handling, account scope, change-time validation and immediate next-search freshness. Do not replace the checks with a time-only directory cache.
- SQLite upsert checking is the next largest measured warm phase. Reassess after scan changes; avoid unnecessary further micro-optimizations without measured gain.
- Repeat on an idle host and in release configuration to characterize tails and compare against the original version before claiming production speedups or RSS reductions.

Validation: isolated benchmark passed (42 measured requests); history search 29 passed (benchmark ignored in normal suite), exact resume 10 passed, Agent Context 2 passed. No production instrumentation, schema changes or application switch.

## Windows no-follow handle metadata fast path

The ordinary Windows preview candidate now opens its final component with `FILE_FLAG_OPEN_REPARSE_POINT`, rejects directory/reparse attributes on the opened handle, normalizes the canonical path on that handle, and derives size, modification time and the native cache stamp from the same metadata snapshot. The 128-bit File ID, volume, creation/last-write/change times, and post-parse stamp comparison remain. Special files, unsupported stamps and open errors use the original path-based compatibility checks. Exact native verification still uses the uncached path. No persistent directory cache was introduced.

Re-ran the original benchmark executable and the new executable on the same shared i5-14500 Windows Debug host, first before/after and then after/before/before/after. Both executables verified their synthetic fixture and all 42 requests per run. The following are per-run p50 milliseconds, with each cell listing three runs in chronological order for that version, then the median of the three run medians (not a pooled percentile).

| Scenario | Before run p50 (ms) | After run p50 (ms) | Before median (ms) | After median (ms) |
|---|---|---|---:|---:|
| Cold total | 78.403, 88.618, 68.055 | 71.703, 78.857, 74.063 | 78.403 | 74.063 |
| Warm total | 32.853, 78.634, 76.470 | 64.832, 67.858, 65.794 | 76.470 | 65.794 |
| Index reopen total | 86.694, 87.858, 85.530 | 74.856, 85.603, 62.015 | 86.694 | 74.856 |
| One file changed total | 82.839, 110.942, 92.987 | 81.766, 87.101, 73.303 | 92.987 | 81.766 |
| Warm scan | 20.047, 53.652, 52.261 | 40.386, 44.315, 43.073 | 52.261 | 43.073 |

The per-request median process other-I/O counts are identical across all three runs of each version:

| Scenario | Before | After | Reduction |
|---|---:|---:|---:|
| Cold | 5144 | 4120 | 1024 |
| Warm | 4340 | 3316 | 1024 |
| Index reopen | 4360 | 3336 | 1024 |
| One file changed | 4343 | 3319 | 1024 |

This is four fewer counted other-I/O operations per candidate, or 23.59% fewer for warm search. These are Windows process counters, not a direct count of individual system calls or physical disk transfers. Warm search still has zero counted read/write operations and zero SQL writes. The repeated elapsed timings are noisy (the initial before run was unusually fast in warm search); do not interpret their differences as guaranteed release performance. No independent Release or RSS measurement was performed.

Validation: 31 history regressions (one benchmark ignored in the normal test run), 10 exact resume tests, 2 Agent Context tests passed; the Windows symlink fixture was actually created and rejected. Both versions passed three benchmark runs each (252 measured requests overall). New tests confirm regular-handle metadata/mtime/stamp equivalence and oversized file accounting; existing tests exercise changed, deleted, replaced and same-length rewritten files with restored mtime, plus long Unicode paths. The 64-file scan regression confirms 64 handle metadata reuses and zero path metadata probes for normal files, with exact verification still taking the compatibility path.
