# T4 Terminal Search Replica Source Handoff

Date: 2026-09-17

## Scope

T4 from `docs/rust-heavy-operation-parallel-work-plan.md` is complete on branch `perf/terminal-search-replica-source`.

- Base: `f5e0cb2ffafd82e5a95d8c6fc8f0ae2fa022482c`
- Implementation commit: `f2c78cfc` (`perf(terminal): search worker replica model directly`)
- Production ownership: terminal search source selection / restore reattachment only
- No worker protocol, renderer, parser, PTY, FFI, or generated binding changes
- No second worker-side search index was added

## What changed

Parser-worker sessions now attach `TerminalSearchController` directly to the authoritative `TerminalXtermBufferModel` exposed by `TerminalXtermReplicaTerminal.replicaModel`.

The runtime keeps the generic `XtermTerminalSearchSource` fallback for non-replica / legacy paths. Snapshot rebuilds use the same source-selection helper, so replacing the xterm facade reattaches search to the new replica model rather than silently falling back to the facade.

The resulting worker path is:

```text
worker authoritative buffer
  -> TerminalXtermBufferModel (TerminalSearchSource)
  -> TerminalSearchController
```

instead of:

```text
worker authoritative buffer
  -> TerminalXtermBufferModel
  -> TerminalXtermReplicaTerminal xterm facade
  -> XtermTerminalSearchSource
  -> TerminalSearchController
```

## Regression coverage

Added/extended coverage for:

- same-height TUI viewport rewrites while preserving the selected match;
- stable match behavior across circular head trims;
- parser-worker session ownership of `replicaModel`;
- snapshot rebuild / restore source replacement;
- hidden parser-worker output followed by reveal with an active search;
- existing search open/close listener lifetime and navigation behavior.

Final focused validation:

```text
flutter test --no-pub \
  test/unit/terminal_search_controller_test.dart \
  test/unit/terminal_runtime_native_test.dart
```

Result: **143 passed, 2 skipped**. The two skips are existing POSIX FFI cases unavailable on Windows.

Static validation:

```text
flutter analyze --no-pub <8 T4 production/test/benchmark files>
```

Result: **No issues found**.

`git diff --check` also passed.

## Benchmark

Benchmark file:

```text
benchmark/terminal_search_benchmark.dart
```

Run with:

```text
flutter test --no-pub benchmark/terminal_search_benchmark.dart
```

The benchmark compares the direct `replicaModel` source with the legacy xterm-facade source using identical worker-produced deltas. It covers 10k/100k logical lines, low/high match density, active output, and next/previous navigation.

The harness performs symmetric scan/navigation warm-up and an untimed active-output warm-up before measurement to avoid first-JIT/order bias. Five process-level samples were then collected sequentially.

### Five-sample median

| Scenario | Matches | Direct scan | Legacy facade scan | Legacy / direct | Direct active output | Legacy active output |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 10k low density | 10 | 9.11 ms | 11.32 ms | 1.24x | 287 us | 146 us |
| 10k high density | 2,500 | 12.02 ms | 11.71 ms | 0.97x | 2.008 ms | 2.576 ms |
| 100k low density | 100 | 22.87 ms | 65.63 ms | 2.87x | 153 us | 106 us |
| 100k high density | 25,000 | 75.24 ms | 94.22 ms | 1.25x | 21.895 ms | 24.321 ms |

Interpretation:

- The strongest repeatable win is large-history scan cost, especially low-density 100k scrollback (about 2.87x lower median scan time).
- 100k high-density scan is also lower (about 1.25x).
- 10k low-density improves modestly; 10k high-density is effectively parity and was about 2.6% slower at the median.
- Active-output cost is mixed: direct source is tens to roughly 140 microseconds slower in the low-density cases, while it is faster in both high-density cases. There is no new millisecond-scale low-density output penalty after benchmark warm-up.
- Navigation is sub-millisecond and noisy in this harness; after the match index is built it does not meaningfully exercise the source abstraction, so it is retained as a correctness/sanity measurement rather than a source-cutover performance claim.

No RSS or FFI payload delta is expected from T4 because the worker protocol and FFI surfaces are unchanged and no duplicate match index was introduced.

## Merge / conflict notes

T4 owns terminal search source wiring. It may overlap textually with terminal profiling work if T5 violated its profile-only constraint, and it should land before any T6 production terminal optimization.

If `main` advanced after the base commit, integrate the two T4 commits onto current `main`, resolve only genuine terminal-runtime conflicts, then rerun the focused search + runtime suites, analyzer, and `git diff --check`.

The benchmark is intentionally under `benchmark/` rather than `integration_test/`; it is headless and does not require the Windows desktop C++/CMake toolchain.
