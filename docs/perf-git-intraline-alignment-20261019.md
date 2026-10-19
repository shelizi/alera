# Git Diff native line-alignment CPU profile (2026-10-19)

## Goal

Reduce the CPU and temporary-allocation overhead of Rust Git Diff line alignment
introduced with content-aware replacement-line pairing. This optimization must
not change replacement pairing, UTF-16 character ranges, or diff presentation.

## Benchmark and environment

- Baseline: `main` commit `c42a65ec9714edfe0c01f1a722de9ac0fcd0cf96`.
- Compare native `git_intraline::align_changed_lines` directly, without Flutter
  or a Dart/FFI boundary.
- Compile with `rustc --edition=2021 -O benchmark/git_intraline_alignment_benchmark.rs -o .git/git-intraline-bench.exe`.
- Execute `.git/git-intraline-bench.exe` from the repository root on Windows.
- Seven timing batches per fixture; numbers are medians of microseconds per
  alignment after running the specified number of iterations per batch.
- Fixture contents are deterministic ASCII code-like strings with a change near
  the middle. They intentionally exercise ordered near-matches, not arbitrary
  real-world file distributions.

| Fixture | Old median | New median | Speedup | Saved time |
| --- | ---: | ---: | ---: | ---: |
| 10x10 lines, 28-char payload, 180 iterations | 197.86 us | 68.74 us | 2.88x | 65.3% |
| 36x36 lines, 48-char payload, 28 iterations | 4940.31 us | 2853.80 us | 1.73x | 42.2% |
| 64x64 lines, 95-char payload, 28 iterations | 1476.89 us | 96.59 us | 15.3x | 93.5% |

The above are standalone microbenchmark results on one Windows machine;
they are *not* representative measurements of end-to-end Alera UI speed,
whole-repository diffs, memory reduction, or cold-start time. Use repeated
profiles with realistic Git workloads before making broader claims.

## Changes

- Prepare trimmed Unicode scalar arrays once per hunk line, not once for each
  old/new line pair.
- Reuse two LCS dynamic-programming rows for all candidate comparisons.
- Select the shorter string for the DP row width.
- Exclude identical prefix and suffix from LCS work, including the same
  characters in the original score so line-pairing semantics are preserved.
- Fast-path pure-addition and pure-deletion hunks without decoding or scoring.
- Keep the original 4096 similarity cell and line-alignment bounds unchanged.
- Add reference-algorithm parity tests for diverse Unicode, whitespace,
  duplicate, and long-line input strings, plus one-sided hunk tests.

## Verification (2026-10-19)

- Intraline regression suite: 7 passed, 0 failed.
- Git Diff/side-by-side projection suite: 34 passed, 0 failed.
- Final benchmark recompiled from current source; results in table above.
- This work does not include a native Windows GUI E2E benchmark.

## Follow-up checks

- Test command: `cargo test --manifest-path rust/Cargo.toml --lib git_intraline`.
- Check `cargo test --manifest-path rust/Cargo.toml --lib git_diff` for upstream
  projection compatibility.
- Future profiling: investigate duplicate alignment calls between native
  side-by-side projection and intraline projection, and whether carrying
  alignment results through both projections reduces CPU/FFI overhead.
- Leave Dart fallback semantics untouched: this is a Rust-only optimization.
