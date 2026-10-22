# Terminal Scrollback search: incremental tail match projection (2026-10-19)

## Motivation

Alera's TerminalSearchController already stores matched lines keyed by stable
buffer line ID and scans only the tail during ordinary output. However, every
update used to reconstruct a global match list: it called `lineIndexOf` on
every cached matching line, sorted all matching lines, created each
TerminalSearchMatch again, and searched the entire result list to retain
selection. For deep 100,000-line scrollback containing 25,000 matches this
cost can dominate an output batch even when only one row changed.

## Change

- Remember the first indexed line ID alongside source generation/size.
- When a normal tail append or same-height TUI rewrite leaves the buffer
  identity and first line ID stable, preserve the already ordered match prefix.
- Use a binary search by lineIndex to find the modified tail boundary and
  reconstruct matches only for that suffix. Reuse earlier match objects and
  retain the selected prefix entry in O(1).
- If the buffer is replaced, resized, shortened, or its first line ID changes
  (including circular head trimming at a constant height), fall back to the
  authoritative full reindex/sort/stale-ID cleanup.
- Keep query semantics, per-line index, ordering, selected-match behavior,
  source attachment, visible-tail TUI handling, and public API unchanged.

No second terminal index, Rust parser migration, worker protocol, or FRB
surface changes were introduced. This optimization removes redundant work
on the existing Flutter-side match presentation and does not duplicate
terminal scrollback state.

## Deterministic targeted benchmark

Run on the same development machine, Flutter SDK, power profile and
build mode:

```text
flutter test --no-pub benchmark/terminal_search_incremental_benchmark.dart
```

The harness uses a stable mutable `TerminalSearchSource` holding 100,000
rows, with one hit per 4 lines (dense, 25,000 matches) or one hit per 1,000
lines (sparse, 100 matches). Each of 7 rounds constructs a new controller
and times the initial query, 20 appended output updates, and 20 last-line
rewrites. Round one warms the VM; the following six batches are sorted and
the median recorded. Results are microseconds per operation.

| Scenario | Before initial | Before append | Before TUI rewrite |
| --- | ---: | ---: | ---: |
| Dense (25,000 hits) | 36,445 us | 19,426 us | 19,701 us |
| Sparse (100 hits) | 36,378 us | 124 us | 86 us |

| Scenario | After initial, run A/B | After append, run A/B | After rewrite, run A/B |
| --- | ---: | ---: | ---: |
| Dense | 37,324 / 37,327 us | 692 / 549 us | 758 / 810 us |
| Sparse | 14,996 / 15,191 us | 53 / 53 us | 10 / 9 us |

Dense high-hit output updates therefore improved approximately **28–35x**
for append and **24–26x** for rewrite across these runs. Initial dense
query did not improve. Sparse initial query readings differed markedly and
are treated as harness noise, not an established speedup.

These figures are Flutter test microbenchmarks over a fake in-memory search
source. They exclude real worker parsing, transport, terminal painting,
release AOT, memory/RSS, and full app/UI frame timings. They cannot be
treated as desktop-wide terminal speedups.

## Correctness

```text
flutter test --no-pub test/unit/terminal_search_controller_test.dart
  test/unit/terminal_runtime_native_test.dart
```

Result: 156 passed, 2 skipped platform-specific cases. New tests explicitly
exercise:
- Tail append preserving sorted hit order, identity and active selection
- Same-height TUI suffix rewrite adding/removing hits
- Constant-height circular head trim reindexing surviving IDs and selection

The existing worker-replica source, output updates, restore and navigation
regressions also passed.

## Future work

- Inspect 100k initial full-query scan for repeated per-line singleton
  allocations and repeated query case folding, after profiling.
- Check actual desktop input-to-highlight latency and p95 UI frames with the
  parser worker attached. Do not create a Rust-hosted second scrollback index
  without evidence of an additional bottleneck.
