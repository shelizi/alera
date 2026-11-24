# Classic sidebar projection performance (2026-11-24)

Background agent state changes may invalidate the sidebar row projection.
Previously, manual workspace ordering rebuilt the same global ID-to-position
map for every pinned, regular, archived or section sibling group. Project
sorting also ran independently for the pinned and regular project sections.
Collapsed pinned sections still built workspace trees and agent rows only to
discard them, and collapsed custom sections sorted their hidden members.

This change scopes the lazy manual-order map and sorted project list to a
single projection, preserving ordering and avoiding stale cross-build caches.
A collapsed pinned section computes only its filtered workspace count, which
also preserves the flat All header even when the pinned rows are hidden.
Collapsed custom sections defer workspace sorting until expansion. Project
header agent counts, workspace filters, archived rows and agent activity
ranking retain their existing behavior.

## Verification

- Eight sidebar listing unit suites: 90/90 passed. Two new regressions verify
  a 1,003-ID manual order is read exactly once per projection, and that a
  collapsed pinned-only workspace (with pinned duplicates disabled) does not
  look up its agent status in any of the three grouping modes. Expansion
  restores the pinned row and agent-status lookup.
- Five sidebar/workbench widget suites: 71/71 passed.
- Targeted Dart analysis: no issues found.
- Formatting and Git whitespace checks passed.

The operation-count assertions demonstrate avoided computations, not measured
native Windows FPS, CPU or memory improvements. No installed application or
running agent was changed.
