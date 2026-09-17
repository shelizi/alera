# Alera Rust Heavy-Operation Parallel Work Plan

Status: active coordination plan
Date: 2026-09-17
Baseline reviewed: `main` @ `609bff2b4c5e162c2c04d7e580e10b0f12909a5f`

This document replaces the earlier 2026-09-17 first-pass allocation. Most of the original A/B/E/F/G work has already landed. The remaining high-value work is now concentrated in two architecture tracks: **Terminal worker/model activation** and **NativeEditorDocument production integration**.

The goal is still to move expensive ownership/state to the right boundary, not to move every Dart loop to Rust. Do not reopen completed migrations merely because an older roadmap or handoff still lists them.

## 1. Current repository state used for this plan

At re-audit time:

```text
branch: main
HEAD:   609bff2b4c5e162c2c04d7e580e10b0f12909a5f
```

Tracked `main` files were clean. Existing untracked workspace artifacts remained:

```text
?? .worktrees/
?? docs/history-session/
```

Do not add/delete those paths as part of the work packages below.

The active terminal architecture work is in the existing worktree:

```text
worktree: alera/.worktrees/terminal-parser-worker-phase1
branch:   perf/terminal-parser-worker-phase1
HEAD:     058c937ef7527b4fe2205fe765c3fd8db631e334
```

At this audit that worktree still had local work in:

```text
M  lib/src/features/workbench/presentation/terminal_vt_viewport_model.dart
M  lib/src/features/workbench/presentation/terminal_vt_worker.dart
?? test/unit/terminal_vt_shadow_parity_test.dart
```

Those changes belong to the terminal owner and must not be overwritten by another worker.

## 2. Original work-package status after the first parallel wave

| Old ID | Result | Current decision |
| --- | --- | --- |
| A - Agent runtime overlay | Completed in `0603e080` | Keep; only production benchmark/telemetry remains |
| B - Large-file editor profile gate | Completed in `354f309b`, result B1 | Closed; current hot path is bounded enough |
| C - NativeEditorDocument Phase 1 | Completed in `25c3e664` | **Continue as C2 production integration** |
| D - Terminal search Dart optimization | Superseded by terminal worker/model track | Fold into terminal authority migration |
| E - Regex snapshot/compatibility | Completed in `145206b3` | Closed; keep Dart `RegExp` compatibility authority |
| F - Git history graph projection | Completed in `4837335a` | Closed; no F2 native work justified now |
| G - Workspace search projection | Completed in `6262dc3b` | Closed; no G2 native paging justified now |
| H - Terminal core shadow PoC | No longer deferred | **Reclassified as active Terminal Track** |

## 3. Evidence for closing F/G/E rather than adding more native APIs

### Git history projection

Current five-sample benchmark after `4837335a`:

```text
linear 50:          13.3 us median
merge-heavy 50:     24.7 us
linear 500:         82.6 us
merge-heavy 500:   125.9 us
linear 5000:       916.5 us
merge-heavy 5000: 1338.5 us
```

A ~1.34 ms projection for a synthetic 5k merge-heavy history does not justify a new root FRB graph projection today.

### Workspace search projection

The retained projection work reduced current production-cap warm projection to sub-millisecond cost. Native workspace search currently caps results at 2,000, so another native paging/projection API is not justified until the result cap or measured UI cost changes materially.

### Editor regex

The remaining dominant cost was the full Rope snapshot, not Dart regex execution. Version-aware snapshot reuse reduced repeated multi-MiB query sequences without changing lookaround/backreference semantics. Do not introduce a second regex engine until new profiling after NativeEditorDocument integration proves regex execution itself is material.

## 4. Current priority map

| ID | Priority | Work package | Start now? | Parallel class | Primary owner surface |
| --- | --- | --- | --- | --- | --- |
| T1 | P0 | Terminal worker parity/correctness completion | **Yes; already active** | Terminal exclusive write owner | terminal worker/model files |
| T2 | P0 | Terminal live-shadow + benchmark harness | Yes, harness-first | Parallel with T1 if test/harness-only | new tests/bench/docs first |
| T3 | P0 gate | Terminal production cutover / default-on | After T1/T2 gates | Serialized after parity evidence | terminal runtime integration |
| T4 | P1 | Terminal search/scrollback authority migration | After T3 | Blocked by terminal authority | terminal model/search |
| C2 | P0 | NativeEditorDocument production integration | **Yes** | Independent from Terminal | CodeForge/editor |
| C3 | P1 profile | Editor open/reopen initialization profile | **Yes, profile-only** | Parallel with C2 | benchmark/profile only |
| A2 | P1 validation | Agent overlay production launch benchmark/telemetry | **Yes** | Independent | agent overlay benchmark/tests |
| C4 | P2 | Folding/bracket/symbol/structural features from retained tree | After C2 | Blocked by C2 stability | CodeForge/editor |

There are **five immediate independent lanes**: T1, T2-harness, C2, C3-profile, and A2. Do not create artificial extra work just to fill more workers.

## 5. Terminal Track - active architecture work

The old plan described a future Rust terminal shadow PoC. That description is stale. The branch `perf/terminal-parser-worker-phase1` has already established a much larger worker/model boundary.

Current branch work includes, among other things:

```text
4f662065 perf(terminal): add isolated Ghostty VT worker
86189b40 perf(terminal): stream dirty VT rows from worker
45d3a844 perf(terminal): stream xterm scrollback deltas
b7e5198c perf(terminal): mirror xterm scrollback buffer
0855c098 perf(terminal): mirror xterm render state
1dab0564 perf(terminal): mirror xterm selection queries
930ea242 perf(terminal): abstract terminal search source
3ac2bda5 perf(terminal): build xterm renderer replica
0f5065eb perf(terminal): mirror xterm input state
189cc24d perf(terminal): add opt-in parser worker backend
14a7cdb7 perf(terminal): serialize parser worker resize
df846867 perf(terminal): reset parser worker generations
058c937e perf(terminal): synchronize parser worker focus
```

The branch is approximately 47 files / +6.6k lines relative to the audited `main`. It is no longer a small parser experiment.

### Critical current fact

Production default is still effectively legacy-authoritative:

```text
TerminalXtermRuntime(... parserWorkerEnabled = false)
```

The parser worker is opt-in. Therefore the next objective is **not another terminal implementation**. The objective is to prove parity and safely activate the worker/model path.

### T1 - parity/correctness completion

**Owner:** terminal implementation worker. Reuse the existing `terminal-parser-worker-phase1` worktree; do not create a competing branch for the same files.

Owned files include:

```text
lib/src/features/workbench/presentation/terminal_vt_worker.dart
lib/src/features/workbench/presentation/terminal_vt_viewport_model.dart
lib/src/features/workbench/presentation/terminal_xterm_worker.dart
lib/src/features/workbench/presentation/terminal_xterm_buffer_model.dart
lib/src/features/workbench/presentation/terminal_xterm_replica_terminal.dart
lib/src/features/workbench/presentation/terminal_runtime_parser_worker.dart
related focused unit tests
```

Finish the current dirty batch first: wide/tail-cell row text semantics, resize delta, and Ghostty/xterm shadow parity.

Then extend parity coverage to at least:

1. ASCII + styled cells;
2. CJK wide cells;
3. emoji / surrogate / combining/grapheme sequences;
4. wrapping and reflow across resize;
5. alternate buffer enter/leave/redraw;
6. cursor movement/visibility;
7. DEC/private interaction modes;
8. mouse modes and formats;
9. focus reporting;
10. bracketed paste;
11. title/BEL/PTY writeback effects;
12. OSC 8 hyperlinks;
13. OSC 133/633 shell/prompt metadata used by Alera;
14. OSC 52 behavior where supported by the current model;
15. selection semantics;
16. deep scrollback and eviction boundaries;
17. restore/reattach/snapshot rebuild;
18. real TUI traces captured from Claude/Codex/Pi/Devin-style sessions where practical.

If Ghostty and xterm intentionally differ, document the compatibility rule; do not silently normalize correctness failures away.

### T2 - live-shadow + performance harness

**Owner:** separate benchmark/parity worker.

T2 may start in parallel with T1 **only if it does not edit T1-owned implementation files**. Start by adding independent harness/fixtures/docs against the already committed worker API. If a parity failure requires implementation changes, report it to T1 rather than fixing T1 files in the T2 branch.

Suggested branch/worktree:

```text
perf/terminal-worker-shadow-benchmark
```

Target harness:

```text
PTY bytes
  -> existing authoritative xterm path
  -> worker replica in shadow
  -> compare revisions/state after quiescence
```

Compare at least:

- viewport text/cells;
- cursor position/visibility;
- terminal modes;
- scrollback tail/window hash;
- side-effect sequence;
- resize generation;
- restore generation;
- search-visible text source where applicable.

Benchmark scenarios:

- sustained compiler/log output;
- bursty agent output;
- full-screen TUI repaint;
- hidden terminal + catch-up;
- reveal after large hidden backlog;
- deep scrollback;
- resize storm;
- session restore/reattach.

Collect at minimum:

- UI isolate CPU;
- worker isolate CPU if measurable;
- frame/jank evidence;
- RSS/heap growth;
- bytes/messages crossing isolate boundaries;
- delta row count / full repaint frequency;
- wall time for catch-up/restore.

Use at least five comparable samples for cutover decisions.

### T3 - production cutover gate

Do not default-enable the worker merely because unit tests pass.

Gate sequence:

```text
T1 parity complete
  +
T2 benchmark/shadow evidence acceptable
  -> opt-in runtime flag in real builds
  -> dogfood / fallback telemetry
  -> default-on with legacy fallback
  -> remove duplicate authority only after stable period
```

During the opt-in/default-on stages preserve a quick fallback to the current authoritative xterm path.

### T4 - terminal search/scrollback authority

Do this only after T3 establishes the worker/model as the state authority.

The earlier separate D package is retired. `terminal_search_controller.dart` already has a search-source abstraction, and the worker branch already mirrors scrollback/search-related state. Avoid creating a second independent Rust/Dart search index while authority is still duplicated.

Once worker authority is stable:

```text
worker/model owns scrollback
  -> worker/model owns ordered search index/results
  -> UI requests bounded search/navigation projection
```

### Terminal architecture decision: Rust Host vs isolate worker

Do **not** start a separate wezterm-term/Rust Host rewrite now.

First complete T1-T3 and measure. Escalate to host-owned Rust terminal state only if evidence shows one of these remains dominant:

- isolate message serialization/copy;
- duplicated xterm/worker memory;
- Dart worker scheduling overhead;
- FFI chatter around the native VT/model;
- restore/search state duplication that cannot be removed cleanly.

If those are not material, keeping the terminal model off the UI isolate may deliver the desired responsiveness without another large rewrite.

## 6. C2 - NativeEditorDocument production integration

Phase 1 exists and is validated, but it is not yet production-active.

`NativeEditorDocument` already retains:

```text
Ropey document
revision
Tree-sitter parser/tree
syntax query
line/byte/scalar mapping
explicit lifetime
```

and exposes:

```text
open
applyEdits
querySyntaxSpans
info
close
```

The key Phase 1 benchmark result was that viewport query cost remained around the same ~10-13 ms band from 2k through 100k logical lines. Incremental reparse is substantially cheaper than full open, though it still grows with very large syntax trees.

### Missing production contract

The missing piece is a reliable committed scalar-edit stream from the CodeForge controller/Rope mutation boundary.

Do **not** integrate by calling `controller.text` or otherwise materializing a whole document after each edit. That would reintroduce the large-file regression the earlier work removed.

Target flow:

```text
initial existing document payload
  -> NativeEditorDocument.open once

committed CodeForge edit
  -> scalar start/end + replacement
  -> NativeEditorDocument.applyEdits(expectedRevision, newRevision, edits)

viewport scheduling window
  -> one querySyntaxSpans(viewport + fixed overscan)
  -> cache by native revision/range
  -> reject stale response
  -> paint grammar spans
  -> merge semantic/LSP styling as today
```

### C2 required work

1. identify the single committed edit mutation boundary;
2. expose scalar edit deltas without whole-text reconstruction;
3. define one revision authority and monotonic mapping;
4. open retained native state from already-available initial text;
5. serialize edit deltas to native state;
6. add viewport+overscan syntax query scheduling/debouncing;
7. cache native spans by revision/range;
8. reject stale results;
9. preserve `re_highlight` fallback for unsupported language/error;
10. close native state on controller/document disposal;
11. verify undo/redo, replace-all, paste, multi-step edit and file reload paths;
12. re-enable large-file syntax highlighting only after the bounded path is proven.

Suggested branch/worktree:

```text
perf/editor-native-document-integration
```

### C2 ownership/conflict rule

C2 is the only active worker allowed to change the CodeForge editor production integration while this batch is open. Do not run a native-regex or folding worker against the same CodeForge FRB/generated/controller files in parallel.

## 7. C3 - editor open/reopen initialization profile

B showed sustained editing/scrolling is no longer the main 60 Hz problem. Open/reopen remains a separate initialization cost shape:

```text
wall median:  ~347 ms
build p95:    ~37 ms
raster p95:   ~1.86 ms
```

That points more strongly to initialization/build work than GPU raster work.

C3 is **profile-only while C2 is active**. Do not change CodeForge production files in parallel with C2.

Profile and attribute:

- file decode/read;
- initial Rope construction;
- controller/provider initialization;
- initial editor state projection;
- first viewport text materialization;
- first syntax setup;
- first layout/build chain;
- widget/provider rebuild fan-out;
- native initialization/FFI payloads.

Suggested branch/worktree:

```text
perf/editor-open-profile
```

Deliver evidence and a ranked list of measured contributors. Any production fix touching C2-owned files waits until C2 merges.

## 8. A2 - Agent runtime overlay production validation

A's recursive mirror/copy/delete/link/managed-file reconciliation is already native. Do not rewrite it again.

A2 should prove the production effect and catch regressions:

- terminal launch preparation latency for small/medium/large source trees;
- first launch vs repeated no-op launch;
- link-success vs copy-fallback path;
- Windows behavior;
- error/warning projection;
- UI-isolate stall evidence;
- no leftover recursive Dart filesystem walk in the launch path.

If useful, add low-overhead timing telemetry around the coarse native call, but avoid per-file production logging.

Suggested branch/worktree:

```text
perf/agent-overlay-production-benchmark
```

A2 should normally be tests/benchmark/docs only. Change native overlay behavior only for a demonstrated regression.

## 9. Later work - explicitly blocked

### C4 retained-tree features

Only after C2 is stable should workers reuse the retained Tree-sitter state for:

- folding ranges;
- bracket matching;
- structural selection;
- symbol outline/navigation.

Do them as separate measured packages. Do not bundle all four into C2.

### Native regex

Still blocked unless post-C2 profiling proves regex execution, rather than snapshot/materialization, is the remaining material cost.

### Native Git history projection

Blocked unless real histories substantially above current workloads demonstrate Dart graph projection is a measurable problem.

### Native workspace-search paging

Blocked unless the result cap rises substantially or UI profiling shows result projection/materialization becomes a frame problem.

## 10. Revised parallel staffing

### If five people are available now

```text
Person 1 -> T1 Terminal parity/correctness completion
Person 2 -> T2 Terminal live-shadow + benchmark harness (test/harness ownership only)
Person 3 -> C2 NativeEditorDocument production integration
Person 4 -> C3 Editor open/reopen profiling only
Person 5 -> A2 Agent overlay production benchmark/telemetry
```

These five lanes can run concurrently under the ownership rules above.

### If six people are available

Do **not** create a sixth overlapping implementation branch merely to use the person.

Recommended sixth assignment:

```text
Person 6 -> assist T2 with terminal corpus/fixture collection and real-session parity cases
```

Person 6 should own fixtures/new tests rather than `terminal_*worker.dart` implementation. This keeps T1 as the single implementation owner while increasing parity coverage in parallel.

Alternative after C2's edit-delta interface is frozen:

```text
Person 6 -> C2 integration-test/benchmark subtrack using the frozen adapter interface
```

Again, avoid concurrent edits to CodeForge generated/API/controller files.

## 11. Conflict matrix for the new allocation

Legend: LOW = safe; MEDIUM = coordinate ownership; HIGH = serialize.

| Pair | Risk | Rule |
| --- | --- | --- |
| T1 vs T2 | MEDIUM | T2 adds harness/fixtures only; implementation fixes go through T1 |
| T1 vs C2 | LOW | terminal vs CodeForge editor |
| T1 vs C3 | LOW | terminal vs editor benchmark |
| T1 vs A2 | LOW | terminal vs agent overlay |
| T2 vs C2/C3/A2 | LOW | separate subsystems |
| C2 vs C3 | MEDIUM | C3 profile-only; no production CodeForge changes until C2 merges |
| C2 vs future C4 | HIGH | same retained editor/controller/FRB surface; serialize |
| C2 vs native regex | HIGH | same CodeForge API/generated/dependency lane; serialize |
| C3 vs A2 | LOW | independent benchmarks |
| T3 cutover vs any terminal feature branch | HIGH | one integration owner during default/fallback changes |

## 12. Worktree and merge rules

Every worker must:

1. start from the agreed base or explicitly record a different dependency base;
2. use a dedicated worktree except T1, which must continue the existing terminal worktree;
3. preserve unrelated changes;
4. never use `git add -A` for integration commits;
5. commit exact paths/batches;
6. add focused correctness tests before behavior changes where practical;
7. benchmark before/after for performance claims;
8. report generated-file/dependency conflicts explicitly;
9. re-check `main` immediately before merge;
10. never hand-merge FRB generated function IDs/content hashes.

### Generated surfaces

Root Alera FRB and CodeForge FRB remain separate integration lanes.

For generated conflicts:

```text
merge handwritten source first
-> regenerate from source of truth
-> review generated diff
-> focused tests/analyzer
-> git diff --check
```

Do not preserve generated numbering from two branches manually.

## 13. Revised merge order

Independent benchmark-only work can merge whenever validated:

```text
A2
C3
T2 harness-only batches
```

Architecture lanes:

```text
T1 parity/correctness
  -> rebase T2 harness if needed
  -> T1+T2 gate
  -> T3 opt-in/default-on cutover
  -> T4 worker-owned search/scrollback

C2 production integration
  -> large-file syntax validation
  -> C4 retained-tree features one by one
```

Terminal and C2 may merge in either order because they are separate subsystems.

## 14. Definition of done

A package is not done merely because it compiles.

Required where applicable:

- behavior parity/regression tests;
- stale/generation/revision safety;
- bounded state lifetime;
- explicit fallback semantics;
- reproducible five-sample performance evidence for performance decisions;
- UI-isolate/worker/RSS/FFI evidence appropriate to the package;
- no accidental whole-document/full-history re-materialization in hot paths;
- focused analyzer/lint/tests;
- generated bindings regenerated from source of truth;
- `git diff --check`;
- exact-path commit(s);
- handoff with base, final commits, known limitations and merge order.

## 15. Top-line execution order

The current project order is now:

```text
T1 Terminal parity/correctness
 ||
T2 Terminal live-shadow/benchmark harness
 ||
C2 NativeEditorDocument production wiring
 ||
C3 Editor open/reopen profiling
 ||
A2 Agent overlay production validation

T1 + T2 pass
 -> T3 terminal opt-in/default-on cutover
 -> T4 terminal search/scrollback authority

C2 pass
 -> large-file syntax enabled on bounded native path
 -> C4 folding/brackets/symbols/structural selection one by one

Only if evidence remains
 -> consider Rust Host terminal ownership
 -> consider native regex/F2/G2
```

The coordination rule is now: **do not start more migrations; finish ownership transitions already built, prove parity, then remove duplicate work/state.**
