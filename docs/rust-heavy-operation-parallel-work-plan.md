# Alera Rust Heavy-Operation Parallel Work Plan

Status: active coordination plan
Date: 2026-09-17
Baseline reviewed: `main` @ `3b354577518268b9ef7f8d5d37379c54c157e6d2`

This document is the current source of truth for the next heavy-operation / Rust performance phase. It supersedes the earlier same-day allocation at `856051cb`: the terminal parser worker is already enabled in production, NativeEditorDocument is already wired into CodeForge, editor open/reopen profiling is complete, and agent-overlay production validation is complete.

The coordination rule for this phase is: **finish the measured ownership/copy problems that remain; do not reopen migrations whose evidence gate has already passed.**

## 1. Repository snapshot used for this re-sync

At re-audit time:

```text
branch: main
HEAD:   3b354577518268b9ef7f8d5d37379c54c157e6d2
upstream: origin/main
ahead: 721
behind: 84
```

Tracked `main` files were clean. Existing workspace artifacts remained untracked:

```text
?? .worktrees/
?? docs/history-session/
```

Do not add/delete those paths as part of a work package. The existing upstream divergence is not part of this performance phase and must not be reconciled implicitly.

## 2. What changed since the previous plan

### 2.1 Terminal parser/model worker is production-active

The old plan treated production cutover as future T3 work. That is stale.

Current production provider constructs:

```text
XtermTerminalRuntime(parserWorkerEnabled: true, ...)
```

The worker isolate is now the authoritative parser/model. The UI isolate uses `TerminalXtermReplicaTerminal` to reconstruct the xterm-compatible render surface from worker deltas without parsing PTY output itself.

Relevant landed work includes:

```text
1c91aba8 perf(terminal): enable parser worker in production
8be432ce perf(terminal): adapt to replica apply cost
2e74a53e perf(terminal): stream partial xterm cell spans
75eb27af perf(terminal): update buffer mirror in place
1431f60f perf(terminal): amortize buffer mirror head trims
517725da perf(terminal): skip immutable scrollback comparisons
560eaae9 perf(terminal): coalesce synchronized xterm updates
df0b3f0b perf(terminal): skip input-only repaints
999f7cbe perf(terminal): cache only active xterm rows
5f39aa1d merge: terminal parser worker phase 1
```

Therefore **do not assign another worker to T1/T3 parser-worker cutover**.

### 2.2 The preserved Ghostty worktree is not active production work

The worktree below is still attached:

```text
alera/.worktrees/terminal-parser-worker-phase1
branch: perf/terminal-parser-worker-phase1
HEAD:   999f7cbe
```

Its only remaining dirt is the older Ghostty alternative-backend experiment:

```text
M  lib/src/features/workbench/presentation/terminal_vt_viewport_model.dart
M  lib/src/features/workbench/presentation/terminal_vt_worker.dart
?? test/unit/terminal_vt_shadow_parity_test.dart
```

Those files remain intentionally uncommitted because Ghostty resize/reflow did not match current xterm semantics. **Leave them unstaged and do not use that worktree as the base of new production terminal work.** Reopen the Ghostty alternative only by an explicit architecture decision.

### 2.3 NativeEditorDocument production wiring is complete

`6a061b2d perf(editor): wire retained native document into CodeForge` landed the former C2 package.

Current production CodeForge now:

- opens `NativeEditorDocument`;
- emits committed Unicode-scalar edit deltas;
- applies monotonic native revisions;
- queries viewport + overscan syntax spans;
- rejects stale responses;
- preserves fallback behavior;
- schedules native highlighting from the visible viewport.

This closes the old "native document exists but is not connected to production" gap.

### 2.4 Editor open/reopen profiling is complete

`9dbfb382 perf(editor): profile open initialization` completed C3.

Five-sample 50k-line / ~4.0 MiB profiling found:

| Stage | Median | p95 | Interpretation |
| --- | ---: | ---: | --- |
| production native editor read | 259.22 ms | 262.09 ms | Rust read/decode/normalization + full String back to Dart |
| initial Rope construction | 257.82 ms | 274.11 ms | full Dart String sent back into CodeForge native Rope |
| first CodeForge frame | 59.28 ms | 61.03 ms | state/layout after controller/Rope construction |
| controller init | 0.05 ms | 0.23 ms | negligible |
| full open -> first frame | 544.27 ms | 564.30 ms | independent end-to-end measurement |

Raw warm file I/O (~3.70 ms) and plain Dart UTF-8 decode (~0.98 ms) are not the dominant cost. The measured problem is **whole-document ownership crossing native -> Dart -> native**.

Because C3 was measured before C2 production wiring, the next editor worker must first re-run the same benchmark on latest `main`; C2 may add another large native-document open/parse stage, so the latest-main attribution must be recorded before changing architecture.

### 2.5 Agent-overlay production validation is complete

`4acc67dd perf: validate agent overlay production path` is merged through `3b354577`.

On Windows through the actual FRB path:

| Files | First median | Repeated unchanged median |
| ---: | ---: | ---: |
| 20 | 6.297 ms | 7.404 ms |
| 500 | 115.438 ms | 155.768 ms |
| 2000 | 478.278 ms | 636.891 ms |

The Dart event loop remained responsive during native work. The remaining measured opportunity is clear: **repeated unchanged preparation currently deletes/rebuilds the overlay instead of taking a no-op/reuse fast path.**

### 2.6 Old editor performance worktrees are not outstanding work

The following branches still appear as separate worktrees/refs, but `git cherry main <branch>` reports their patches as already equivalent in `main`:

```text
perf/codeforge-large-file-fastpath
perf/codeforge-large-scroll-fastpath
perf/editor-change-refresh
perf/editor-document-version
perf/editor-large-file-highlight
perf/editor-large-snapshot-debounce
perf/markdown-preview-debounce
```

Do not assign workers to "finish" these branches merely because their original commit hashes are not literal ancestors of current main. They are superseded by patch-equivalent main history.

## 3. Next-phase package summary

| ID | Priority | Work package | Start now? | Parallel rule |
| --- | --- | --- | --- | --- |
| C4 | P0 | Editor native-open handoff / remove whole-text round-trip | **Yes** | Single editor architecture owner |
| T4 | P1 | Terminal search source cutover to replica model | **Yes** | Independent from C4/A3; coordinate with T5 |
| T5 | P1 gate | Production terminal worker/render/restore profiling | **Yes, profile-only** | May run with T4 if it does not change terminal production code |
| A3 | P1 | Agent overlay repeated-unchanged fast path | **Yes** | Independent |
| S1 | P2 gate | Process-cold Alera + CodeForge Rust initialization profile | **Yes, profile-only** | Independent |
| C5 | P2 | Retained-tree folding/brackets/symbols/structural features | **No; after C4** | Same CodeForge/native-document lane |
| T6 | Evidence only | Renderer/delta/restore optimization selected by T5 | **No; after T5** | Same terminal lane |

Five primary workers can therefore run immediately: **C4 || T4 || T5 || A3 || S1**.

## 4. C4 - Editor native-open handoff / remove whole-text round-trip

**Priority: P0. Highest measured remaining editor cost.**

### Current production path

The current workspace editor does roughly:

```text
Rust workspace file API
  -> read + decode + normalize
  -> WorkspaceEditorTextFile(rawContent, displayContent, metadata)
  -> full Strings cross FRB to Dart
  -> EditorDocumentSession stores loaded/current Strings
  -> _controller.text = currentText
  -> CodeForge builds native Rope from the full String
  -> NativeEditorDocument opens from logical text for retained syntax state
```

The pre-C2 profile already measured ~259 ms for the first whole-text boundary and ~258 ms for the next Rope construction. C2 now also opens retained native syntax state, so **latest-main re-baselining is mandatory before implementation**.

### C4a - mandatory latest-main gate

First re-run/adapt `integration_test/editor_open_profile_benchmark.dart` on the current `main` and split at least:

- workspace native read/decode;
- Dart `EditorDocumentSession` acceptance;
- CodeForge Rope creation;
- NativeEditorDocument open/initial parse;
- first visible syntax query;
- first CodeForge frame;
- end-to-end open -> first useful frame.

Use the same 50k-line fixture and five samples so the result is comparable to C3.

### C4b - architecture target

Remove the unnecessary large-file ownership bounce. The desired property is:

```text
file bytes/text become native editor state once
-> Dart receives bounded metadata/state
-> viewport/edit operations stay incremental
-> a full Dart String is materialized only when a compatibility operation truly needs it
```

Do not assume that a Rust opaque handle from the root Alera FRB library can simply be passed into the separate CodeForge FRB library. They are separate native/FRB surfaces. The worker must choose and document a safe ownership design rather than hand-wave cross-library handle sharing.

Candidate approaches to evaluate include:

1. a CodeForge native open-from-file/source API with Alera-owned validated path/encoding metadata;
2. a shared/native document owner factored below both surfaces;
3. another bounded-transfer design that avoids returning a multi-MiB Dart String only to immediately send it back native.

Choose by measured cost, correctness, packaging complexity, and lifetime semantics.

### Compatibility requirements

The new path must preserve:

- workspace path containment/protected-path behavior;
- encoding detection/reopen-with-encoding;
- BOM and legacy encodings;
- tab/display normalization semantics;
- external-file-change detection and F5 reload;
- dirty state;
- save conflict/content-token checks;
- autosave;
- undo/redo;
- reload/discard;
- current large-file edit-buffer behavior;
- NativeEditorDocument revision/stale guarantees;
- unsupported-language fallback.

`EditorDocumentSession` currently stores `loadedRawText`, `loadedText`, and `currentText`; C4 must explicitly decide which of these can become lazy/versioned/native-backed. Do not delete that state until save/reload/diff semantics have replacement coverage.

### C4 success criteria

- latest-main benchmark exists before and after;
- the large-file open path no longer performs redundant full native -> Dart -> native text round-trips;
- no new full-document snapshot appears in edit/paint/scroll hot paths;
- initial/reopen latency is materially lower on the same fixture;
- memory/RSS and FFI payload evidence is included;
- focused load/save/reload/encoding/external-change tests pass.

Suggested branch/worktree:

```text
perf/editor-native-open-handoff
```

C4 is the sole production owner for this editor/open architecture batch.

### C4 implementation status - 2026-09-17

Implementation is merged to `main` (`bdbaa84d` implementation, `8dc011ef` merge). The selected ownership design is CodeForge-native open-from-workspace-file: CodeForge reads/decodes the validated workspace source directly into its native Rope, Dart retains bounded encoding/content-token/revision metadata, and `NativeEditorDocument` opens by structurally cloning that Rope rather than round-tripping a full Dart String. Native-backed `EditorDocumentSession` dirty state is revision-based; full text is materialized only at compatibility boundaries such as Save or an explicit dirty snapshot consumer.

Validation completed:

- CodeForge Rust: 13 passed, 0 failed, 1 manual benchmark ignored;
- native-backed Dart session regressions: 2 passed;
- root Rust native-backed Save/tab-preservation regression: passed;
- targeted Dart analyze for the C4 production/test files: no issues;
- terminal provider compile blocker fixed by exposing the shell-launch builder across libraries (`d45ff07d` implementation; merged to `main`); targeted terminal analyze: no issues;
- broader workspace editor widget suite rerun on latest `main`: 20 passed, 0 failed.

C4a Windows timing gate is now executable and the benchmark harness has been corrected to measure the actual production native-open path rather than the old C3 Dart-String -> `RopeBridge.create` path. Two environment blockers were identified: `C:\Program Files\coreutils\bin\link.exe` shadowed MSVC `link.exe`, and the MCP shell was missing standard Windows variables including `CommonProgramFiles` / `CommonProgramFiles(x86)`, which caused MSBuild `FileTracker` path-normalization failures. After restoring those variables and removing Coreutils from the benchmark PATH, the Windows runner builds normally. A separate FRB hash mismatch was caused by the generated loader reading stale `rust/target/release/alera_native.dll`; rebuilding that exact release DLL synchronized the runtime with the generated Dart/Rust hash.

Latest 50k-line / five-sample Windows evidence (2026-09-18, same run): `codeforge_native_workspace_open` median **115.35 ms** versus legacy full Dart String -> Rope construction median **468.11 ms**; first CodeForge frame after native open median **59.11 ms**; same-controller rebuild median **9.08 ms**. End-to-end native open -> first frame currently has a noisy **1108.29 ms** median because retained syntax work overlaps the frame sequence. The explicit retained native Tree-sitter first viewport probe is now the largest measured structural cost at **5542.45 ms** median on the 50k-line fixture; treat that as a C5 scheduling/latency concern and do not synchronously wait for it on cursor/paint paths. The benchmark passes when configured like production (`WorkspaceEditorSurface` does not pass `CodeForge.filePath`, avoiding the legacy synchronous file reload setter). RSS and direct FFI-payload instrumentation remain unmeasured; ownership evidence confirms the production open boundary returns bounded `WorkspaceSourceInfo` metadata rather than the full document String.

## 5. T4 - Terminal search source cutover to the worker replica model

**Priority: P1. Small, well-bounded next ownership cleanup.**

The parser/model worker is already authoritative. `TerminalXtermBufferModel` already implements `TerminalSearchSource` and maintains stable search line identities, but runtime search still normally constructs/reattaches through `XtermTerminalSearchSource(terminal)`.

Current shape:

```text
worker authoritative buffer
  -> TerminalXtermBufferModel replica
  -> TerminalXtermReplicaTerminal xterm facade
  -> XtermTerminalSearchSource facade
  -> TerminalSearchController
```

Target shape for parser-worker sessions:

```text
worker authoritative buffer
  -> TerminalXtermBufferModel (TerminalSearchSource)
  -> TerminalSearchController
```

Keep the generic xterm source only as a fallback for non-worker/legacy test paths.

### Required work

- attach `TerminalSearchController` directly to `replicaModel` when the active terminal is `TerminalXtermReplicaTerminal`;
- preserve source replacement on restore/rebuild;
- preserve selected match/navigation semantics;
- verify head trim / stable line identity behavior;
- verify TUI same-height viewport rewrites;
- verify hidden output + reveal;
- verify search open/close listener lifetime;
- benchmark 10k/100k logical lines, low/high match density, active output, next/previous navigation.

Do **not** add a second worker-side match index in this package. If UI-side text matching remains material after the direct-source cutover, record evidence for a later T4b rather than duplicating state prematurely.

Suggested branch/worktree:

```text
perf/terminal-search-replica-source
```

## 6. T5 - Production terminal worker/render/restore profile gate

**Priority: P1 evidence gate. Profile-only while T4 runs.**

The old terminal notes identified possible O(history) line-identity work, full viewport repaint, and restore replay cost. Since then main landed in-place buffer mutation, amortized head trims, immutable-scrollback skips, synchronized update coalescing, input-only repaint suppression, and active-row caching. Therefore the old hot-path list must not be treated as current truth without profiling.

T5 should instrument the **latest production path**, not the old Ghostty experiment.

### Scenarios

Profile at least:

- sustained compiler/log output;
- bursty agent output;
- full-screen TUI repaint;
- synchronized-update bursts;
- deep scrollback with ongoing output;
- hidden terminal output;
- reveal after large hidden backlog;
- large host snapshot restore/reattach;
- resize storm.

### Separate costs

Measure where practical:

- worker parse/model time;
- worker delta construction/serialization;
- isolate message bytes/count;
- UI replica apply time;
- changed-cell/changed-row count;
- full-repaint frequency;
- renderer build/paint/raster time;
- frame jank;
- RSS/heap;
- restore catch-up wall time.

Use at least five comparable samples for any implementation decision.

### T5 decision tree

```text
UI replica apply dominates
  -> T6a optimize replica/delta apply

renderer paint dominates
  -> T6b investigate true dirty-row renderer seam

full repaint protocol dominates
  -> T6c refine structural delta/full-repaint triggers

restore replay dominates
  -> T6d design direct worker snapshot hydration / bounded restore path

none are material
  -> stop terminal micro-optimization
```

T5 must not edit production terminal implementation while T4 is active. It may add benchmark/test instrumentation and documentation. Implementation begins only as a separately assigned T6 after the evidence report.

Suggested branch/worktree:

```text
perf/terminal-production-profile
```

## 7. A3 - Agent overlay repeated-unchanged fast path

**Priority: P1. Directly justified by A2 measurements.**

Current native `prepare_agent_runtime_overlay` removes the existing overlay before reconciliation. A2 proved that repeated unchanged launch is not a no-op and can be slower than first launch.

For 2,000 linked files on the Windows production bridge:

```text
first:              478.278 ms median
repeated unchanged: 636.891 ms median
```

The copy-fallback case is much more expensive.

### Target

Add a safe native reuse/reconciliation shortcut so an unchanged request/source does not recursively tear down and recreate the entire overlay.

The fast-path identity must include all semantics that can affect output, not just source path. At minimum consider:

- source tree/resource fingerprint;
- managed files/content;
- excluded managed subdirectories;
- wrapper/shell/generated content;
- overlay target/session identity;
- link vs copy-fallback state;
- source disappearance/addition/removal;
- existing target validity;
- copied-resource marker validity;
- version/schema marker so future behavior changes invalidate safely.

Reuse generic fingerprint helpers where contracts genuinely match existing Claude/Codex resource synchronization; do not duplicate an incompatible fingerprint format merely for speed.

### Required tests

- exact repeated unchanged request -> reuse fast path;
- source file content changes;
- source add/remove/rename;
- managed file changes;
- wrapper/generated content changes;
- overlay target manually removed/corrupted;
- link path and forced copy-fallback path;
- stale/old marker schema;
- Windows paths;
- containment/error semantics unchanged.

Re-run the A2 small/medium/large five-sample matrix and production Windows benchmark. The result must materially reduce repeated-unchanged latency without weakening cleanup/correctness.

Prefer keeping the existing public FRB API shape. If no API shape changes, A3 should not regenerate root bindings unnecessarily.

Suggested branch/worktree:

```text
perf/agent-overlay-unchanged-fastpath
```

## 8. S1 - Process-cold Rust initialization profile

**Priority: P2 evidence gate. Profile-only.**

C3 measured one-time process-cold initialization of approximately:

```text
Alera Rust library:     590.13 ms
CodeForge Rust library: 228.57 ms
```

These costs are not recurring editor reopen costs, but they are large enough to justify a separate startup investigation.

S1 should answer before any production change:

- are these initializations on the user-visible startup critical path?;
- how much is dynamic/native asset load vs FRB initialization vs first native call?;
- can Alera and CodeForge initialization overlap safely?;
- is lazy initialization already hiding part of the cost?;
- would post-first-frame prewarm improve perceived latency without increasing contention/RSS unacceptably?;
- does Windows differ materially from Linux/macOS packaging behavior?

Use fresh-process measurements; do not mix warm in-process calls with cold startup samples.

Deliver a profile report and a decision. Do not add startup prewarm or concurrency behavior until the profile demonstrates it improves a real user-visible path.

Suggested branch/worktree:

```text
perf/rust-cold-init-profile
```

## 9. Deferred follow-ups

### C5 - retained Tree-sitter features

After C4 stabilizes editor ownership/open semantics, the retained native tree may be extended one feature at a time for:

- folding ranges;
- bracket matching;
- structural selection;
- symbol outline/navigation.

Do not start C5 in parallel with C4. They share CodeForge controller/native-document/FRB ownership.

#### C5 folding-ranges status - 2026-09-17

The first C5 feature is complete and merged to `main` (`3607d825` implementation, `6e85b235` merge):

- folding now prefers ranges derived from the already-retained `NativeEditorDocument` Tree-sitter tree instead of rescanning every Rope character;
- the native query is revision-aware and rejects stale results before Dart can apply them;
- LSP folding remains higher priority, and unsupported/unavailable native parsers retain the existing `foldsComputeAll` Rope-scan fallback;
- the structural traversal excludes string/comment contents by operating on selected Tree-sitter container nodes, preventing delimiter characters inside strings from creating bogus folds;
- folding results are discarded if the document version changes while the async query/fallback is in flight.

Validation:

- CodeForge Rust: 16 passed, 0 failed, 1 manual benchmark ignored;
- retained folding regressions cover string-contained braces, stale revisions, incremental edits, and plaintext fallback;
- targeted CodeForge Dart analyze: no issues;
- root `workspace_editor_surface_test.dart`: 20 passed, 0 failed;
- `git diff --check`: clean.

Continue C5 one feature at a time. Next candidates are bracket matching, structural selection, then symbol outline/navigation; do not combine them into one branch.

#### C5 bracket-matching status - 2026-09-17

The second C5 feature is complete and merged to `main` (`517e346e` implementation, `64208f6a` merge):

- bracket matching now uses a synchronous retained Tree-sitter query when the native document revision is current and has no pending edits;
- structural delimiter matching walks the parsed token/parent relationship, so braces/brackets/parentheses inside strings are authoritative no-match results instead of being paired by raw-text scanning;
- if native parsing is unsupported, still opening, syncing, stale, or otherwise unavailable, the existing `foldsFindMatchingBracket` Rope scan remains the compatibility fallback;
- the paint/hit-test path remains synchronous and does not add an async wait on cursor movement.

Validation:

- CodeForge Rust: 19 passed, 0 failed, 1 manual benchmark ignored;
- bracket regressions cover structural braces/arrays, string-contained braces, stale revision, and plaintext fallback;
- targeted CodeForge Dart analyze: no issues;
- root `workspace_editor_surface_test.dart`: 20 passed, 0 failed;
- `git diff --check`: clean.

#### C5 structural-selection status - 2026-09-18

The third C5 feature is complete and merged to `main` (`1890395b` implementation, `962b06e6` merge):

- expand-selection queries the already-retained `NativeEditorDocument` Tree-sitter tree synchronously; it does not materialize the full document or trigger a fresh parse;
- the native result is revision-aware and uses Unicode scalar offsets, with Rope scalar<->byte conversion only at the Tree-sitter boundary;
- the Dart controller rejects native structural queries while the parser is unsupported/unavailable, the native document is still opening, edits are pending, or revisions are stale;
- expand/shrink maintains a controller-side selection history and clears that history on ordinary edits/manual selection changes;
- default shortcuts are `Shift+Alt+ArrowRight` to expand and `Shift+Alt+ArrowLeft` to shrink;
- when the retained native tree is not ready the shortcut is a no-op rather than synchronously waiting for the measured first-parse cost. The C4 50k-line benchmark measured the first retained Tree-sitter parse/query at about 5.54 s median, so that scheduling/latency remains a separate concern for future work.

Validation:

- CodeForge Rust: 22 passed, 0 failed, 1 manual benchmark ignored;
- targeted CodeForge Dart analyze: no issues;
- root `workspace_editor_surface_test.dart`: passed;
- root `editor_native_document_integration_boundary_test.dart`: 3 passed, 0 failed after refreshing its C4 native-open assertions to `Rope.openWorkspaceFile` + `NativeEditorDocument.openFromRope` and the revisioned edit-stream contract;
- `git diff --check`: clean.

Remaining C5 item: symbol outline/navigation.

### T6 - evidence-selected terminal implementation

T6 exists only after T5 identifies a material current bottleneck. Do not start a generic "optimize terminal more" branch without that gate.

### Native regex / Git F2 / Workspace Search G2

Still deferred. Existing evidence does not justify new native APIs:

- regex execution was not the dominant editor regex cost;
- Git history projection is already around low-millisecond even for synthetic 5k merge-heavy history;
- workspace search UI projection is already sub-millisecond under the current native 2,000-result cap.

### Ghostty alternative backend

Deferred/quarantined. Current xterm worker architecture is production-active, and the preserved Ghostty experiment has known resize/reflow parity differences. Do not spend a parallel worker on it unless the product explicitly reopens the backend choice.

## 10. Revised parallel staffing

### Five workers available now

```text
Person 1 -> C4  Editor native-open handoff / whole-text round-trip removal
Person 2 -> T4  Terminal search direct replica-source cutover
Person 3 -> T5  Latest production terminal profile gate (profile/test only)
Person 4 -> A3  Agent overlay repeated-unchanged fast path
Person 5 -> S1  Process-cold Rust initialization profile (profile only)
```

All five may start from the latest `main` in separate worktrees.

### Six workers available

Do not create a second C4 or terminal implementation owner. Use the sixth worker for one of:

```text
Person 6 -> C4 benchmark/compatibility fixture subtrack only
```

or

```text
Person 6 -> T5 real-session/TUI corpus + benchmark fixtures only
```

The sixth worker should return fixtures/evidence to the architecture owner rather than independently modifying the same production files.

## 11. Conflict matrix

Legend: LOW = safe; MEDIUM = coordinate ownership/generated integration; HIGH = serialize.

| Pair | Risk | Rule |
| --- | --- | --- |
| C4 vs T4 | LOW | editor vs terminal |
| C4 vs T5 | LOW | editor vs terminal profile |
| C4 vs A3 | LOW/MEDIUM | handwritten areas differ; coordinate only if C4 changes root FRB and A3 unexpectedly changes API shape |
| C4 vs S1 | LOW | S1 profile-only |
| C4 vs C5 | HIGH | same CodeForge/native document/controller/FRB lane |
| T4 vs T5 | MEDIUM | may run together only because T5 is profile/test-only; T5 production fixes wait |
| T4 vs future T6 | HIGH | same terminal ownership lane; merge T4 first or coordinate explicitly |
| T4 vs A3/S1 | LOW | separate subsystems |
| A3 vs S1 | LOW | agent runtime vs startup profile |

## 12. Generated/native surface rules

C4 may touch both root Alera file APIs and CodeForge native APIs depending on the selected ownership design. Treat those as two separate generated lanes.

If a generated conflict occurs:

```text
merge handwritten source first
-> regenerate from the corresponding source-of-truth config
-> review generated diff
-> run focused tests/analyzer
-> git diff --check
```

Never hand-merge FRB function IDs/content hashes.

A3 should prefer an internal native fast path that keeps the existing public API stable; do not create generated churn solely for telemetry that can be tested/benchmarked internally.

## 13. Worktree rules

Each new package must:

1. create a dedicated worktree/branch from the current agreed `main`;
2. record its base commit;
3. not use the stale/superseded performance worktrees as implementation bases;
4. leave the dirty Ghostty experiment untouched;
5. preserve unrelated changes;
6. stage/commit exact paths rather than `git add -A`;
7. use TDD/focused regressions for behavior changes;
8. benchmark before/after for performance claims;
9. re-check `main` immediately before integration;
10. avoid push unless explicitly requested.

Suggested branches:

```text
perf/editor-native-open-handoff
perf/terminal-search-replica-source
perf/terminal-production-profile
perf/agent-overlay-unchanged-fastpath
perf/rust-cold-init-profile
```

## 14. Recommended merge order

The architecture lanes are mostly independent, so completion order does not need to match staffing order.

Benchmark/profile-only results may merge whenever validated:

```text
T5
S1
```

Independent implementation lanes:

```text
T4
A3
C4
```

Before merging C4, re-run editor load/save/reload/encoding/external-file tests and the latest-main open benchmark. Before merging T4, run terminal search + restore/rebuild suites. Before merging A3, run both link-success and copy-fallback matrices.

Afterward:

```text
C4 complete
  -> C5 retained-tree features one at a time

T5 evidence + T4 complete
  -> optional T6 implementation selected by the measured bottleneck

S1 evidence
  -> optional startup prewarm/parallel-init implementation only if justified
```

## 15. Definition of done

A performance package is complete only when applicable items below are satisfied:

- behavior parity/regression coverage;
- stale/generation/revision/lifetime safety;
- explicit fallback/error semantics;
- reproducible five-sample evidence for performance decisions;
- UI-isolate/worker/RSS/FFI evidence appropriate to the package;
- no accidental whole-document/history reconstruction in a hot path;
- generated bindings regenerated only from source of truth;
- focused tests/analyzer/lints;
- `git diff --check`;
- exact-path commit(s);
- handoff documenting base, commits, benchmarks, limitations, conflicts, and merge order.

## 16. Top-line next phase

```text
C4 Editor native-open / remove whole-text ownership bounce
 ||
T4 Terminal search -> replica source
 ||
T5 Terminal latest-production profile gate
 ||
A3 Agent overlay unchanged fast path
 ||
S1 Rust cold-init profile

C4 done -> C5 retained-tree features
T5 + T4 done -> evidence-selected T6 only if needed
S1 done -> startup implementation only if user-visible evidence justifies it
```

The highest-value measured target is C4. The cleanest independent wins are T4 and A3. T5 and S1 exist specifically to prevent the next round from optimizing stale assumptions.
