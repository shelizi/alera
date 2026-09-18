# C6P retained parser cold-start profile

## Scope

C6P is an evidence gate for the C6 retained Tree-sitter parser lane. This work intentionally does **not** change production scheduling or editor behavior.

Baseline: `c7947b42b623147465e1534844d340b3912d7152`

Branch/worktree:

- `perf/editor-native-parse-profile`
- `.worktrees/editor-native-parse-profile`

Profiler changes:

- `third_party/code_forge/rust/examples/c6p_parse_profile.rs`
- `integration_test/editor_open_profile_benchmark.dart`

The Rust profiler uses the same Rope, Tree-sitter parser, language grammars, highlight queries, and viewport-query machinery as the retained native document path, but runs as a benchmark-only executable.

## Measurement protocol

Each matrix cell has one untimed warm-up and five timed samples.

Synthetic corpora:

- logical lines: 2k / 20k / 50k / 100k
- languages: Rust / Dart / TypeScript
- first viewport: 80 lines

Measured native stages:

1. Rope clone
2. parser creation + language setup
3. full Tree-sitter parse
4. highlight-query compilation
5. first viewport syntax query
6. process working-set movement on Windows

A separate rapid-supersession workload starts three independent 20k Rust cold parses at once and treats only the final generation as useful. This models the current controller behavior where a superseded generation is rejected only **after** `openFromRope` finishes.

Command:

```powershell
cd third_party/code_forge/rust
cargo run --release --example c6p_parse_profile
```

## Final release matrix

Times below are five-sample medians from the final release run.

| Language | Lines | Full parse | Query compile | First viewport query | RSS delta |
| --- | ---: | ---: | ---: | ---: | ---: |
| Rust | 2k | 25.448 ms | 15.212 ms | 0.761 ms | 8.59 MB |
| Rust | 20k | 260.443 ms | 14.693 ms | 0.723 ms | 80.08 MB |
| Rust | 50k | 4,706.590 ms | 104.454 ms | 1.086 ms | 200.28 MB |
| Rust | 100k | 6,275.008 ms | 32.201 ms | 1.234 ms | 399.02 MB |
| Dart | 2k | 112.207 ms | 117.070 ms | 0.351 ms | 3.44 MB |
| Dart | 20k | 587.386 ms | 61.166 ms | 0.367 ms | 37.81 MB |
| Dart | 50k | 1,887.317 ms | 100.627 ms | 0.314 ms | 95.02 MB |
| Dart | 100k | 3,824.708 ms | 59.141 ms | 0.373 ms | 190.41 MB |
| TypeScript | 2k | 13.887 ms | 92.863 ms | 0.721 ms | 0.36 MB |
| TypeScript | 20k | 750.133 ms | 55.145 ms | 0.598 ms | 0.20 MB |
| TypeScript | 50k | 1,846.181 ms | 74.958 ms | 1.098 ms | 0.12 MB |
| TypeScript | 100k | 4,780.498 ms | 81.522 ms | 0.642 ms | 16.51 MB |

Rope clone rounded to 0 microseconds in every timed sample. Parser creation/language setup stayed around 0.004-0.007 ms median across the matrix.

### Variance note

This workstation was also running other parallel Alera work during the profile. The high-size cells therefore have material variance, especially Rust 100k (p95 14.97 s, MAD 2.81 s). The decision-relevant result is still stable across two independent runs: full cold parse grows into multi-second territory, while first viewport query remains around the sub-millisecond to ~1 ms range.

The RSS number is Windows working-set movement relative to the already-running benchmark process. It is useful for high-water/directional evidence, not as a clean per-language retained heap measurement: allocator reuse makes later TypeScript deltas particularly unsuitable for cross-language comparison.

## Rapid supersession result

20k Rust, three generations, five samples:

| Metric | Median | p95 |
| --- | ---: | ---: |
| Wall time | 2,323.992 ms | 2,526.073 ms |
| Total parse work | 5,860.679 ms | 6,465.312 ms |
| Useful final-generation parse | 1,949.679 ms | 2,127.175 ms |
| Superseded/wasted parse work | 3,882.733 ms | 4,338.137 ms |

Two of the three generations are guaranteed to be discarded in this workload, so the theoretical discarded-generation fraction is **66.7%**. The measured CPU-time proxy agrees: roughly 3.88 s of the 5.86 s median native parse work belongs to generations whose result is not used.

This is consistent with the current controller structure:

- `configureNativeSyntaxDocument` advances a Dart-side generation and starts async `NativeEditorDocument.openFromRope`.
- `openFromRope` performs the full native parse/query setup.
- the Dart generation check happens after that open completes.
- therefore supersession prevents publishing a stale document, but does not cancel the expensive cold parse already in progress.

## C6 priority decision

### P0: cancellation / single-flight supersession

C6 should first prevent obsolete cold parses from running to completion.

Preferred shape:

1. only one retained cold-parse job per controller/document lane is admitted at a time;
2. a newer generation marks the older generation cancelled before doing more expensive work;
3. if Tree-sitter supports a cooperative progress/cancellation hook in the pinned version, check cancellation from inside the full parse;
4. otherwise use generation-aware admission/debounce so obsolete jobs do not all enter full parse concurrently;
5. closing a tab/controller must invalidate pending work before it consumes another multi-second parse.

Why first: this is the only measured path that can discard seconds of CPU work without changing syntax correctness. Parser setup and Rope clone are negligible, and viewport querying is already bounded and fast.

### P1: size-aware deferred admission

After supersession is fixed, use file size / logical-line thresholds to delay or deprioritize initial retained parsing for very large documents.

Measured 50k/100k medians are already multi-second for all three languages. Starting that work immediately for a tab the user may leave quickly is poor resource admission even when it does not block the first frame.

A size-aware policy should preserve the existing first-frame path, then start retained parse after the editor becomes useful, with more aggressive debounce/admission for large files.

### P2: parser/query caching only after scheduling

Parser construction is only a few microseconds. Highlight-query compilation is usually tens of milliseconds and is visible, but it is still much smaller than the multi-second full parse at large sizes.

Caching compiled queries may be worthwhile later, but it should not precede cancellation/supersession or size-aware admission.

### Not a priority: viewport query / Rope clone

The first 80-line viewport query remains around 0.3-1.2 ms median even at 100k lines. Rope clone is structurally cheap enough to round to 0 microseconds in this benchmark.

Do not spend the next C6 batch optimizing these stages.

## Flutter / FRB / first-frame instrumentation

`integration_test/editor_open_profile_benchmark.dart` now contains a C6P matrix that records:

- native Rope-ready latency
- first CodeForge frame
- native-open -> first useful frame
- first retained syntax readiness
- native-open -> syntax-ready
- process RSS deltas
- bounded `WorkspaceSourceInfo` JSON payload-size proxy
- rapid replacement behavior

The bounded source-info proxy is asserted below 1 KiB so future C6 work cannot accidentally reintroduce whole-document FRB handoff in this measurement path.

Static validation:

```
flutter analyze integration_test/editor_open_profile_benchmark.dart
No issues found!
```

### Windows C6 after-profile closure

The Windows integration matrix was subsequently closed on the same workstation using the existing Visual Studio 2022 Community installation on `F:` with MSVC 19.38.33133 and Windows SDK 10.0.26100.0.

The benchmark ran all 12 Rust/Dart/TypeScript x 2k/20k/50k/100k cells with five samples per cell and passed end-to-end.

Five-sample medians:

| Language | Lines | Native Rope ready | Native open -> first useful frame | Native open -> syntax ready | Source-info proxy |
| --- | ---: | ---: | ---: | ---: | ---: |
| Rust | 2k | 5.624 ms | 42.588 ms | 232.355 ms | 148 B |
| Rust | 20k | 49.957 ms | 94.664 ms | 1,128.307 ms | 152 B |
| Rust | 50k | 118.534 ms | 158.193 ms | 2,760.323 ms | 152 B |
| Rust | 100k | 237.994 ms | 279.086 ms | 5,214.875 ms | 152 B |
| Dart | 2k | 4.641 ms | 41.704 ms | 163.321 ms | 148 B |
| Dart | 20k | 37.746 ms | 74.391 ms | 902.805 ms | 152 B |
| Dart | 50k | 96.647 ms | 122.134 ms | 1,363.531 ms | 152 B |
| Dart | 100k | 190.147 ms | 221.303 ms | 2,895.852 ms | 152 B |
| TypeScript | 2k | 4.614 ms | 40.604 ms | 174.727 ms | 148 B |
| TypeScript | 20k | 44.671 ms | 70.275 ms | 563.640 ms | 152 B |
| TypeScript | 50k | 105.494 ms | 137.477 ms | 1,263.913 ms | 152 B |
| TypeScript | 100k | 226.783 ms | 253.106 ms | 2,445.289 ms | 152 B |

The result confirms the C6 scheduling objective: large-file retained syntax readiness is still a multi-second operation, but it no longer has to complete before the editor reaches its first useful frame. At 100k lines the first useful frame arrives in roughly 221-279 ms median while retained syntax becomes ready roughly 2.45-5.21 s after open.

The bounded `WorkspaceSourceInfo` proxy remained 148-152 bytes in every cell, well below the 1 KiB guard, so the after-profile did not reveal a whole-document FRB payload regression.

RSS deltas were recorded but remain allocator/GC-sensitive and sometimes negative; use them as directional process-working-set evidence rather than clean retained-heap attribution.

Rapid replacement (20k Rust, three configure generations) also completed:

- replacement issue wall median: 7.536 ms
- final-generation syntax-ready median: 984.197 ms
- process RSS delta median: 933,888 bytes

The production cancellation path therefore preserves fast supersession issuance while allowing the final generation to become ready normally.

## Reproduction / validation

Completed:

- `cargo check --example c6p_parse_profile`
- `rustfmt --check examples/c6p_parse_profile.rs`
- `cargo run --release --example c6p_parse_profile`
- `dart format integration_test/editor_open_profile_benchmark.dart`
- `flutter analyze integration_test/editor_open_profile_benchmark.dart`

Windows after-profile:

- full Windows integration matrix: 12/12 profile cells completed
- rapid replacement workload completed
- final runner result: `All tests passed.`
- `flutter analyze integration_test/editor_open_profile_benchmark.dart`: `No issues found!`

## Handoff to C6

The production lane should consume this result in this order:

1. implement cancellable or single-flight generation supersession;
2. add tests that prove rapid replacement/close does not finish obsolete full parses;
3. add size-aware deferred parse admission for large documents;
4. rerun the C6P Rust matrix and the Flutter C6P matrix;
5. only then consider query caching if profiling still shows it matters.

C6P itself should remain benchmark/report-only.
