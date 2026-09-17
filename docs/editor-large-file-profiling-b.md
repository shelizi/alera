# Work package B - current large-file editor profiling

Date: 2026-09-17

Baseline: `c5ea3d56` (latest main when B worktree was created)

Merge validation was rebased onto main at `8247c0e3`, which includes the completed Work package E regex snapshot-cache optimization.

Conclusion: **B1 - current Dart/CodeForge hot path is now bounded enough; proceed to the NativeEditorDocument parser spike.**

## Scope

This work package intentionally did not start a broad parser rewrite. It profiled the current editor after the recent large-file viewport, long-line, search-highlight, selection, document-highlight, and diagnostics work.

The reusable benchmark is `integration_test/editor_large_file_benchmark.dart`. It exercises all scenarios required by `docs/rust-heavy-operation-parallel-work-plan.md` on a real Windows desktop runner:

- fast vertical scrolling;
- fast horizontal scrolling on a 30,000-character line;
- caret movement;
- word movement;
- selection dragging;
- typing bursts;
- literal search with many matches;
- regex search with many matches;
- diagnostics-heavy large files;
- large-file open/reopen.

The fixture has 50,000 lines, one literal/regex match per line, and the diagnostics scenario installs 10,000 diagnostics. Every scenario is warmed first and then measured for five samples. The report uses median, p95, and MAD rather than a single run.

## How to run

Low-observer-effect frame/jank run, which is the run to compare before and after editor rendering changes:

```text
flutter test integration_test/editor_large_file_benchmark.dart -d windows
```

Allocation/GC/UI CPU evidence run:

```text
BENCH_VM_METRICS=1 flutter test integration_test/editor_large_file_benchmark.dart -d windows
```

On Windows `cmd.exe`, the equivalent is:

```text
set BENCH_VM_METRICS=1&& flutter test integration_test/editor_large_file_benchmark.dart -d windows
```

VM metrics mode connects to the Dart VM service, resets allocation accumulators for each sample, collects CPU samples, heap/allocation counters and GC events, and therefore has measurable observer effect. **Do not use the VM-metrics run's frame timings as the frame-regression baseline.** Use it only for allocation/CPU/GC evidence.

Each scenario also emits a Timeline range named `EditorLargeBenchmark.<scenario>`. In non-release builds the existing CodeForge long-paragraph path emits `CodeForge.largeFileParagraphLayout`, allowing DevTools traces to correlate a scenario with paragraph/shaping work and GC.

## Low-observer-effect results

These numbers are from the five-sample Windows desktop run with VM-service metrics disabled.

| Scenario | wall median / p95 ms | frame median / p95 ms | build p95 ms | raster p95 ms | 60 Hz jank | RSS delta median / p95 MiB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| open/reopen | 347.24 / 364.43 | 6.40 / 40.66 | 37.22 | 1.86 | 5 / 19 frames | 4.62 / 9.31 |
| fast vertical scroll | 463.10 / 534.92 | 7.46 / 15.29 | 12.53 | 2.69 | 4 / 77 frames | 5.36 / 17.27 |
| fast horizontal scroll, long line | 461.20 / 465.48 | 5.85 / 8.55 | 5.22 | 2.01 | 0 / 77 frames | 2.14 / 3.66 |
| caret movement | 325.64 / 326.71 | 5.16 / 12.84 | 2.71 | 2.21 | 0 / 55 frames | 0.91 / 1.45 |
| word movement | 327.52 / 341.09 | 5.60 / 15.93 | 2.32 | 2.34 | 3 / 57 frames | 2.16 / 3.87 |
| selection dragging | 394.61 / 398.70 | 6.01 / 8.81 | 4.22 | 2.59 | 1 / 73 frames | 2.92 / 8.66 |
| typing burst | 265.22 / 266.37 | 5.81 / 8.48 | 4.52 | 2.75 | 0 / 50 frames | 3.35 / 3.50 |
| literal search, 50k matches | 405.56 / 424.39 | 3.33 / 10.80 | 2.94 | 2.21 | 3 / 65 frames | 0.28 / 1.75 |
| regex search, 50k matches | 234.45 / 243.49 | 4.48 / 7.85 | 3.77 | 2.85 | 0 / 38 frames | -1.52 / 4.83 |
| diagnostics-heavy scroll | 368.71 / 374.01 | 5.90 / 12.46 | 10.55 | 3.37 | 1 / 63 frames | 4.14 / 10.41 |

### Frame interpretation

The current sustained interaction paths do not expose a dominant 60 Hz bottleneck. Fast vertical scrolling is the closest case, but its frame p95 is 15.29 ms and raster p95 is only 2.69 ms. Long-line horizontal scrolling, typing and selection dragging are comfortably below the 16.7 ms frame budget at p95.

Open/reopen has a different shape: approximately one expensive initialization/first-layout frame per sample. Its raster p95 is still only 1.86 ms, while build p95 reaches 37.22 ms. This is an initialization/first-frame cost rather than a continuously reachable rendering hot path, so it is not evidence for a B2 rewrite before the retained-parser spike.

The 120 Hz budget is not universally met, especially for vertical scrolling and cursor/word navigation. That is worth retaining as a future optimization target, but no current path shows the kind of dominant whole-document work that should block C.

## VM-service CPU / allocation / GC evidence

The following medians come from the five-sample `BENCH_VM_METRICS=1` run. This mode intentionally has observer effect; the values are useful for relative allocation/CPU evidence, not as production-memory totals or frame-regression numbers.

| Scenario | UI CPU median ms | allocated median MiB | allocated instances median | natural GC events median |
| --- | ---: | ---: | ---: | ---: |
| open/reopen | 184 | 224.44 | 2,401,328 | 25 |
| fast vertical scroll | 106 | 228.60 | 2,494,302 | 29 |
| fast horizontal scroll, long line | 93 | 229.84 | 2,514,084 | 28 |
| caret movement | 136 | 230.75 | 2,529,214 | 31 |
| word movement | 152 | 230.45 | 2,442,466 | 32 |
| selection dragging | 64 | 241.06 | 2,608,179 | 29 |
| typing burst | 56 | 243.14 | 2,657,205 | 28 |
| literal search, 50k matches | 63 | 246.88 | 2,712,452 | 35 |
| regex search, 50k matches | 41 | 250.67 | 2,812,631 | 37 |
| diagnostics-heavy scroll | 70 | 248.20 | 2,772,186 | 28 |

The debug/JIT allocation profile is deliberately broad: it includes all main-isolate allocation while VM allocation counters and profiling are enabled. This explains the large absolute allocation totals and the frequent GC counts. The useful signal is that no required scenario is an order-of-magnitude allocation outlier. Regex/literal search and diagnostics are somewhat higher than the simple interaction paths, but the uninstrumented frame run shows they do not create a sustained UI-frame bottleneck.

## Paragraph / shaping evidence

CodeForge already avoids shaping the whole very-long line in the relevant large-file mode. The renderer obtains native line layout information and, for fixed-width safe ASCII lines, materializes only the horizontally visible Rope slice plus overscan. The 30,000-character horizontal-scroll scenario validates this path: frame p95 is 8.55 ms, build p95 5.22 ms, raster p95 2.01 ms, and there are zero 60 Hz jank frames in 77 measured frames.

For fallback long-paragraph construction, `_buildParagraph` already records `CodeForge.largeFileParagraphLayout` in non-release builds. The B harness adds outer scenario Timeline ranges rather than adding always-on production instrumentation.

## FFI payload review

The current important FFI shapes are bounded or assigned to a separate work package:

- **Document open/reopen:** current Rope construction necessarily transfers the whole initial document once. The measured open/reopen median is about 347 ms for the 50k-line fixture. This is initialization cost, not repeated scroll/edit payload.
- **Long-line scrolling:** the large-file ASCII path asks native code for line layout metadata and transfers only the visible `rope.substring(...)` slice for rendering. It does not transfer the 30k-character line every frame.
- **Literal search:** Dart sends the query to native `rope.findLiteral` and receives match start/end ranges. The 50k-match fixture therefore returns O(matches) range data, not a full text snapshot. Frame p95 is 10.80 ms and wall median 405.56 ms; the wall result includes the find controller's debounce.
- **Regex search:** the latest main (after Work package E) keeps Dart `RegExp` compatibility but caches one `rope.getTextSnapshot()` per `documentVersion`, reusing that full-document FFI snapshot across repeated Find queries until the document revision changes. The first snapshot remains O(document size), but repeated queries no longer repeat the dominant copy. In B's 50k-line fixture regex search did not dominate UI frames, so it does not turn B into B2.

## Decision

**B1: current Dart/CodeForge hot path is now bounded enough; proceed to NativeEditorDocument parser spike.**

No bounded production-code fix is justified by B's measurements. Starting another broad Dart/renderer rewrite here would be lower leverage than C's retained incremental parser work and would overlap the purpose of the next architecture phase.

C can proceed with its intended narrow Phase 1: retained native document state, incremental edits, viewport syntax spans with fixed overscan, revision-based stale-result rejection, and lifecycle cleanup. B does not justify adding symbol outline, folding, bracket matching, structural selection, search indexing, or diagnostics indexing to that first parser phase.

Work package E completed while B was in progress. Its version-aware snapshot reuse is complementary to B: B establishes that regex search is not a current frame-hot-path blocker, while E reduced repeated full-snapshot cost without changing Dart regex compatibility semantics.

## Validation performed

```text
flutter pub get
flutter analyze integration_test/editor_large_file_benchmark.dart
flutter test integration_test/editor_large_file_benchmark.dart -d windows
BENCH_VM_METRICS=1 flutter test integration_test/editor_large_file_benchmark.dart -d windows
```

Both Windows benchmark modes completed the full 10-scenario matrix with five samples per scenario and passed. Existing build warnings from unrelated Rust sidecar dead-code/unused items were present but did not fail the build.
