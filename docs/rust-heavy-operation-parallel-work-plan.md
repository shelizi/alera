# Alera Rust Heavy-Operation Parallel Work Plan

Status: active coordination plan
Date: 2026-09-18
Baseline reviewed: `main` @ `fe947080ec04c5edcb20139ede808a9caccc6540`

This document is the current source of truth for the next heavy-operation / Rust performance phase. The 2026-09-17 plan is now largely complete: A3, T4, T5, S1, C4, the evidence-selected T6c cuts, and the full C5 retained-tree feature series have landed. The next phase therefore shifts from migration/cutover work to the remaining measured latency and validation gaps.

The rule for this phase is: **profile the latest architecture, close the remaining evidence gaps, and only then add another production optimization.**

## 1. Repository snapshot used for this re-sync

At re-audit time:

```text
branch: main
HEAD:   fe947080ec04c5edcb20139ede808a9caccc6540
upstream: origin/main
ahead: 758
behind: 116
```

Tracked `main` files are clean. Existing workspace artifacts remain untracked:

```text
?? .worktrees/
?? docs/history-session/
```

The upstream divergence is not part of this performance phase. Do not rebase/pull/push implicitly.

The preserved worktree `alera/.worktrees/terminal-parser-worker-phase1` is already merged for the production xterm worker work but is still dirty with the older Ghostty alternative-backend experiment. Leave that dirt untouched unless the backend choice is explicitly reopened.

## 2. Completion ledger since the previous allocation

### 2.1 A3 - repeated-unchanged agent overlay reuse: COMPLETE

Implementation:

```text
24353b5b perf(agent): reuse unchanged runtime overlays
```

A3 adds a private versioned reuse manifest and validates both request/source identity and the materialized overlay before taking a zero-mutation reuse path.

Five-sample Windows native benchmark medians for repeated unchanged preparation:

| Files | Mode | Before | After | Change |
| ---: | --- | ---: | ---: | ---: |
| 20 | link | 4.148 ms | 9.509 ms | integrity validation overhead |
| 20 | forced copy | 22.153 ms | 11.226 ms | 1.97x faster |
| 500 | link | 82.692 ms | 42.564 ms | 1.94x faster |
| 500 | forced copy | 4815.203 ms | 62.601 ms | 76.9x faster |
| 2000 | link | 1936.822 ms | 181.511 ms | 10.7x faster |
| 2000 | forced copy | 13852.686 ms | 278.280 ms | 49.8x faster |

The A3V production FRB closure is complete on Windows. The native integration test passed all three production-bridge cases (reconcile, warning projection, error projection), and the enabled 20 / 500 / 2000-file five-sample benchmark passed with repeated unchanged calls reporting zero removed/written/linked/copied mutations. Production-FRB repeated-unchanged medians were 8.532 ms / 36.265 ms / 550.038 ms respectively. No A4 follow-up is justified by this validation.

Source: `docs/a3-agent-overlay-unchanged-fastpath-handoff.md`.

### 2.2 T4 - terminal search direct replica source: COMPLETE

Implementation:

```text
f2c78cfc perf(terminal): search worker replica model directly
6c4c7182 Merge branch 'perf/terminal-search-replica-source'
```

Parser-worker sessions now search `TerminalXtermBufferModel` directly instead of routing through the xterm facade.

Five-sample benchmark highlights:

- 100k low-density scan: 22.87 ms direct vs 65.63 ms legacy (~2.87x lower).
- 100k high-density scan: 75.24 ms direct vs 94.22 ms legacy (~1.25x lower).
- Navigation remains sub-millisecond after indexing.
- No second worker-side search index was added.

Source: `docs/t4-terminal-search-replica-source-handoff.md`.

### 2.3 T5 - production terminal profile gate: COMPLETE

Implementation/report:

```text
b6a61fa1 perf(terminal): profile production worker render paths
678afeb8 Merge branch 'perf/terminal-production-profile'
```

The latest-production profile rejected the old generic O(history) concern for normal output. It instead identified two structural hotspots:

1. hidden-backlog reveal -> one whole-buffer full-state transfer;
2. resize storms -> one full repaint per resize callback.

It also found a renderer/raster tail under `flutter_tester`, but native Windows render/restore evidence was missing at that time.

Source: `docs/t5-terminal-production-profile-report.md`.

### 2.4 T6c - structural terminal full-state traffic reduction: TWO EVIDENCE-BACKED CUTS COMPLETE

First cut:

```text
343b2c0b perf(terminal): coalesce parser worker resize storms
```

40 resize callbacks now collapse to one final parser-worker resize at the existing ordering/debounce boundary.

Paired result:

| Metric | Before | After |
| --- | ---: | ---: |
| full repaints | 40 | 1 |
| wall median | 1652.51 ms | 35.05 ms |
| worker roundtrip median | 1329.27 ms | 31.54 ms |
| replica apply median | 126.97 ms | 0.97 ms |
| logical delta proxy | 141.5 MB | 3.44 MB |

Second cut:

```text
24f778b9 perf(terminal): pack full buffer snapshot transfer
5953e71f Merge branch 'perf/terminal-reveal-backlog-staging'
```

Full snapshots now use packed `Uint32List` + `TransferableTypedData` transport rather than nested per-cell object lists.

Paired hidden-reveal result:

| Stage | Nested | Packed |
| --- | ---: | ---: |
| raw worker roundtrip median | 3558.70 ms | 257.78 ms |
| worker materialize median | 1154.30 ms | 245.39 ms |
| isolate transfer/scheduling median | 2404.40 ms | 2.69 ms |
| UI decode median | 863.35 ms | 299.95 ms |
| replica apply median | 289.72 ms | 229.43 ms |
| reveal end-to-end median | 4711.81 ms | 864.19 ms |

The production-profile reveal median after the packed cut is about 651.99 ms. Further terminal work must return to profiling before changing protocol again.

Source: `docs/t6-terminal-structural-delta-handoff.md`.

### 2.5 S1 - process-cold Rust initialization profile: COMPLETE

Implementation/report:

```text
fffa63fd perf: profile process-cold Rust initialization
8696d64e Merge branch 'perf/rust-cold-init-profile'
```

Important decision:

- sequential dev-DLL `bothReady` median: 22.855 ms;
- concurrent FRB-init median: 22.767 ms;
- delta: ~0.4%, within noise.

Therefore do **not** convert the two startup awaits to `Future.wait`.

Both Alera and CodeForge Rust initialization are still awaited before `runApp()`. The next justified startup experiment is CodeForge post-first-frame/lazy initialization with a readiness gate, not simple concurrent initialization.

Source: `docs/performance/rust-cold-init-s1-report.md`.

### 2.6 C4 - native editor open handoff: COMPLETE

Implementation:

```text
bdbaa84d perf(editor): hand off file open to native rope
6bccb422 perf(editor): benchmark native open handoff
```

The production editor no longer needs the old native -> full Dart String -> CodeForge native Rope ownership bounce for the initial large-file open. CodeForge opens the workspace source into a native Rope, and `NativeEditorDocument` structurally clones that Rope.

Latest 50k-line / five-sample Windows evidence:

| Stage | Median |
| --- | ---: |
| CodeForge native workspace open | 115.35 ms |
| legacy full Dart String -> Rope | 468.11 ms |
| first CodeForge frame after native open | 59.11 ms |
| same-controller rebuild | 9.08 ms |
| native-open -> first frame, current noisy end-to-end | 1108.29 ms |
| first retained Tree-sitter parse/query probe | 5542.45 ms |

The whole-text handoff problem is solved. The largest remaining editor structural cost is now retained Tree-sitter startup/first parse.

Current code confirms why: `NativeEditorDocument.open_from_rope` clones the Rope and immediately runs a full Tree-sitter parse before the retained document becomes ready.

Source: `docs/rust-heavy-operation-parallel-work-plan.md` C4 status and `third_party/code_forge/rust/src/api/editor_document.rs`.

### 2.7 C5 - retained Tree-sitter feature series: COMPLETE

Landed one feature at a time:

```text
3607d825 perf(editor): derive folds from retained syntax tree
517e346e perf(editor): match brackets from retained syntax tree
1890395b perf(editor): expand selection from retained syntax tree
1a0c5ae5 perf(editor): outline symbols from retained syntax tree
e71c1a77 fix(editor): align outline surface integration
fe947080 docs(perf): complete C5 retained symbol outline
```

Completed retained-tree consumers:

- folding ranges;
- bracket matching;
- structural expand/shrink selection;
- symbol outline/navigation.

All preserve stale/revision safety and existing fallbacks. None claims to solve the ~5.54 s large-file initial parse.

## 3. Current measured gaps

The previous five-worker allocation is complete. The remaining work is now narrower and more evidence-driven.

### Gap E1 - retained Tree-sitter cold parse dominates large-file editor startup

On the 50k-line fixture the first retained Tree-sitter parse/query probe is about 5.54 s median. The production open itself is already down to about 115 ms, so parser startup is now the obvious editor target.

Current `open_from_rope` behavior:

```text
clone native Rope
-> configure Tree-sitter parser
-> full parse_rope(...)
-> compile highlight query
-> publish retained NativeEditorDocument
```

The FRB call is asynchronous, so this does not block the Flutter UI isolate directly, but it can overlap first-frame/editor work, consume CPU, delay native syntax readiness, and waste work when a large document is closed or replaced quickly.

### Gap T1 - terminal post-T6 evidence is incomplete

T6 eliminated the two largest structural transport pathologies, but the packed reveal still shows meaningful:

- UI decode/materialization cost (~300 ms in the paired stage-local case);
- replica apply cost (~229 ms in that paired case);
- end-to-end production-profile reveal around 652 ms.

Separately, T5's renderer signal came from `flutter_tester`, not the native Windows compositor/GPU path, and the native restore benchmark was not collected because the desktop compiler environment was broken at the time.

The C4 benchmark work later identified and fixed the relevant Windows environment class (Coreutils `link.exe` shadowing MSVC plus missing standard Windows environment variables), so these native terminal measurements should be retried before another terminal production patch.

### Gap S1 - startup still awaits CodeForge before first frame

`lib/main.dart` still does:

```text
await RustLib.init()
await code_forge.RustLib.init()
...
runApp(...)
```

S1 proved concurrent init is not useful. CodeForge remains the stronger candidate for post-first-frame prewarm or lazy initialization because it is editor-specific.

### Gap V1 - A3 production FRB validation closure

A3's native benchmark and Rust correctness coverage are strong, but its actual Flutter/Windows FRB rerun was blocked by the same desktop build environment that was subsequently repaired for C4. Close this evidence gap before considering A3 fully validated end-to-end.

### Gap M1 - C4 RSS / direct payload metrics are still missing

C4 proved the ownership boundary structurally and measured latency, but direct FFI payload and whole-app/RSS evidence remain unmeasured. Collect them while profiling E1 instead of creating a separate production architecture branch.

## 4. Next-phase package summary

| ID | Priority | Work package | Start now? | Ownership |
| --- | --- | --- | --- | --- |
| C6P | P0 gate | Retained parser cold-start + memory/payload profile | **Yes** | benchmark/evidence only |
| C6 | P0 | Large-file retained parser scheduling / lazy readiness | **Yes after C6P baseline** | CodeForge/native editor |
| T7 | P1 gate | Post-T6 native Windows terminal render/restore/reveal profile | **Complete** | benchmark/evidence only |
| S2 | P1 gate | CodeForge post-first-frame/lazy-init A/B | **Yes, profile/prototype first** | startup/readiness |
| A3V | P1 validation | Windows production FRB validation for overlay reuse | **Yes** | validation only |
| T8 | P1 | T8d direct worker snapshot hydration / restore redesign | **Selected by T7; not started** | terminal implementation |
| S3 | Evidence only | Startup implementation retained by S2 evidence | **No** | startup implementation |

Four completely independent evidence lanes can start immediately: **C6P || T7 || S2 || A3V**. C6 implementation can begin its tests/design in parallel but should not lock in production scheduling policy until the C6P baseline is recorded.

## 5. C6P - retained parser cold-start profile and C4 measurement closure

**Priority: P0 evidence gate. No production behavior changes.**

Suggested branch/worktree:

```text
perf/editor-native-parse-profile
```

### Required measurements

Use the current C4 production open path and collect at least five samples for:

- 2k / 20k / 50k / 100k logical lines where practical;
- Rust, Dart, TypeScript/TSX at minimum;
- Rope clone time;
- parser creation / language setup;
- full Tree-sitter parse;
- highlight-query compilation;
- first viewport syntax query;
- native-document ready latency;
- first CodeForge frame;
- native open -> first useful frame;
- CPU time / contention signal where available;
- process RSS / heap movement;
- direct FRB payload or a stable proxy that confirms initial-open metadata remains bounded.

Also measure rapid tab replacement/close while parsing to quantify wasted cold-parse work.

### C6P output

Produce one report that decides whether the first C6 production cut should prioritize:

1. deferred parse start;
2. cancellation/supersession;
3. size-aware parse policy;
4. parser/query caching;
5. another measured source.

Do not optimize based only on the old 5.54 s aggregate.

C6P owns `integration_test/editor_open_profile_benchmark.dart` and new profile docs while active.

## 6. C6 - large-file retained parser scheduling / lazy readiness

**Priority: P0 implementation, gated by C6P baseline.**

Suggested branch/worktree:

```text
perf/editor-native-parse-scheduling
```

Current retained-tree consumers already tolerate native syntax state being unavailable:

- viewport syntax uses fallback/no-highlight policy according to large-file mode;
- folding has fallback;
- bracket matching falls back;
- structural selection is a no-op while native state is not ready;
- outline can use LSP/fallback and refresh later.

That makes scheduling the retained parse later safer than blocking user interaction on parser readiness.

### Target architecture

Separate **native document/Rope ownership** from **full Tree-sitter readiness**.

Desired state machine:

```text
Rope ready
-> native syntax document handle exists
-> parse state = pending / parsing / ready / unsupported / failed / cancelled
-> retained-tree consumers query only when revision-ready
```

For large documents, do not automatically force a multi-second full parse into the first-frame window.

### Candidate first cut

Subject to C6P evidence:

- small files: preserve eager behavior if latency is already cheap;
- large files: schedule first parse after first frame / idle window rather than immediately on controller configuration;
- cancel/supersede stale parse generations when the document closes, language changes, or a newer generation replaces it;
- queue committed edit deltas safely while initial parse is pending;
- when the parse becomes ready, apply/verify the current revision before exposing the tree;
- never make cursor movement, paint, bracket matching, structural selection, or outline synchronously wait for first parse;
- avoid starting duplicate cold parses from multiple consumers.

### Correctness gates

Cover:

- edit while initial parse is pending;
- close/dispose while parse is pending;
- language switch during parse;
- reload/replace during parse;
- stale generation completion;
- unsupported grammar;
- parse failure fallback;
- first viewport request before readiness;
- folding/bracket/selection/outline behavior before and after readiness;
- Unicode scalar revision correctness.

### Success criteria

- first-frame/open latency no longer materially overlaps the multi-second cold parse for large files;
- retained syntax still becomes available eventually;
- no UI-isolate synchronous wait;
- no whole-document Dart snapshot regression;
- no duplicate cold parse per consumer;
- rapid tab close/change does not leave useless parse work/lifetime leaks;
- C6P before/after evidence includes RSS and readiness latency.

## 7. T7 - post-T6 native Windows terminal profile gate

**Priority: P1 evidence gate. No production terminal changes in T7.**

**2026-09-18 status: COMPLETE.** Native Windows restore and flush/render gates now pass under `@home-node`. Restore median/p95 is 9.90/12.44 s with 0/5 inside the 3 s target; native raster median/p95 is 58.61/84.32 ms. T7 therefore selects **T8d: direct worker snapshot hydration / restore redesign** as the single next terminal architecture cut. See `docs/t7-terminal-post-t6-native-profile-report.md`.

Suggested branch/worktree:

```text
perf/terminal-post-t6-native-profile
```

Now that the Windows desktop environment issue is understood, rerun the native gates that T5 could not collect.

### Required runs

At minimum:

```text
flutter test integration_test/terminal_restore_benchmark.dart -d windows
flutter test integration_test/terminal_flush_cadence_benchmark.dart -d windows
```

Also rerun:

- `test/benchmarks/terminal_production_worker_profile_benchmark.dart`;
- `test/benchmarks/terminal_reveal_pipeline_profile_benchmark.dart`;
- `test/benchmarks/terminal_streaming_render_profile_benchmark.dart`.

Use five samples and capture the repaired environment recipe so future worktrees are reproducible.

### Required interpretation

T7 must answer four questions:

1. Is native restore now a material user-visible bottleneck?
2. Does the native Windows renderer confirm or reject the `flutter_tester` raster-tail signal?
3. After packed snapshot transport, is reveal dominated by UI decode/materialization, replica apply, or renderer work?
4. Is the remaining ~650 ms reveal worth another architecture cut under realistic backlog sizes?

### T8 decision tree

```text
UI packed decode/materialization dominates
  -> T8a keep packed storage longer / avoid rebuilding per-cell Dart objects

replica apply dominates
  -> T8b apply packed rows directly into BufferLine / retained replica state

native renderer dominates
  -> T8c investigate a dirty-row renderer seam

restore replay dominates
  -> T8d direct worker snapshot hydration / restore redesign

none are material in native measurements
  -> stop terminal architecture work
```

T7 evidence collection is complete.

Native restore dominates the measured terminal bottlenecks: 9.90 s median / 12.44 s p95 versus a 431.22 ms packed reveal median and a 97.23 ms native total-frame median. Per the decision tree, the next terminal implementation is **T8d: direct worker snapshot hydration / restore redesign**. Keep T8a (packed UI materialization) and T8c (renderer seam) deferred so the first T8 cut remains isolated and measurable.

## 8. S2 - CodeForge post-first-frame / lazy-init A/B gate

**Priority: P1 evidence/prototype gate.**

Suggested branch/worktree:

```text
perf/codeforge-lazy-init-profile
```

S1 already rejected simple concurrent initialization. S2 should test the next justified idea: move CodeForge initialization off the pre-`runApp` critical path without creating an unacceptable first-editor stall.

### S2 variants

Compare at least:

1. current baseline: CodeForge initialized before `runApp`;
2. post-first-frame prewarm;
3. lazy init immediately before first editor/native CodeForge use.

### Measure

Fresh process, at least five samples:

- process start -> first frame;
- process start -> app interactive;
- first editor open when CodeForge is already warm;
- first editor open when lazy init is still cold;
- CPU contention around first frame;
- whole-app RSS/working set;
- error/readiness behavior if an editor is opened immediately at startup.

### Coordination with C6

S2 owns startup/readiness behavior, not CodeForge parser internals. While C6 is active:

- S2 may add instrumentation and an isolated prototype/readiness gate;
- do not merge a production lazy-init cut that changes first-editor initialization ordering until C6's parser-readiness contract is stable;
- C6 must not assume CodeForge was initialized before `runApp`.

If S2 does not show a material first-frame win, stop and keep the current startup sequence.

## 9. A3V - production FRB validation closure

**Priority: P1 validation only.**

Suggested branch/worktree:

```text
perf/agent-overlay-frb-validation
```

Do not redesign A3.

Using the repaired Windows desktop environment:

- rerun `integration_test/agent_runtime_overlay_native_test.dart -d windows`;
- rerun `integration_test/agent_runtime_overlay_benchmark.dart -d windows --dart-define=ALERA_RUN_AGENT_OVERLAY_BENCHMARK=true`;
- verify repeated unchanged calls report zero mutation through the actual production bridge;
- record five-sample 20/500/2000 results;
- verify link-success and the public error/warning projection;
- update the A3 handoff with production bridge evidence.

The 20-file link case may remain slower than the old rebuild path because A3 intentionally performs content/integrity validation. Do not weaken correctness merely to win that micro-case.

If all FRB checks pass, close A3 completely. Only create A4 if a correctness bug or a materially important production regression is found.

## 10. Deferred / stopped work

### C7 and later retained-tree features

The originally planned retained-tree consumers are complete. Do not add more native structural features until C6 fixes or consciously accepts the large-file parser startup cost.

### Native regex

Still stopped. Existing evidence says the full-text snapshot was the dominant old regex cost, not regex execution itself.

### Git F2

Still stopped. Current projection costs are already low-millisecond even at synthetic 5k merge-heavy history.

### Workspace Search G2

Still stopped under the current 2,000-result cap.

### Ghostty terminal backend

Still quarantined. The production xterm worker architecture is active and the preserved Ghostty experiment has known resize/reflow parity differences.

## 11. Revised parallel staffing

### Five workers

```text
Person 1 -> C6P  retained parser cold-start + RSS/payload profile
Person 2 -> C6   parser scheduling/cancellation tests + implementation after C6P baseline
Person 3 -> T7   post-T6 native Windows terminal profile
Person 4 -> S2   CodeForge lazy/post-first-frame init A/B
Person 5 -> A3V  production FRB validation closure
```

Parallelism rules:

- C6P and C6 may run together only with explicit file ownership:
  - C6P owns benchmark/profile files and report;
  - C6 owns CodeForge controller/native parser implementation and focused tests.
- C6 may design tests immediately, but should not finalize the scheduling policy until the C6P baseline is recorded.
- T7 is profile-only and does not compete with C6/S2/A3V.
- S2 production merge waits for the C6 readiness contract if it changes CodeForge first-use ordering.
- A3V is validation-only and independent.

### Four workers

Prefer:

```text
C6P/C6 combined owner
T7
S2
A3V
```

### Six workers

Use the sixth worker only for reusable benchmark/corpus work, for example:

```text
Person 6 -> terminal real-session/TUI replay fixtures for T7
```

or:

```text
Person 6 -> editor language/size benchmark matrix for C6P
```

Do not create a second production C6 or T8 owner.

## 12. Conflict matrix

Legend: LOW = safe; MEDIUM = coordinate; HIGH = serialize.

| Pair | Risk | Rule |
| --- | --- | --- |
| C6P vs C6 | MEDIUM | split benchmark/docs vs production CodeForge/native files |
| C6 vs T7 | LOW | editor vs terminal |
| C6 vs A3V | LOW | CodeForge/editor vs agent overlay |
| C6 vs S2 | MEDIUM | S2 changes CodeForge readiness/first-use ordering; production merge after C6 contract |
| T7 vs S2 | LOW | terminal profile vs startup |
| T7 vs A3V | LOW | terminal vs agent overlay |
| S2 vs A3V | LOW | startup vs agent overlay |
| T7 vs future T8 | HIGH | T8 starts only after T7 decision |
| C6 vs future retained-tree feature work | HIGH | same retained native document/parser lane |

## 13. Worktree and validation rules

Each package must:

1. create a dedicated worktree/branch from the current agreed `main`;
2. record its base commit;
3. not use stale performance worktrees as implementation bases;
4. leave the dirty Ghostty experiment untouched;
5. preserve unrelated changes;
6. stage/commit exact paths, never blanket-add unrelated workspace artifacts;
7. use TDD/focused regressions for behavior changes;
8. collect before/after evidence for performance claims;
9. use five samples for benchmark decisions;
10. re-check `main` immediately before integration;
11. avoid push unless explicitly requested.

Generated FRB files must only be regenerated from the corresponding handwritten source/config. Never hand-edit generated function IDs or content hashes.

## 14. Recommended merge order

Evidence/validation reports may land first:

```text
A3V
T7
C6P
```

Then:

```text
C6
```

S2:

- report/prototype evidence may land independently;
- a production startup-order change should integrate after C6's readiness contract is stable.

Afterward:

```text
T7 selects T8 only if native evidence justifies it
S2 selects S3 only if first-frame gain is material
C6 decides whether another parser architecture phase is necessary
```

## 15. Definition of done

A next-phase package is complete only when applicable items below are satisfied:

- behavior/regression coverage;
- stale/generation/revision/lifetime safety;
- explicit fallback/error semantics;
- five-sample evidence for performance decisions;
- UI-isolate/worker/RSS/FFI evidence appropriate to the package;
- no accidental full-document/history reconstruction in a hot path;
- generated bindings updated only from source of truth;
- focused tests/analyzer/lints;
- `git diff --check`;
- exact-path commit(s);
- handoff documenting base, commits, benchmark numbers, limitations, and integration order.

## 16. Top-line next phase

```text
C6P  Editor retained-parser cold-start profile
  -> C6 large-file parser scheduling / cancellation / readiness

T7   Terminal post-T6 native Windows profile
  -> T8 only if decode/apply/render/restore evidence justifies it

S2   CodeForge post-first-frame/lazy-init A/B
  -> S3 only if startup gain is material

A3V  Close production FRB validation gap
```

The highest-value measured editor target is no longer file ownership; it is the multi-second retained Tree-sitter cold parse. The highest-value terminal action is now validation/profiling rather than another blind protocol rewrite.
