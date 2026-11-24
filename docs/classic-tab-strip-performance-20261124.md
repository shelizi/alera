# Alera Classic tab-strip responsiveness — 2026-11-24

## Change

Classic's tab strip rendered pinned tabs before regular tabs, then used
`visualTabs.indexWhere` for every chip on each rebuild. With `n` tabs,
this performed `n * (n + 1) / 2` tab-ID comparisons in the unique-ID case.
At 200 tabs, that is 20,100 comparisons per rebuild before other rendering
work. The replacement builds one visual-index map in a single pass and
performs one map lookup per chip. It retains the same pinned/regular order,
drag-and-drop indices, chip keys, and session ownership.

When rapid selections occurred before the next frame, multiple post-frame
callbacks could attempt to reveal different tabs. Each scheduled reveal now
has a generation; only the most recent callback may scroll the tab strip.
This does not modify the active tab, PTY, saved state, or terminal history.

The complexity comparison is deterministic source-code analysis, **not** a
Windows desktop frame-time, CPU, memory, or latency measurement.

## Verification

- Targeted Workbench tab drop, attention, and placement unit tests:
  24/24 passed.
- Shader assets in the first parallel Widget run were corrupted by
  competing writes to the same test asset directory (SkSL/Vulkan mismatch).
  The one affected Widget test passed in a clean isolated sequential run.
- Full Workbench Widget regression: **57/57 passed** after generated
  test assets were rebuilt by a single Flutter test process (exit 0).
  Includes pinned/regular ordering, tab drag and drop, overflow scrolling,
  active-tab reveal, split panes and preview tabs.
- Dart static analysis of the modified Workbench part file ran for the
  175-second execution budget and timed out without reporting diagnostics.
  This is **inconclusive**, not a clean analyzer pass or a reported error.
  The Flutter Widget suite compiled the changed source successfully.
- Dart formatting: passed (0 changed); Git diff whitespace check: passed.

## Remaining acceptance

Run the matched Classic baseline/candidate native GUI in the same Windows
workspace with large restored tab counts (50, 100, 200), split panes and
both pinned and regular tabs. Compare actual tab-switch frame timings and
check that repeated changes during animation reveal the last active tab.
Do not infer any measured UI acceleration from the comparison counts.
