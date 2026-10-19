# Native Git Diff projection reuse profile

Date: 2026-10-19
Baseline: `0281ce42f824a48376339b0706c2d169f4a455ab`

## Scope

Alera currently renders Git Diff files with both native `side_by_side_projection`
and UTF-16 `git_intraline::project`. Previously, each separately ran
`align_changed_lines` for the same deletion/addition block, potentially
repeating Unicode normalization and O(n*m) line-scoring work.

The new `project_from_side_by_side` accepts the already-computed
`GitDiffSideBySideRow` index projection. Only character-level LCS runs for
paired modified lines. Unpaired additions/deletions still receive full-line
ranges; unchanged context lines receive no character ranges.

Applied to four native Git Diff construction paths:
- Worktree/staged Git Diff from `git_diff_impl::diff_file_for_area_with_whitespace`.
- Commit/range Git Diff from `git_diff_impl::commit_diff_file_for_path`.
- Untracked files (`git_diff_untracked.rs`).
- Git submodule gitlink files (`git_submodule_impl.rs`).

No Flutter UI changes, no FRB model changes, no text duplication, and
no change to the public `git_intraline::project` standalone API.

## Correctness

- Shared projection parity compared with the independent algorithm on
  one-sided hunks, multiple hunks, Unicode including supplementary scalars,
  repeated lines, blank lines, and 70x70 oversized fallback blocks.
- Native intraline suite: 8 passed, 1 manual benchmark ignored.
- Git Diff suite: 34 passed, 0 failed. Real tracked and untracked diffs check
  that computed ranges equal the standalone algorithm.
- Git submodule suite: 13 passed, 0 failed. Gitlink projection parity checked.
- Entire `alera_native` Rust library suite: 269 passed, 0 failed, 2 ignored.
- `git diff --check` on touched Rust source: passed.

## Measurement method

An ignored Rust unit benchmark executes the two production projection
functions in the same test process. Each fixture is deterministic,
runs 7 batches, and reports the median microseconds per full
side-by-side + intraline projection pair.

Run: `cargo test --manifest-path rust/Cargo.toml --lib profile_diff_projection_reuse -- --ignored --nocapture`

| Rust debug case | Original double pass | Shared single pass | Shared / Original |
| --- | ---: | ---: | ---: |
| 10x10 near-match lines (28-char payload) | 2,435.55 us | 2,646.65 us | 1.09 |
| 36x36 near-match lines (48-char payload) | 116,092.03 us | 60,441.20 us | 0.52 |
| 64x64 near-match lines (95-char payload) | 19,143.13 us | 23,116.75 us | 1.21 |

Debug timings are mixed; unoptimized small and large cases may be dominated
by character-level LCS or noisy CPU load. They do not establish a universal
latency gain.

## Optimized Rust (--release) results

Run with:
`cargo test --release --manifest-path rust/Cargo.toml --lib profile_diff_projection_reuse -- --ignored --nocapture`

The two consecutive, warm executions both improved all three fixtures in the
same process and compared the old and shared path at the same optimization level:

| Fixture | First old -> shared (us) | Speedup | Repeat old -> shared (us) | Speedup |
| --- | ---: | ---: | ---: | ---: |
| 10x10, 28 chars | 175.11 -> 101.66 | 1.72x | 175.50 -> 101.53 | 1.73x |
| 36x36, 48 chars | 6,021.26 -> 3,165.13 | 1.90x | 6,069.14 -> 3,129.77 | 1.94x |
| 64x64, 95 chars | 1,570.72 -> 1,497.16 | 1.05x | 1,707.36 -> 1,442.15 | 1.18x |

For medium-sized near-match blocks, sharing removes roughly half of the
combined native projection CPU cost. The 64x64 fixture already takes the
bounded alignment fallback, so its gain is modest. All values are synthetic
native Rust microbenchmark times, not Flutter frame times, file I/O time,
whole Git repository timings, or user-observable UI latency.

## Follow-up

- Retain old standalone projection as a reference for parity.
- Consider realistic large-repository Git Diff end-to-end profiling before
  attributing these native microbenchmark timings to Flutter UI responsiveness.
