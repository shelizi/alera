# Workspace search projection benchmark

Work package: `G` from `docs/rust-heavy-operation-parallel-work-plan.md`.

## Goal

Measure the Dart-side cost of projecting native workspace-search results into the grouped/collapsible result tree, then decide whether a new root FRB paging/projection API is justified.

The pre-G1 path rebuilt the directory tree, reparsed every relative path, copied child/file lists, and sorted them again whenever collapse state changed. G1 retains an immutable projection per native result, so path parsing/grouping/sorting happens once and later expand/collapse updates only flatten the visible rows.

## Harness

Run:

```text
dart run tool/bench/workspace_search_projection_bench.dart
```

The harness uses one match per synthetic file and runs five comparable samples for each size. `legacy expanded rebuild` mirrors the pre-G1 Dart grouping/sorting path. `retained cold expanded` includes the one-time projection build; `retained warm expanded` reuses that projection after a collapse-state change.

Current native workspace search clamps `max_results` to 2,000, so 2,000 is the production ceiling. The 10k and 50k cases are forward-looking stress cases.

## Results

Measured on 2026-09-17 in the project worktree:

| Matches | Legacy expanded rebuild median / p95 | Retained cold expanded median / p95 | Retained warm expanded median / p95 | Retained all-collapsed median / p95 |
| ---: | ---: | ---: | ---: | ---: |
| 1,000 | 4.057 / 22.802 ms | 3.763 / 10.227 ms | 0.197 / 0.313 ms | 0.005 / 0.138 ms |
| 2,000 | 3.644 / 6.473 ms | 4.144 / 6.097 ms | 0.109 / 0.242 ms | 0.005 / 0.049 ms |
| 10,000 | 16.430 / 18.585 ms | 18.255 / 19.192 ms | 0.594 / 0.617 ms | 0.004 / 0.005 ms |
| 50,000 | 86.374 / 91.411 ms | 86.875 / 89.956 ms | 4.135 / 5.172 ms | 0.007 / 0.018 ms |

The cold path intentionally remains approximately the same order of cost because the same grouping/sorting work still has to happen once for a new native result. The improvement is on repeated UI projection: at the current 2,000-match production cap, warm expanded projection measured 0.109 ms median and 0.242 ms p95 rather than rebuilding the tree.

## G2 decision

Do **not** add native paging/projection or another root FRB API in this work package.

Reasons:

- native search currently caps returned matches at 2,000;
- the expensive Dart grouping/sorting work is now paid once per native result rather than on every collapse/expand state change;
- the 2,000-match warm projection is sub-millisecond in this harness;
- even the 50,000-match forward-looking stress case remains about 4 ms median for warm expanded flattening;
- a native paging API would add cancellation, ordering, page-boundary, replacement, FRB-generation, and compatibility surface without benchmark evidence that it is needed today.

Revisit G2 only if the native result cap is raised substantially or profiling shows projection/materialization, rather than native search or widget rendering, becoming a measurable frame-budget problem.
