# Classic terminal search unchanged-tail optimization (2026-11-24)

When a search is open, terminal output already uses an incremental tail scan
when the buffer identity, first line and width remain stable. Previously, even
an unchanged result caused a copy of the sorted historical prefix, a new
immutable match list, and a search-controller notification on every output
batch.

The controller now compares tail matches against the existing immutable list
by line identity, line index and start/end offsets. It lazily allocates a new
list only at the first difference, including a removed suffix. When results
are identical, it retains the published list, selection and search index
metadata, and skips the search notification. Changes still follow the existing
selection retention and notification path. Buffer resets, resize and head
trimming retain their existing full-rebuild/reindex behavior.

For a stable prefix with 2,001 historical matches, the regression exercises
200 append/rewrite notifications without rebuilding that list or notifying
the search UI. This demonstrates avoided work, not a measured wall-clock
speedup. Search matching itself and PTY output are unchanged.

## Verification

- Flutter terminal search controller and literal-query unit tests: 19/19 passed.
- Terminal surface widget regression suite: 64/64 passed.
- Dart formatter: two source/test files already formatted.
- Git whitespace check: passed.
- Targeted Dart analyzer exceeded its 60-second limit without diagnostics;
  no static analysis pass is claimed.
- Native Windows performance A/B remains unmeasured. No running Alera
  process or main checkout was changed.
