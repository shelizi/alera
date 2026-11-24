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

## Direct Windows candidate snapshot

Replace the ordinary preview path's broad `std::fs::File::metadata()` call with three documented Win32 information classes: FileStandardInfo (EOF, directory/delete-pending), FileBasicInfo (attributes and timestamps) and FileIdInfo (volume serial and full 128-bit identity). These queries share one already-open no-follow handle. The scanner retains only length and millisecond update time; the cache stamp retains all original identity and change-time fields. Use EOF, not allocation size, and preserve the pre-Unix-epoch clamp and millisecond truncation. Unsupported information classes or invalid values fall back to the original compatibility path. The private SQLite index and post-parse stamp check still use the original FileStamp::read path.

Interleaved Debug benchmark runs on the same shared host, using the prior no-follow implementation at `61eea04ff` as baseline (before/after/after/before/before/after). Both binaries passed all 42 measured requests per run, 252 requests overall. Each cell lists the three run p50 values for that version in chronological order; the summary is the median of these three medians, not a pooled percentile.

| Scenario | Before run p50 (ms) | After run p50 (ms) | Before median (ms) | After median (ms) |
|---|---|---|---:|---:|
| Cold total | 62.085, 88.627, 83.068 | 110.870, 72.270, 65.203 | 83.068 | 72.270 |
| Warm total | 65.653, 71.779, 66.266 | 64.157, 63.126, 64.035 | 66.266 | 64.035 |
| Index reopen total | 82.472, 94.429, 72.434 | 64.836, 65.600, 71.102 | 82.472 | 65.600 |
| One file changed total | 91.629, 75.388, 76.283 | 78.393, 81.761, 74.610 | 76.283 | 78.393 |
| Warm scan | 40.775, 45.220, 42.841 | 41.170, 39.642, 40.309 | 42.841 | 40.309 |

Process other-I/O median counts are identical across all three runs per version:

| Scenario | Before | After | Reduction |
|---|---:|---:|---:|
| Cold | 4120 | 3608 | 512 |
| Warm | 3316 | 2804 | 512 |
| Index reopen | 3336 | 2824 | 512 |
| One file changed | 3319 | 2807 | 512 |

Warm search has 15.44% fewer counted other-I/O operations than the previous implementation and 35.39% fewer than the initial 4340 baseline. All process read/write operation counts and SQL write counts remain unchanged. This is two fewer counted other-I/O operations per ordinary file, not a direct syscall or physical disk transfer count. Wall-clock changes are not uniformly positive: single-file-change median increased by about 2.8%, and one after run had a cold scan tail near 2 seconds. The repeatable I/O reduction justifies retaining this scoped fast path; do not claim guaranteed elapsed-time improvements. No release-profile, full suite or RSS measurement was performed.

Validation: 32 history regressions passed (one benchmark ignored in the normal run), 10 exact resume tests and 2 Agent Context tests passed. The actual Windows symlink fixture was created and rejected. The new direct-snapshot test verified exact EOF for an 8193-byte file, pre-epoch and submillisecond timestamps against legacy std metadata, and identical cache stamp fields. Existing freshness and scope regressions remain green.

## Read-only warm batch equality check

For requests whose native preview reuse count covers the current result batch, attempt a single read-only SQLite statement using a bounded transient canonical JSON batch. The statement joins bound document keys and exact canonical JSON against the persisted rows, checks text types and verifies the retention count in the same statement snapshot. An exact match skips the explicit update transaction, previous-row materialization, and separate retention SELECT. It does not use a hash or retain any additional cross-request cache. Any missing or changed record, noncanonical JSON, conflicting duplicate, oversized index, or batch exceeding the 2 MiB serialized JSON threshold falls back to the existing transaction path. All records use the same scope and field-budget validation. Cold and partially changed native scans continue to use the original path.

Interleaved Windows Debug A/B runs against `cbf721a0d` (before/after/after/before/before/after), 42 measured requests per run, 252 total. Values are individual run p50 values followed by their median, not pooled percentiles.

| Scenario | Before run p50 (ms) | After run p50 (ms) | Before median (ms) | After median (ms) |
|---|---|---|---:|---:|
| Cold total | 131.254, 67.682, 76.499 | 61.639, 74.888, 67.577 | 76.499 | 67.577 |
| Warm total | 70.219, 66.624, 67.119 | 62.625, 25.982, 62.416 | 67.119 | 62.416 |
| Index reopen total | 71.667, 72.352, 71.173 | 65.415, 29.451, 70.033 | 71.667 | 65.415 |
| One file changed total | 75.903, 75.495, 76.340 | 71.697, 71.234, 82.596 | 75.903 | 71.697 |
| Warm upsert check | 11.910, 11.392, 11.397 | 8.213, 4.399, 8.413 | 11.397 | 8.213 |
| Index reopen upsert check | 11.897, 12.687, 12.603 | 9.663, 5.231, 9.680 | 12.603 | 9.663 |

Warm upsert check median declined 27.9%, warm total 7.0%, reopen upsert check 23.3%, reopen total 8.7%. Warm/reopen fast-path counters show one equality SELECT and no old document rows returned to Rust for every measured request (84 total warm/reopen requests across the three after runs). The previous path materializes up to 256 existing rows and executes two SELECTs in an explicit transaction. Warm/reopen SQL writes remain zero. Cold and one-file-change paths are unchanged; do not attribute their noisy elapsed-time differences to this optimization. Windows process read/write and other-I/O operation counts were unchanged; this improvement targets SQLite/Rust row materialization and query overhead, not physical disk transfers. Complete JSON serialization is still required. The fast path aggregates serialized JSON into one temporary buffer, with a threshold checked after each record; its peak capacity and RSS have not been measured.

Validation: 33 history regressions passed (one benchmark ignored in the ordinary run), 10 exact resume tests, 2 Agent Context tests. New tests exercise exact canonical comparison, rejection of an over-retention snapshot, same-file persisted JSON repair, duplicate update/revert order, clear/recreation and invalid input rejection. The existing service test confirms the warm fast path with an unregistered file excluded. Six benchmark runs passed. Shared-host Debug timings remain noisy, especially the unusually fast second after run. No complete Rust suite, release benchmark or RSS measurement was performed.

## Reusable ranking normalization and term coverage

The shared core search ranking now normalizes the bounded query once, borrows query words from that normalized string, and reuses one normalization buffer and one matched-term set across candidate records. Bounded ASCII prefixes have a byte-oriented normalization path; non-ASCII input retains the original Unicode character classification and per-character lowercase expansion. Separator collapsing, query and field truncation, repeated-term scoring, distinct-term coverage, field weights, stable ranking, and session identity/deduplication are preserved. No FTS5 query, database schema, candidate limit, resume authorization, or session identity rule changed.

Interleaved Windows Debug A/B runs against `6ea8d6aa3` (before/after/after/before/before/after), 42 measured requests per run, 252 total. Values are individual run p50 timings followed by their median, not pooled percentiles.

| Scenario | Before run p50 (ms) | After run p50 (ms) | Before median (ms) | After median (ms) |
|---|---|---|---:|---:|
| Cold total | 71.622, 88.310, 71.310 | 69.828, 65.281, 116.171 | 71.622 | 69.828 |
| Warm total | 69.024, 56.951, 63.160 | 27.363, 60.806, 61.171 | 63.160 | 60.806 |
| Index reopen total | 70.460, 69.982, 93.587 | 31.006, 58.115, 66.913 | 70.460 | 58.115 |
| One file changed total | 75.027, 89.168, 83.904 | 31.341, 88.522, 68.559 | 83.904 | 68.559 |
| Cold ranking | 2.905, 3.440, 2.600 | 1.530, 1.757, 2.378 | 2.905 | 1.757 |
| Warm ranking | 5.041, 4.578, 4.825 | 1.342, 2.712, 2.553 | 4.825 | 2.553 |
| Index reopen ranking | 4.836, 5.281, 4.526 | 1.399, 2.561, 2.693 | 4.836 | 2.561 |
| One file changed ranking | 4.778, 5.089, 5.260 | 1.335, 2.473, 2.558 | 5.089 | 2.473 |

The warm ranking phase decreased by 47.1%; cold ranking by 39.5%, reopen ranking by 47.0%, and one-file-changed ranking by 51.4%. The warm end-to-end median decreased by 3.7%. The ranking phase includes current-observed identity filtering as well as the core scorer. Every after-run ranking p50 was lower than every before-run p50 within each scenario. The first after run had unusually fast scan and database phases, and cold scans still exhibited long tails. Only ranking code changed, so total, scan and database timing differences are not evidence of improvements to the unchanged stages. No allocator counts or RSS measurements were taken; scratch-buffer reuse is established by source inspection and pointer-stability regression checks, not a measured memory-footprint reduction.

Validation: 13 core search tests, 33 history tests, 10 exact resume tests, and 2 Agent Context tests passed (58 total). The two new core tests compare normalization and scoring with independent copies of the prior algorithms, including ASCII, Unicode and mixed-script cases, bounded character prefixes, duplicate terms, ignored tails and milestones, and cross-record scratch reuse. All six benchmark runs passed; the CLI rebuilt the changed core rather than reusing its old build artifact. The normal history test run leaves the opt-in benchmark ignored. Shared build lock delayed CLI compilation but released without interrupting other work. No complete Rust suite, release benchmark, or process RAM measurement was performed.

## Selective incremental upsert after main integration (2026-11-25)

Integrated the twelve history-search optimization commits into main at `2b95facbf`. The history-search, benchmark and ranking source files at the merge are identical to the previously validated `eeb38ed62` source. The previous main commits and unrelated dirty files were preserved. The new incremental branch starts at the merge commit.

When at least half the observed records can plausibly reuse native previews, and the request is not already eligible for the warm read-only check, the update path binds canonical JSON and identity keys in one bounded CTE inside the existing transaction. SQLite returns only mismatches or missing rows. The same exact-byte comparison and text-type checks are shared with the warm query. Missing rows carry a NULL ID; omitted keys are unchanged. Reuse the serialized JSON for changed rows rather than serializing it again. The ordinary transaction still updates documents/FTS and performs retention. Duplicate document keys or aggregate JSON above the 2 MiB threshold take the original ordered path; cold and mostly changed scans also retain the original path. No new persistent cache, schema or security policy was added.

Interleaved Windows Debug A/B runs (before/after/after/before/before/after), comparing the preceding `eeb38ed62` executable with the new branch based on integrated main. Both versions use the same relevant baseline history and ranking sources and synthetic fixture; unrelated main changes are present only in the new executable. All six runs passed, 42 measured requests per run, 252 requests total. The table lists each run's p50 and the median of the three per-version p50 values, not pooled percentiles.

| Scenario | Before run p50 (ms) | After run p50 (ms) | Before median (ms) | After median (ms) |
|---|---|---|---:|---:|
| Cold total | 63.583, 117.347, 120.219 | 97.071, 81.995, 76.969 | 117.347 | 81.995 |
| Warm total | 27.736, 61.299, 61.638 | 69.735, 62.285, 29.390 | 61.299 | 62.285 |
| Index reopen total | 31.396, 67.819, 64.000 | 66.303, 64.549, 65.849 | 64.000 | 65.849 |
| One file changed total | 78.071, 72.874, 77.590 | 67.983, 73.539, 63.376 | 77.590 | 67.983 |
| One file changed upsert | 12.193, 11.475, 12.712 | 9.101, 9.737, 9.787 | 12.193 | 9.737 |

For every single-file-change request in the new runs (21), the selective lookup ran once, returned one mismatch and materialized one previous document row, compared with 256 previous document rows per request in the old runs. This is a 99.61% reduction in previous-row materialization, not a measurement of total allocations, memory usage or bytes read from the physical disk. SQL write statements remained three; Windows process other-I/O counts remained 2807 at p50. The changed-case update-check median decreased 20.1%, and the end-to-end median decreased 12.4%. All three after-run changed-case upsert p50s are lower than all three before-run p50s. Unchanged paths showed noise (including faster and slower runs), so do not claim improvements in cold, warm, or reopen paths. The shared-host Debug results are not release performance guarantees.

Validation on the integrated-main incremental branch: 35 history tests passed (one opt-in benchmark ignored in the ordinary run), 10 exact resume tests and 2 Agent Context tests passed, 47 total. New tests compare all stored document and FTS fields, row IDs and logical stats against the legacy path on a 256-record batch with text change, metadata change, noncanonical JSON repair and a new account record. They also verify that unobserved old records remain, unchanged rows are not returned, duplicate reverts preserve sequential statistics, and an oversized comparison falls back. Actual Windows symlink rejection passed. The same opt-in benchmark passed all six runs. Format and Git diff checks passed. No separate unmodified-main test executable, full Rust suite, release benchmark or RSS measurement was run.
