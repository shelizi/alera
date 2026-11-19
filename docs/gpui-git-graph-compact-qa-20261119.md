# Native Git Graph compact split-pane and 512-row Windows QA — 2026-11-19

## Scope and implementation
- Work starts from `main` at `572c84b17`; it keeps the existing 512-row cached-topology graph, per-file committed-tip comparison, branch picker and all Git write gates intact.
- Reproduced an actual UI issue in a narrow **secondary Graph pane**: the long Graph title, query placeholder and perspective/merged toolbar labels wrapped into multiple lines and overlapped their fixed-height controls.
- The compact header now presents `Graph · N`; the single-line QuickOpen search and placeholder are clipped to their bounded viewport without altering multiline profile inputs or Windows IME capture; the perspective/merged/compare actions remain accessible as icons with tooltips in a horizontally scrollable, fixed-height toolbar. The full-width layout is unchanged.
- Compact selection uses GPUI *logical* viewport pixels and the existing split-pane identity, not physical monitor pixels; unit tests cover the 720 logical-pixel threshold with and without a secondary pane and the count-bearing compact title.
- When a comparison is visible in compact mode, the scrollable Graph preview is 120 **logical** px high, leaving room for the changed-file list, old/new gutters and hunks in the Diff below. Full-width Graph and non-comparing Graph layouts are unchanged.

## Windows GUI acceptance
- A frozen isolated Rust Debug GPUI executable with its own `ALERA_GPUI_STATE_DIR`, `ALERA_RUNTIME_DIR`, and deliberately non-installed `ALERA_CLI_PATH` was opened on the actual Windows desktop against the local Alera repository; neither installed Alera nor its state was replaced.
- Before the fix, the nested split reduced Graph to about 195 image pixels wide; search placeholder and toolbar text visibly wrapped and collided. After the fix, compact header, single-line search, fixed icon toolbar and branch/compare menus were visible and clickable.
- Input search `virtualize` filtered to **2** history commits in the narrow pane. Searching the comparison branch picker for `feat/gpui-bidiff-hunk-gutters-20261119` resolved the expected local branch and opened the immutable branch-tip comparison; its opposite direction also loaded correctly.
- The 120px Graph preview permits visibly more Patch lines and leaves old/new gutters usable within a narrow comparison pane. The underlying file comparison stays read-only.
- Created an unsaved `Untitled` Editor scratch tab containing `g` in Primary while the Git Graph and comparison occupied Secondary. Reversing compare direction did not remove it; moving Git Graph back to Primary and reselecting the unsaved Editor still displayed `g`.
- Window OS `GetDpiForWindow` returned **120 DPI = 125% scaling** at the time of testing. Other monitor DPI values and light theme were **not** tested.
- The existing 512-row virtualized graph loaded `512/512` commits after repeated Load More actions and navigated older commits, maintaining visible topology and responsiveness. A one-time process snapshot after the scroll gave working set **130.4 MiB**, private memory **210.5 MiB**, responding **True**; this is not a time-series/throughput or leak claim.
- Native GUI binary test suite after the complete compact-toolbar and 120px-viewport fix: **67/67 PASS**, including two compact-breakpoint/title tests. Rust 1.98 offline locked check/build: **PASS**, with only the two unchanged pre-existing unused-code warnings. `rustfmt --check` and `git diff --check`: **PASS**.
- Git history and virtualized graph targeted library regression: **15/15 PASS**, including a real 513-commit fixture verifying the bounded 512-item visible page and history search/scroll, plus mixed-pane tab movement without duplicating Editor or PTY ownership.

## Still outstanding
- Live Host-attached Terminal/PTY and cross-pane keyboard, resize and restore acceptance — **not verified** because this QA build intentionally disabled its CLI Host path.
- Genuine 100%, 150%, 200% DPI GUI comparisons and light theme review.
- Long-duration 512-row throughput, latency distribution and memory-retention measurements beyond the responsive scrolling and single snapshot described above.
