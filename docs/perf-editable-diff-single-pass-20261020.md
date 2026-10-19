# Alera Editable Git Diff single-pass optimization

Date: 2026-10-20
Branch: local main

## Scope

Editable side-by-side Diff previously split and normalized the complete files
twice and recomputed the same Myers line steps twice for every edit: once for
line markers and spacer rows, then independently for UTF-16 character ranges.
Parent rebuilds could also repeat the entire calculation.

This batch is restricted to editable working-tree Git Diff. Read-only Git Diff,
syntax highlighting, file saving, terminal and other user modifications are not
changed.

## Changes

- One preparation pass normalizes the two line lists and computes the Myers
  steps once, then shares them with the alignment and intraline projections.
- Cache the most recent combined projection by baseline, current text and
  whitespace mode, so Flutter parent updates echoing the same editor revision
  do not rerun Myers or reapply decorations.
- When both changed interiors have at least 96 rows and their normalized line
  sets have no intersection, no unchanged Myers match is possible. The existing
  prefix/deletions/additions/suffix fallback is sufficient; skip quadratic
  Myers tracing. If even one shared normalized line is present, keep the
  original Myers behavior.
- Preserve Unicode UTF-16 offsets, CRLF, indentation/whitespace modes,
  unchanged context, scroll controllers, editing and undo history.

## CPU microbenchmark

Run from repo root:

    flutter test --no-pub benchmark/editable_diff_projection_benchmark.dart

This benchmark includes Dart line alignment, spacer calculation and character
highlight projection. Seven timed runs after warmup, median microseconds.
It does NOT include Flutter widget layout/painting, FFI, disk I/O, frame
presentation or Windows Release AOT.

| Case | Fused baseline (us) | Fast path (us) | Speedup |
| --- | ---: | ---: | ---: |
| 10,000 rows, one changed | 3,720 | 3,869 | Approximately unchanged |
| 300 rows fully replaced | 11,815 | 1,115 | 10.6x |
| 600 rows fully replaced | 51,298 | 2,009 | 25.5x |
| 600 rows replaced between 200-row unchanged anchors | 51,862 | 1,684 | 30.8x |

The baseline above is measured AFTER fusing both Myers calculations but
BEFORE introducing the no-shared-middle shortcut. The original separate
line-alignment-only benchmark measured 62,530 us for 600 changed rows;
because its scope differs, do not use it to calculate combined-pipeline
speedup ratios.

## Verification

- Seven unit regressions cover reused projection identity and invalidation,
  scalar/UTF-16 supplementary Unicode offsets, CRLF, whitespace mode,
  unchanged anchors and shared interior lines not being falsely highlighted.
- Full Git Diff widget suite passed 48 tests before memoization.
- Editable-focused Git Diff widget suite passed 6 tests after memoization.
- The benchmark is manual. CI does not assert timing because VM and OS noise
  would cause false regressions.

## Remaining

Complex partially matching edit blocks may still require costly Myers work.
Before implementing a native Rust Rope-to-Rope Diff API, profile those
real-world workloads, UI frame p50/p95 and allocation peaks. A per-keystroke
full-text FFI roundtrip may cost more than the optimized Dart pipeline.

The separate LSP incremental-update and Explorer delta/snapshot workstreams
have not been implemented by this batch.
