# Native selected-occurrence search optimization (2026-10-19)

## Scope

Alera already runs editor selection-occurrence highlighting through CodeForge's native `RopeBridge::find_selected_occurrences` in Rust, with a bounded Dart fallback for unavailable native code.

This batch reduces redundant native work; it does **not** move existing Dart code to Rust or change Flutter/FRB API signatures.

## Previous hot path

For every non-overlapping occurrence on a line, Rust previously called `source[..byte_index].chars().count()`, rescanning from the beginning of the UTF-8 line. For 400 dense occurrences on a long line, this grows toward quadratic Unicode-scalar counting.

Other avoidable costs were allocating a new `String` for each ordinary Rope line and invoking the Unicode regex on newly allocated single-character strings for each ASCII identifier boundary.

## Changes

- Track a monotonic byte cursor and scalar column across `str::match_indices`, counting each gap only once; always advance even when a candidate is rejected by identifier-boundary checks or is the active selection.
- Use an allocation-free ASCII predicate `[A-Za-z0-9_$]` for identifier boundaries. Non-ASCII characters retain the original Unicode regex.
- Borrow Rope lines via `RopeSlice::as_str()` when they fit in one chunk. Retain the original materializing fallback for lines spanning multiple chunks, and preserve CRLF/line-ending trimming.
- Do not clone the already-materialized selected line on every scan.
- Preserve 64-scalar maximum selection, 16-Ki UTF-16-line limit, 160-line large-file radius, 400 maximum highlights, Unicode scalar columns, identifier boundaries, hit order, and active-selection exclusion.

## Benchmark

Run:

```powershell
cargo test --release --manifest-path third_party/code_forge/rust/Cargo.toml profile_selected_occurrence_scanner --lib -- --ignored --nocapture
```

Both algorithms run in the same optimized Rust test binary on the same Rope input. The reference is a copy of the pre-change scanner including its per-character regex boundary check. Each workload is timed in 7 post-warmup batches with alternating implementation order; reported values are medians of microseconds per scan.

| Fixture | First old -> new (µs) | Speedup | Repeat old -> new (µs) | Speedup |
| --- | ---: | ---: | ---: | ---: |
| Dense ASCII token, 900 repetitions | 62.6 -> 14.3 | 4.37x | 60.8 -> 13.5 | 4.51x |
| Dense Unicode token, 900 repetitions | 82.5 -> 18.0 | 4.59x | 79.8 -> 17.4 | 4.59x |
| Rejected identifier-boundary matches | 12.4 -> 12.1 | 1.03x | 11.7 -> 11.9 | 0.98x |
| Multi-line, 3,500 lines | 784.1 -> 585.3 | 1.34x | 808.3 -> 593.9 | 1.36x |

The boundary-rejection case is effectively unchanged. These are synthetic native Rust CPU microbenchmarks, not end-to-end Alera editor input/paint latency or p95 UI measurements.

## Verification

- Golden reference parity test compares prior and optimized match positions across identifiers, partial/rejected boundaries, Unicode, CRLF, multi-chunk lines, oversized source lines, 0-400 match limits, and bounded large-file scanning.
- ASCII boundary predicate is checked against the original Unicode regex for all 128 ASCII characters and representative Unicode categories.
- Full `code_forge` Rust library and existing Flutter editor occurrence fallback tests must pass.
- No generated FRB files, Flutter UI source, or native model structures changed.

## Next steps

Profile UI selection-to-highlight latency (including 120-ms debounce, Rust FFI and Flutter repaint), not just Rust CPU. Prioritize terminal scrollback search or Git Diff lazy materialization only with workload-backed traces.
