# T6 Terminal Structural Delta Handoff

## Scope

T6 followed the T5 production profile decision tree and selected **T6c: structural full-state traffic reduction**. The first evidence-backed target was resize storms because T5 measured 40 resize callbacks producing 40 worker full repaints and about 141 MB of logical delta payload.

Base: `6c4c7182545c4d23dbf23bfc942664af2ef65d69`

Implementation commit: `343b2c0b74513e233b69ce79f0e2d03d9fc256aa` (`perf(terminal): coalesce parser worker resize storms`)

Branch/worktree: `perf/terminal-structural-delta-coalescing` / `.worktrees/terminal-structural-delta-coalescing`

## Architecture change

Before T6, UI resize callbacks had asymmetric behavior:

- PTY resize already used the 150 ms pending-size debounce and therefore normally received only the final dimensions.
- parser-worker resize was dispatched immediately for every callback.
- every worker resize invalidated the row cache and emitted a full repaint delta.

T6 keeps a separate pending parser-worker size alongside the pending PTY size. A resize burst now overwrites the pending worker size until one of these boundaries occurs:

1. the existing resize debounce flushes;
2. terminal output is about to be queued to the parser worker;
3. a hidden terminal is being synchronized for reveal.

At those boundaries the latest worker size is queued once. The PTY pending-size lifetime remains independent, so a worker flush does not lose a resize that still needs to be delivered when the PTY becomes available.

This preserves the important ordering invariant: if output follows a resize before the debounce expires, the latest resize command is queued before the output parse command. The worker therefore never parses the new output at stale columns/rows.

No worker protocol format, FFI boundary, renderer protocol, or xterm implementation was changed.

## Regression coverage

Added runtime regression coverage for a 40-callback burst using the same dimensions as the T5 resize-storm benchmark. After a warm worker state, the burst plus explicit debounce flush advances the parser-worker replica revision exactly once.

The existing `parser worker resizes before parsing following output` regression remains green and verifies that immediate output after resize still wraps using the new width.

The broader runtime, worker, and replica suites also remain green, including hidden-output/reveal, snapshot rebuild, synchronized updates, alternate buffers, durable sessions, and renderer-adapter tests.

## Five-sample production-worker evidence

The existing `test/benchmarks/terminal_production_worker_profile_benchmark.dart` now includes a paired `coalesced resize storm final size` case. Both cases start with the same 300-line worker/replica state. The legacy case issues all 40 structural resize requests; the T6 case represents the production runtime after coalescing and issues only the final dimensions (`156 x 52`). Each report contains five fresh-worker samples.

| Metric | 40-request resize storm | T6 coalesced final resize | Change |
| --- | ---: | ---: | ---: |
| worker resize requests | 40 | 1 | 40x fewer |
| full repaints | 40 | 1 | 40x fewer |
| wall median | 1652.51 ms | 35.05 ms | ~47.1x lower |
| worker roundtrip median | 1329.27 ms | 31.54 ms | ~42.1x lower |
| replica apply median | 126.97 ms | 0.97 ms | ~130.9x lower |
| changed rows | 19,240 | 301 | ~63.9x fewer |
| changed cells | 1,899,204 | 46,308 | ~41.0x fewer |
| logical delta proxy | 141,510,848 B | 3,443,300 B | ~41.1x lower |

The comparison intentionally measures protocol work eliminated by runtime coalescing rather than claiming the final single worker resize became intrinsically faster.

## Second T6c cut: packed full-buffer reveal transfer

The next T6c target was the large hidden-backlog reveal identified by the first T6 handoff. Stage-local profiling showed that the dominant cost was not xterm parsing or replica apply. For an 8,001-row / 1,238,175-cell reveal, the nested object protocol spent most of its time materializing per-cell lists, copying them across the isolate boundary, and rebuilding Dart objects on the UI isolate.

T6 therefore changes only **full-buffer snapshot** transport. Normal incremental `writeBufferDelta`, partial cell spans, head trims, resize deltas, and search behavior keep the existing row-delta representation.

The full snapshot path now packs numeric cell fields into one `Uint32List`, wraps it in `TransferableTypedData`, and keeps row text plus sparse combining-character/hyperlink metadata separately. The UI decoder reconstructs the existing `TerminalXtermWorkerRowDelta` / `TerminalXtermWorkerRenderCell` API so downstream replica and buffer-model code is unchanged. Render-cell text is lazily derived from `content` + combining characters for packed cells instead of transmitting a redundant string per cell.

Implementation commit: `24f778b9461dfb49bec8eb9bde597b23a62bb00a` (`perf(terminal): pack full buffer snapshot transfer`)

### Five-sample stage-local A/B

`test/benchmarks/terminal_reveal_pipeline_profile_benchmark.dart` compares the old nested object snapshot with the packed transferable snapshot against the same 1,325,790-byte hidden input and 8,001-row / 1,238,175-cell final buffer.

| Metric | nested object rows | packed transferable rows | Change |
| --- | ---: | ---: | ---: |
| raw worker roundtrip median | 3558.70 ms | 257.78 ms | ~13.8x lower |
| worker materialize median | 1154.30 ms | 245.39 ms | ~4.7x lower |
| isolate transfer/scheduling median | 2404.40 ms | 2.69 ms | ~894x lower |
| UI decode median | 863.35 ms | 299.95 ms | ~2.9x lower |
| replica apply median | 289.72 ms | 229.43 ms | ~1.3x lower |
| reveal end-to-end median | 4711.81 ms | 864.19 ms | ~5.45x lower |

The especially large isolate-boundary improvement is the direct reason for using `TransferableTypedData`; this result does not depend on a new worker-side duplicate buffer or search index.

### Production-profile verification

Re-running `test/benchmarks/terminal_production_worker_profile_benchmark.dart` after the packed snapshot cut keeps the same 8,001-row reveal semantics and reports:

- reveal-after-large-hidden-backlog wall median: **651.99 ms**;
- worker roundtrip median: **287.69 ms**;
- replica apply median: **85.22 ms**;
- one full repaint, 8,001 changed rows, 1,238,175 changed cells.

The older T6 baseline recorded roughly 1,238.84 ms wall / 1,099.79 ms worker roundtrip for the same logical reveal case. Machine/JIT noise makes cross-run wall values less controlled than the paired A/B benchmark, but both measurements point in the same direction and the paired benchmark isolates the transport improvement directly.

## Validation

Focused correctness:

```text
flutter test --no-pub test/unit/terminal_runtime_native_test.dart --plain-name "parser worker resizes before parsing following output"
flutter test --no-pub test/unit/terminal_runtime_native_test.dart --plain-name "parser worker coalesces a resize burst to the final size"
```

Both pass.

Full T6 terminal regression set after both T6c cuts:

```text
flutter test --no-pub \
  test/unit/terminal_runtime_native_test.dart \
  test/unit/terminal_xterm_worker_test.dart \
  test/unit/terminal_xterm_replica_terminal_test.dart \
  test/unit/terminal_xterm_buffer_model_test.dart \
  test/unit/terminal_search_controller_test.dart
```

Result: **188 passed, 2 skipped**. The two skips are the existing POSIX FFI cases on Windows.

Benchmark:

```text
flutter test --no-pub test/benchmarks/terminal_production_worker_profile_benchmark.dart
```

Result: pass; five measured samples for every scenario.

Static checks:

```text
dart analyze <7 T6 changed Dart files>
git diff --check
```

Result: **No issues found** and diff check passed.

## Worktree dependency note

Fresh linked worktrees did not initially have usable `third_party/dart_terminal` / `third_party/xterm` submodule working trees. `dart_terminal` was initialized normally. The xterm remote no longer advertised the superproject-pinned commit `0c43af05aac9613b44c7d88e70f83cd1a8133591`, so the T6 worktree was seeded from the already verified local main submodule object store and checked out at exactly that pinned commit. No submodule pointer or vendored code change is part of T6.

## Remaining T6c direction

The two evidence-backed structural hotspots are now addressed: resize storms are coalesced at the runtime boundary, and full reveal/initial snapshots use packed transferable cell storage. Further T6 work should return to profiling rather than automatically adding more protocol complexity. The current packed reveal profile shows UI decode/materialization and replica application as the remaining meaningful costs; any next cut should demonstrate a repeatable gain there while preserving stable row IDs, head trims, wide/combining cells, hyperlinks, search refresh, and reveal atomicity. Do not introduce a duplicate hidden-buffer/search index without new evidence.
