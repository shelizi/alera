# Alera Rust Heavy-Operation Parallel Work Plan

Status: active coordination plan
Date: 2026-09-17
Baseline reviewed: `main` @ `a1f637143c28c701ea750e23bbd9923ca29142ca`

This document is the current coordination plan for remaining heavy-operation / Rust performance work in Alera. It is intentionally more execution-oriented than `docs/rust-performance-optimization-roadmap.md`: it records what is still worth doing on the current `main`, what has already been solved, which work packages can run in parallel, which files are likely to conflict, and how to integrate multiple workers safely.

If older handoff/roadmap text conflicts with this file on **current priority or current implementation state**, re-check `main` and prefer this plan's 2026-09-17 inventory.

## 1. Coordination goals

The goal is not to move every Dart loop into Rust. Prioritize work that does at least one of the following:

- removes synchronous recursive filesystem work from the Flutter UI isolate;
- keeps large mutable state/index/parser data native instead of repeatedly copying whole snapshots over FFI;
- removes whole-document / whole-history rebuilding from hot paths;
- replaces large Dart allocation graphs with bounded native projections;
- makes work scale with the changed/visible region rather than total file/history/search size;
- has measurable impact under `docs/performance.md` profiling rules.

For parallel development, a second goal is equally important: **avoid having several workers modify the same generated FRB surface or the same editor hot-path files at the same time unless the merge strategy is explicitly planned.**

## 2. Repository snapshot used for this inventory

At inventory time:

```text
branch: main
HEAD:   a1f637143c28c701ea750e23bbd9923ca29142ca
upstream: origin/main
ahead: 648
behind: 84
```

Tracked working tree was clean. Only the existing workspace artifacts were untracked:

```text
?? .worktrees/
?? docs/history-session/
```

Do not add/delete those paths as part of the work packages below.

The older Rust performance handoff contains historical HEAD/worktree state and must not be used as a current Git-state source without re-checking the repository.

## 3. Findings that are already solved and should not be re-opened

The following items appeared in older reviews/roadmaps but are no longer current P0 work.

### 3.1 Deferred admission / QoS is already bounded

Current Terminal Host admission has:

- active concurrency limit;
- total pending+active capacity;
- dispatch-critical reserve;
- explicit `DispatchCritical`, `Maintenance`, and `Bulk` classes;
- typed backpressure rejection;
- queue/admission metrics.

Relevant code:

```text
rust/alera-cli/src/terminal_host/server/deferred_admission.rs
```

Do not restart the old "unbounded tasks waiting on a shared semaphore" project unless new profiling or correctness evidence shows a new failure mode.

### 3.2 Coordinator Git drift probe is already off the actor

Current flow schedules the blocking Git probe through deferred admission and re-enters the actor using `ServerCommand::CoordinatorDriftProbed`.

Relevant code:

```text
rust/alera-cli/src/terminal_host/server/coordinator_dispatch.rs
```

This is no longer a current actor-blocking migration target.

### 3.3 Automation autostart reconcile is already background/deferred

Settings persistence and the reconcile completion path are separated; identified requests finish through `AutostartReconcileFinished`.

Relevant code:

```text
rust/alera-cli/src/terminal_host/server/host_service_requests.rs
rust/alera-cli/src/terminal_host/server/deferred_requests.rs
```

Do not duplicate this migration.

### 3.4 Managed-workspace external preflight is no longer performed entirely in the actor

The mutation queue performs external preflight before asking the actor for session/process ownership cleanup. The actor-side `prepare_managed_workspace_removal()` is now primarily actor-owned session/shutdown state.

Relevant code:

```text
rust/alera-cli/src/terminal_host/server/runtime_mutation_queue.rs
rust/alera-cli/src/terminal_host/server/runtime_mutations.rs
rust/alera-cli/src/terminal_host/server/session_termination.rs
```

This should be revisited only for a demonstrated remaining stall/correctness problem.

### 3.5 Editor search/decoration/diagnostic viewport scans from the old handoff are already substantially improved

Current `main` includes the later editor work such as:

```text
405b7394  bound search highlight painting
b161b1b1  index visible line decorations
faf4036f  index visible diagnostics
442758aa  search literals in native rope
7b34f56d  run literal search off UI thread
2c7b95b7  run regex search off UI thread
c04adcb9  run replace all off UI thread
```

and the later long-line / viewport shaping series through current `main`.

Do not assign a worker to the old "scan every search highlight/decoration/diagnostic before viewport reject" task without first proving the current implementation regressed or still has a measurable hot path.

## 4. Current work-package summary

| ID | Priority | Work package | Start now? | Parallel class | Native surface |
| --- | --- | --- | --- | --- | --- |
| A | P0 | Agent runtime overlay native reconciliation | Yes | Independent | Root FRB |
| B | P0 gate | Finish current large-file editor profiling / measurement | Yes | Independent | None unless evidence requires changes |
| C | P1 | `NativeEditorDocument` retained incremental parser / viewport spans | After B gate | Independent from A; conflicts with E implementation | CodeForge FRB |
| D | P1 profile | Terminal search benchmark + current Dart rebuild optimization | Yes | Independent | None initially |
| E | P2 | Editor regex full-snapshot benchmark / compatibility decision | Completed 2026-09-17 | Completed without native FRB changes | None |
| F | P2 | Git history graph projection algorithm/cache | Yes, Dart-first | Independent initially | Root FRB only if later native |
| G | P2 | Workspace search result projection/paging benchmark | Yes, benchmark/Dart-first | Independent initially | Root FRB only if later native |
| H | Deferred | Rust terminal core shadow PoC | No, unless explicitly reopened | Separate architecture track | Terminal Host/wire; not current work |
| I | P3/non-priority | Small sync Dart filesystem calls / ICO decode | No | N/A | Not justified now |

## 5. Work package A - Agent runtime overlay native reconciliation

**Priority: P0. Recommended to assign immediately.**

### Why it is still a real heavy path

Claude/Codex recursive runtime resource sync was already native-optimized, but the terminal-launch overlay path used by OpenCode/Pi/Copilot/Amp still performs synchronous filesystem work in Dart.

Current terminal launch preparation can execute:

```text
clear old overlay
-> recursively traverse source
-> create directories
-> create resource links
-> fallback recursive copy on link failure
-> write copied-resource markers
-> write managed files atomically-ish
-> return environment
```

Relevant production path:

```text
lib/src/features/agent_status/application/agent_status_providers.dart
lib/src/features/agent_status/infra/agent_runtime_overlay_service.dart
lib/src/features/agent_status/infra/agent_runtime_overlay_prepare.dart
lib/src/features/agent_status/infra/agent_runtime_overlay_sources.dart
lib/src/features/agent_status/infra/agent_runtime_overlay_wrappers.dart
lib/src/features/agent_status/infra/agent_runtime_overlay_shell.dart
```

The critical recursive/synchronous implementation is in `agent_runtime_overlay_sources.dart`, including `listSync`, recursive copy/delete, link fallback, marker writes, and type checks.

### Proposed ownership

Add one native operation instead of many tiny FFI calls, conceptually:

```text
prepare_agent_runtime_overlay(request)
  -> overlay_path
  -> environment additions
  -> linked/copied/written/removed counts
  -> warnings
```

Rust should own the complete filesystem transaction-like operation:

- safe bounded cleanup under the expected overlay root;
- directory traversal;
- symlink/junction/link creation where current semantics allow it;
- recursive copy fallback;
- managed-file writes;
- copied-resource marker writes;
- stale overlay cleanup;
- path containment/safety checks;
- deterministic error/warning projection.

### Reuse instead of duplicate

Before adding a second recursive filesystem implementation, reuse/refactor the existing native resource helpers where practical:

```text
rust/src/api/agent_runtime_resources.rs
lib/src/rust/api/agent_runtime_resources.dart
```

Do not force incompatible Claude/Codex fingerprint semantics into the overlay API; share only generic filesystem primitives/guard rules that genuinely have the same contract.

### Likely touched files

Hand-written source:

```text
rust/src/api/agent_runtime_resources.rs
rust/src/api/mod.rs
possibly rust/src/api/agent_runtime_overlay.rs
lib/src/features/agent_status/infra/agent_runtime_overlay_*.dart
lib/src/features/agent_status/application/agent_status_providers.dart
focused Rust + Dart tests
```

Generated root FRB files will also change if a new API surface is added.

### Test/benchmark gate

Cover at minimum:

1. no source directory;
2. explicit source path missing;
3. link success;
4. link failure -> recursive copy fallback;
5. nested directories/files;
6. symlink handling;
7. managed subdirectory excludes managed files from mirror;
8. stale overlay removal;
9. target containment guard;
10. repeated no-op/replace preparation;
11. Windows path behavior;
12. behavior parity for OpenCode/Pi/Copilot/Amp launch environment.

Measure terminal-launch preparation for small and large source trees and verify the Flutter UI isolate no longer performs recursive filesystem work.

## 6. Work package B - Current large-file editor profiling / finish the existing hot-path pass

**Priority: P0 gate. Recommended to assign immediately.**

This worker should not start a broad parser rewrite. The job is to establish the post-`a1f63714` baseline after the recent viewport/long-line/editor search work.

Primary areas:

```text
third_party/code_forge/lib/code_forge/code_area.dart
third_party/code_forge/lib/code_forge/find_controller.dart
third_party/code_forge/lib/code_forge/rope.dart
lib/src/features/workbench/... editor surface/profile files
integration/widget performance harnesses
```

Profile at least:

- fast vertical scrolling;
- fast horizontal scrolling on very long lines;
- caret movement;
- word movement;
- selection dragging;
- typing bursts;
- literal search with many matches;
- regex search with many matches;
- diagnostics-heavy large files;
- large file open/reopen.

Record UI CPU, raster time, allocations/GC, frame jank, paragraph/shaping construction, FFI payloads where relevant, and at least five comparable samples for decisions.

### Output of B

B should end with one of two explicit conclusions:

```text
B1: current Dart/CodeForge hot path is now bounded enough; proceed to NativeEditorDocument parser spike
```

or

```text
B2: a still-reachable current hot path dominates; fix/measure that before opening parser migration
```

B is the evidence gate for C.

**2026-09-17 completion:** B concluded **B1** on the current main baseline. The real-device five-sample matrix found no sustained 60 Hz dominant hot path in the required scenarios. See `docs/editor-large-file-profiling-b.md` for the frame, raster, allocation/GC, paragraph/shaping, and FFI evidence. Work package C may proceed. The subsequently merged E result keeps Dart `RegExp` compatibility while reusing one full-text snapshot per document revision, so B does not duplicate that work.

## 7. Work package C - NativeEditorDocument retained incremental parser

**Priority: P1. Start implementation after B confirms the gate.**

This is the next major editor architecture project, not merely "move syntax highlighting to Rust".

### Target native state

Phase 1 should keep scope small:

```text
NativeEditorDocument
  - document id
  - document revision
  - retained Rope/text snapshot owner or coordinated rope handle
  - line/offset index
  - incremental parser/tree
  - syntax-span query state
```

Initial API shape:

```text
open_editor_document(document_id, text/version)
apply_editor_edits(document_id, version, edits)
query_syntax_spans(document_id, version, start_line, end_line, overscan)
close_editor_document(document_id)
```

Response must carry enough revision information for Dart to reject stale results.

### Do not put everything in Phase 1

Do not initially combine:

- symbol outline;
- folding;
- bracket matching;
- structural selection;
- search index;
- diagnostics index.

Prove incremental edit + viewport syntax first, then reuse the retained parse tree in later phases.

### Likely touched area

Prefer keeping this inside the vendored CodeForge native boundary where current editor rope/native APIs already live:

```text
third_party/code_forge/rust/src/api/*
third_party/code_forge/lib/code_forge/*
third_party/code_forge/lib/src/rust/*
Alera editor surface integration/tests as needed
```

This uses the **CodeForge FRB surface**, which is separate from A's root Alera FRB surface. That separation is important for parallel development.

### Success criteria

- edit cost scales primarily with changed region, not whole file size;
- viewport syntax query is bounded by visible range + fixed overscan;
- no full-document syntax span transfer on scroll/edit;
- stale version responses cannot overwrite newer UI state;
- retained native state is released on document close;
- large files can eventually retain syntax highlighting without restoring old whole-document work.

**2026-09-17 Phase 1 completion:** C now has a retained Tree-sitter `NativeEditorDocument` on the CodeForge FRB surface with monotonic revisions, incremental edit deltas, viewport + overscan syntax queries, stale-result rejection, explicit close, unsupported-language fallback, generated Dart bindings, focused correctness tests, and a five-sample 2k/20k/100k-line benchmark. Viewport query cost stayed in roughly the same 10-13 ms band across those sizes. Incremental parse was materially cheaper than a fresh full parse but still showed large-tree traversal cost, so production renderer activation is intentionally deferred until the controller exposes a reliable committed scalar edit stream without reintroducing full snapshots in large-file mode. See `docs/editor-native-document-c.md`.

## 8. Work package D - Terminal search benchmark and bounded Dart optimization

**Priority: P1 profile track. Recommended to assign immediately.**

Current terminal search is not a naive full rescan on every append; it already incrementally rescans the relevant tail/viewport in important paths. The remaining candidate is match reconstruction/sorting/allocation when histories and match counts become large.

Relevant code:

```text
lib/src/features/workbench/presentation/terminal_search_controller.dart
lib/src/features/workbench/domain/terminal_search.dart
```

### First assignment

Build a reproducible benchmark for:

- 10k and 100k logical lines;
- low/high match density;
- repeated output appended while search is active;
- TUI viewport rewrites;
- next/previous match navigation;
- case-sensitive/insensitive literal search.

If `_rebuildMatches()` dominates, optimize the current Dart/xterm model first: maintain ordered per-line ranges, avoid rebuilding/sorting unchanged global match state, or update only affected line ranges.

### Architecture guard

Do **not** create a second Rust scrollback copy only for search while xterm2 remains the authoritative terminal state. Native terminal search should be implemented together with future Rust terminal-state ownership, not as duplicated state.

This makes D safe to run in parallel with A/B/F/G.

## 9. Work package E - Editor regex full-snapshot benchmark and compatibility decision

**Priority: P2. Benchmark/research can start immediately. Native implementation must coordinate with C.**

Current regex search no longer blocks the UI isolate, but it still does roughly:

```text
Rust Rope
-> full String snapshot
-> FFI transfer
-> Dart isolate transfer
-> Dart RegExp scan
-> match ranges back
```

Relevant code:

```text
third_party/code_forge/lib/code_forge/find_controller.dart
third_party/code_forge/lib/code_forge/rope.dart
third_party/code_forge/rust/src/api/rope.rs
```

### Phase E1 - benchmark only

Measure 512 KiB / multi-MiB files, high/low match density, repeated query edits, lookaround patterns, Unicode/scalar offset behavior, and replace-all.

### Phase E2 - compatibility decision

Do not replace Dart `RegExp` with Rust `regex` blindly. Current semantics include constructs such as lookahead, which Rust `regex` does not support.

Evaluate one of:

- keep current Dart isolate path if snapshot/copy cost is acceptable;
- native engine with a compatibility-capable implementation;
- native fast path for a supported subset plus explicit Dart fallback;
- another engine only if packaging/security/maintenance costs are justified.

### E1/E2 outcome - 2026-09-17

E1 completed with a Windows desktop integration benchmark (`Flutter 3.47.2`, `Dart 3.13.2`) using five measured samples per case. The harness covers 512 KiB and 4 MiB ASCII inputs, low/high match density, 4 MiB Unicode/scalar offsets, lookahead compatibility, replace-all, and repeated query edits.

Final representative medians:

| Case | Full Rope snapshot | Dart regex/isolate | End to end |
| --- | ---: | ---: | ---: |
| 4 MiB ASCII, low density | 98.15 ms | 9.58 ms | 105.77 ms |
| 4 MiB ASCII, high density | 103.74 ms | 25.16 ms | 128.00 ms |
| 4 MiB Unicode/scalar offsets | 111.90 ms | 19.48 ms | 129.81 ms |

The full `Rope -> String` snapshot accounts for roughly 81-93% of the measured 4 MiB regex end-to-end latency. Isolate overhead after the snapshot is small in the stable run (roughly 0.9-2.2 ms for the 4 MiB cases), so replacing Dart `RegExp` first would optimize the smaller part of the path while creating a compatibility and CodeForge-FRB conflict with C.

Compatibility regression coverage locks in Dart semantics for lookahead, backreferences, whole-word behavior, Unicode scalar offsets, and regex replace-all. Therefore the E2 decision is:

1. keep Dart `RegExp` as the compatibility authority for now;
2. do **not** add a native regex engine or CodeForge Rust/FRB API in E;
3. remove the dominant repeated-copy cost by caching one full-text snapshot per `documentVersion` while Find is active;
4. invalidate that cache on document revision, Find close/clear, failure, and controller disposal;
5. let C own any future retained-native-document/API shape. Revisit native regex only after C stabilizes and new profiling shows regex execution itself is still material.

The implemented version-aware snapshot reuse changes a 4 MiB five-query edit sequence from a median **692.60 ms** with a fresh snapshot per query to **285.64 ms** including the first cached snapshot, a reduction of **406.96 ms (~58.8%)** for that sequence. It also deduplicates concurrent requests for the same document revision and shares the snapshot with replace-all when the revision is unchanged.

Reproduction:

```text
flutter test integration_test/editor_regex_search_benchmark.dart -d windows
flutter test test/unit/editor_regex_search_compatibility_test.dart test/unit/versioned_text_snapshot_cache_test.dart
flutter analyze integration_test/editor_regex_search_benchmark.dart test/unit/editor_regex_search_compatibility_test.dart test/unit/versioned_text_snapshot_cache_test.dart third_party/code_forge/lib/code_forge/find_controller.dart third_party/code_forge/lib/code_forge/versioned_text_snapshot_cache.dart
```

Result: benchmark passed, 10 focused tests passed, and focused analyzer reported no issues.

### Parallelism rule

E1 benchmark/research is safe in parallel with C.

E2 native implementation is **not** a good parallel branch with C because both are likely to modify CodeForge Rust APIs, CodeForge generated FRB files, rope/editor wrappers, and possibly dependencies. Schedule E2 after C's API shape stabilizes or merge one before starting the other.

## 10. Work package F - Git history graph projection algorithm/cache

**Priority: P2. Dart-first work can start immediately.**

Git history retrieval is already Rust/libgit2 and already supports offset paging. The remaining obvious heavy part is the Dart swimlane/view-model projection.

Relevant code:

```text
rust/src/api/git.rs
rust/src/api/git_history_impl.rs
rust/src/api/git_history_refs.rs
lib/src/shared/infra/git/git_history_graph.dart
lib/src/features/workbench/application/workspace_git_history_loader.dart
lib/src/features/workbench/presentation/workspace_git_history_panel.dart
lib/src/features/workbench/presentation/workspace_git_history_surface.dart
```

### F1 - low-conflict Dart algorithm pass

Before moving anything native:

- eliminate linear parent lookup where a `commit id -> item` map is sufficient;
- avoid recomputing old page swimlanes when appending history pages;
- keep a continuation state for the final output swimlanes of the previous page;
- avoid rebuilding identical view models on unrelated widget rebuilds;
- benchmark 50/500/5k commit histories and merge-heavy histories.

This phase does not need root FRB changes and is safe to parallelize.

### F2 - native projection only if evidence justifies it

If Dart projection still dominates after F1, consider returning a native graph projection or page continuation state. F2 would modify the root Alera FRB surface and must use the integration rules below.

## 11. Work package G - Workspace search result projection/paging

**Priority: P2. Benchmark/Dart-first work can start immediately.**

Search itself is already native and cancellable. Rust currently owns search matching/result construction; Flutter still builds grouped/collapsible rows and performs some per-directory sorting/projection.

Relevant code:

```text
rust/src/api/workspace_search/*
lib/src/rust/api/workspace_search.dart
lib/src/features/workbench/application/workspace_search_controller.dart
lib/src/features/workbench/application/workspace_search_service.dart
lib/src/features/workbench/presentation/workspace_search_panel*.dart
```

### G1 - profile current projection

Use large result sets (for example 1k/10k/50k matches where practical) and separate:

- native scan time;
- FFI result payload size/time;
- Dart grouped-tree construction;
- sorting;
- widget row materialization;
- memory/GC.

First optimize avoidable Dart recomputation/caching if that is sufficient.

### G2 - native projection/paging only if evidence justifies it

Possible direction:

```text
search session / result handle
-> grouped/sorted page projection
-> only visible/expanded rows or bounded result pages cross FFI
```

G2 modifies the root FRB surface and therefore conflicts at integration time with A/F2 and any other root native API branch.

## 12. Work package H - Rust terminal core shadow PoC

**Status: deliberately deferred. Do not assign as active work unless explicitly reopened.**

The roadmap still recommends a shadow emulator approach, but the project has intentionally paused the terminal-core migration.

When reopened, do it as a separate architecture track:

```text
xterm2 authoritative
+ Rust terminal core shadow
-> canonical state comparison
-> viewport snapshots
-> changed-row deltas
-> finally host-owned search/scrollback
```

Do not combine terminal model migration with a renderer rewrite.

## 13. Work package I - Explicit non-priorities

Do not spend a worker on these without profile evidence:

- converting individual `existsSync()`/single-file reads/path checks to Rust;
- external-editor executable existence checks;
- one-off `.git` / `commondir` reads;
- small workspace/project validation syscalls;
- ICO decode, which is already moved off the UI isolate using `compute()`;
- terminal buffer accounting as a standalone native project.

Small syscalls can cost less than adding another FFI API and ownership layer.

## 14. Parallel work waves

### Wave 1 - can run at the same time now

This is the recommended initial multi-worker split.

| Worker | Branch suggestion | Assignment | Conflict risk with other Wave 1 workers |
| --- | --- | --- | --- |
| 1 | `perf/rust-agent-overlay` | A - native agent overlay reconciliation | Low with B/D/E1/F1/G1; root-FRB conflict only with future native F2/G2 |
| 2 | `perf/editor-large-profile` | B - post-current-main editor profile + remaining bounded hot-path fixes | Low if E1 stays benchmark-only |
| 3 | `perf/terminal-search-profile` | D - terminal search benchmark/current Dart optimization | Very low |
| 4 | `perf/editor-regex-benchmark` | E1 - regex snapshot/copy/compatibility benchmark | Low; avoid implementation touching the same CodeForge API as C until coordinated |
| 5 | `perf/git-history-projection` | F1 - Dart graph algorithm/cache + benchmark | Very low |
| 6 | `perf/workspace-search-projection` | G1 - search result projection/payload benchmark + Dart caching | Very low |

All six can be worked on simultaneously if each uses its own worktree and stays within the stated phase boundary.

### Wave 2 - evidence-driven implementation

Start after Wave 1 results are known:

1. **C NativeEditorDocument** after B's profile gate.
2. **E2 native regex** only if E1 shows meaningful cost and after C API/dependency decisions are stable.
3. **F2 native Git history projection** only if F1 is insufficient.
4. **G2 native workspace search projection/paging** only if G1 shows meaningful Dart/FFI cost.

C can run in parallel with A because C uses the CodeForge FRB surface while A uses the root Alera FRB surface.

F2 and G2 can be developed in separate worktrees at the same time, but their root generated bindings will conflict at merge time. Treat them as parallel **implementation** with serialized **integration**.

## 15. Conflict matrix

Legend:

- `LOW`: normally safe to develop and merge independently.
- `MEDIUM`: source areas differ, but generated/dependency files may conflict.
- `HIGH`: likely same hand-written source/API/generated surface; avoid simultaneous implementation unless deliberately coordinated.

| Pair | Risk | Reason |
| --- | --- | --- |
| A overlay native vs B editor profile | LOW | root agent API vs editor/CodeForge profiling |
| A overlay native vs C NativeEditorDocument | LOW | root Alera FRB vs separate CodeForge FRB |
| A overlay native vs D terminal search Dart | LOW | different subsystems |
| A overlay native vs E1 regex benchmark | LOW | root Alera vs CodeForge benchmark |
| A overlay native vs F1 Git graph Dart | LOW | root native API vs Dart-only graph work |
| A overlay native vs G1 workspace search Dart/profile | LOW | root native API vs existing search UI/profile |
| A overlay native vs F2 Git native projection | MEDIUM/HIGH integration | hand-written APIs differ, but both regenerate root FRB |
| A overlay native vs G2 search native projection | MEDIUM/HIGH integration | hand-written APIs differ, but both regenerate root FRB |
| C NativeEditorDocument vs E1 regex benchmark | LOW/MEDIUM | benchmark can remain separate; may touch shared tests |
| C NativeEditorDocument vs E2 native regex | HIGH | CodeForge Rust API, generated bindings, rope/editor wrappers, possible Cargo dependency changes |
| F1 Git graph Dart vs G1 search Dart/profile | LOW | separate feature areas |
| F2 Git native vs G2 search native | MEDIUM/HIGH integration | different Rust source modules but same root generated FRB files |
| D terminal search Dart vs all non-terminal packages | LOW | isolated UI/controller work |
| H terminal core PoC vs D terminal search native redesign | HIGH architecture | terminal authority/search ownership decisions must be one design |

## 16. FRB generated-file merge policy

Parallel work is practical only if generated bindings are treated as generated artifacts rather than hand-merged source.

### 16.1 Root Alera FRB surface

Root API changes regenerate files under roughly:

```text
rust/src/frb_generated.rs
lib/src/rust/frb_generated.dart
lib/src/rust/frb_generated.io.dart
lib/src/rust/frb_generated.web.dart
lib/src/rust/api/*.dart
```

Root project source-of-truth command is documented by the Makefile:

```text
flutter_rust_bridge_codegen generate
```

Do not hand-edit function IDs/content hashes to resolve generated conflicts.

If two root-native branches are merged:

1. merge/reconcile the **hand-written Rust API/model sources** first;
2. reconcile hand-written Dart integration code;
3. discard conflict-resolution attempts inside generated bindings;
4. run `flutter_rust_bridge_codegen generate` from the root source-of-truth state;
5. review the generated diff;
6. run focused tests/analyzer/diff-check.

### 16.2 CodeForge FRB surface

CodeForge has its own FRB configuration:

```text
third_party/code_forge/flutter_rust_bridge.yaml
rust_root: rust/
dart_output: lib/src/rust
```

Its generated surface lives under the vendored package, for example:

```text
third_party/code_forge/rust/src/frb_generated.rs
third_party/code_forge/lib/src/rust/frb_generated.dart
third_party/code_forge/lib/src/rust/frb_generated.*.dart
third_party/code_forge/lib/src/rust/api/*.dart
```

Root Alera FRB changes and CodeForge FRB changes are separate and can normally be developed in parallel.

C and E2 both use the CodeForge surface, so they should be serialized or explicitly rebased/regenerated after one another.

## 17. Dependency/Cargo conflict rules

Native branches may also collide in dependency manifests/locks.

- A should prefer existing root dependencies/std APIs where possible.
- C may need parser dependencies in `third_party/code_forge/rust/Cargo.toml`.
- E2 may need a regex engine dependency in the same CodeForge Cargo manifest as C.

Therefore C and E2 should not independently choose/add competing CodeForge dependencies without coordination.

If two branches alter the same Cargo manifest/lock, merge the intended dependency set explicitly, regenerate/update the lock using Cargo, and rerun the full focused Rust gate rather than accepting a textual lock-file merge blindly.

## 18. Worktree/branch rules for delegated workers

Each worker should:

1. create a dedicated worktree/branch from the latest agreed integration base;
2. record the exact base commit in their handoff/PR/commit notes;
3. preserve unrelated changes;
4. avoid `git add -A`;
5. commit exact paths per batch;
6. use TDD/focused regression tests for behavior/invariant changes;
7. benchmark before/after for performance claims;
8. do not rewrite another worker's subsystem "while nearby";
9. do not hand-edit FRB generated IDs/content hashes;
10. re-check `main` HEAD/status before integration because multiple workers may finish out of order.

Suggested branches:

```text
perf/rust-agent-overlay
perf/editor-large-profile
perf/terminal-search-profile
perf/editor-regex-benchmark
perf/git-history-projection
perf/workspace-search-projection
perf/editor-native-document      # start after B gate
```

## 19. Recommended merge order

For the first parallel wave, prefer integrating low-conflict measurement/Dart-only work before native API surfaces when they are ready and independently validated:

```text
B / D / E1 / F1 / G1
```

Their exact order is flexible because they normally touch distinct areas.

Then integrate native API tracks:

```text
A root-native overlay
-> regenerate root FRB from source-of-truth
-> focused integration gate

C CodeForge NativeEditorDocument (after B gate)
-> regenerate CodeForge FRB from source-of-truth
-> editor focused integration gate
```

If F2/G2 are later justified:

```text
merge one root-native branch
-> regenerate root FRB
-> validate
-> merge next root-native branch
-> regenerate root FRB again
-> validate again
```

Do not try to preserve generated function numbering from two branches by manual merge.

E2 native regex should follow C once C changes the CodeForge API/dependency shape, unless C is explicitly postponed.

## 20. Per-worker deliverable template

Every delegated work package should return:

```text
Branch/worktree:
Base commit:
Final commit(s):

Problem measured:
Baseline benchmark:
Implementation summary:
Behavior/parity tests:
Post-change benchmark:
Known limitations/fallback:

Hand-written files changed:
Generated files changed:
Cargo/pubspec dependencies changed:

Expected conflicts with other work packages:
Recommended merge/rebase order:
```

This makes integration possible without relying on chat history.

## 21. Definition of done for each package

A work package is not complete just because code compiles.

Required where applicable:

- behavior parity/regression coverage;
- stale/cancelled operation safety;
- bounded retained native state lifetime;
- failure/fallback semantics;
- five-sample or equivalent reproducible benchmark for performance decisions;
- UI-isolate/Rust/RSS/FFI evidence appropriate to the work;
- generated bindings regenerated from source-of-truth;
- focused analyzer/lint/tests;
- `git diff --check`;
- exact-path commit(s);
- short handoff containing conflicts/merge order.

## 22. Current recommended staffing split

If six people are available **right now**, use:

```text
Person 1 -> A  Agent runtime overlay Rust migration
Person 2 -> B  Large-file editor post-current-main profiling
Person 3 -> D  Terminal search benchmark / bounded Dart optimization
Person 4 -> E1 Regex snapshot/copy benchmark + compatibility research
Person 5 -> F1 Git history graph algorithm/cache + benchmark
Person 6 -> G1 Workspace search result projection benchmark/cache
```

When B reports the parser gate is ready, start a seventh or reassigned worker on:

```text
C -> NativeEditorDocument incremental parser + viewport syntax spans
```

Do not start E2 native regex implementation in parallel with C unless the workers explicitly divide the CodeForge API/generated/dependency ownership.

## 23. Current top-line execution order

The current priority is therefore:

```text
A. Agent runtime overlay Rust migration
   ||
B. Editor profile gate
   ||
D. Terminal search benchmark
   ||
E1. Regex benchmark
   ||
F1. Git history Dart projection optimization
   ||
G1. Workspace search projection benchmark

B passes gate
   -> C. NativeEditorDocument incremental parser

Evidence only
   -> E2 / F2 / G2 native follow-ups

Explicit product/maintainer reopen only
   -> H. Rust terminal core shadow PoC
```

The key coordination rule is: **parallelize domain work, serialize generated-surface integration.** Root Alera FRB and CodeForge FRB are separate integration lanes, so one worker may safely implement A while another implements C after its profile gate; however two workers changing the same FRB lane should expect the later integration to regenerate bindings from the combined hand-written source rather than manually merge generated output.
