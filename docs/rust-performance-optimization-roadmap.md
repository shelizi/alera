# Rust Performance Optimization Roadmap

Status: active / partially implemented

This document records candidate Alera performance work that can move CPU-, filesystem-, parsing-, indexing-, or projection-heavy work from Dart/Flutter into Rust. It is a roadmap, not a mandate to rewrite every loop in Rust.

The guiding rule is to reduce total work and data movement, not merely change implementation language.

## Goals

- Keep expensive filesystem, parsing, indexing, diff projection, compression, and terminal-state work off the Flutter UI isolate.
- Reduce large Rust -> FFI -> Dart object graphs that are immediately regrouped, sorted, or projected again in Dart.
- Prefer native ownership of snapshots and indexes, then return only the UI projection or delta that Flutter needs.
- Preserve current behavior with TDD, shadow comparisons, and bounded migrations.
- Measure before and after on the same machine/build/display configuration, following `docs/performance.md`.

## Architecture direction

Avoid this pattern when the payload is large:

```text
Rust computes
  -> FFI sends a large flat payload
  -> Dart allocates objects
  -> Dart sorts/groups/parses/reconciles
  -> Flutter renders
```

Prefer:

```text
Rust owns snapshot / index / parser state
  -> Rust derives the requested projection or delta
  -> FFI sends only UI-needed data
  -> Flutter renders
```

This can improve CPU time, GC pressure, RSS, FFI serialization cost, and UI responsiveness together.

## Priority summary

| Priority | Area | Current remaining Dart-heavy work | Proposed Rust responsibility | Expected benefit |
| --- | --- | --- | --- | --- |
| P0 | Agent runtime resource sync | Recursive sync filesystem traversal, fingerprinting, JSON hashing, copy/delete | Reconcile/fingerprint/link/copy/delete in one native operation | High UI-isolate and filesystem benefit |
| P0 | Git status projection | Mapping, grouping, sorting, tree rows, reconciliation | Native status projection and optional snapshot/delta ownership | High on large repos |
| P1 | Git diff projection | Hunk parsing, line alignment, full-file/side-by-side row construction | Native aligned rows plus paging | High on large diffs/files |
| P1 | Terminal emulator ownership | Dart/xterm VT parsing and cell-buffer mutation | Rust terminal core plus viewport delta | Potentially very high, but large migration |
| P1 | Terminal scrollback search | Dart scans xterm lines and rebuilds match lists | Rust host-side searchable scrollback index | High for deep histories |
| P2 | Diagnostics ZIP | Sync file reads plus in-memory ZIP | Streaming ZIP in Rust | Medium, low risk |
| P2 | Syntax highlighting/parser | Dart editor highlighting/parsing | Incremental native parser/highlight spans | Potentially high; profile first |
| P3 | ICO decoding | CPU decode already moved to isolate | Native decode | Low incremental gain |
| P3 | Terminal buffer accounting | Dart scans buffer lines | Cached/native accounting | Low unless profiling says otherwise |

### Current implementation snapshot (2026-09-15)

- Agent runtime recursive fingerprint/copy/delete and copy reconciliation are merged into `main` through `b5630823`; small top-level ownership/link decisions intentionally remain in Dart.
- Git status grouping/tree projection, allocation-light reconciliation, native entry-index rebinding, and linear merging of already-sorted Unified Changes groups are merged into `main` through `b5630823`.
- `main` through `b5630823` has native full-file diff alignment/single-column projection plus lazy materialization for full-file and side-by-side rows; row-level native paging remains future work.
- `integration_test/terminal_parser_benchmark.dart` provides a parser/model-only xterm2 baseline. Terminal-core migration has not started.
- The diagnostics ZIP Rust prototype is preserved on `perf/diagnostics-streaming-zip-v2` and intentionally kept off `main` until it writes directly to an output file and the Dart production path stops building/returning the whole archive in memory.

---

## P0 - Agent runtime resource reconciliation

### Current state

The original Claude/Codex path performed recursive synchronous filesystem work in Dart. The heavy recursive fingerprint/copy/delete/reconcile path is now merged into `main` through `b5630823`; small top-level ownership/link decisions remain in Dart, and pure-Dart fallbacks remain available for compatibility/testing.

Relevant code:

- `lib/src/features/agent_status/infra/claude_runtime_resources.dart`
  - `_resourceFingerprint()` recursively uses sync filesystem APIs, sorts entries, builds a JSON structure, then hashes it.
  - `_copyEntity()` recursively copies files/directories.
  - `_deleteEntity()` synchronously deletes resources.
  - runtime sync/link fallback work is also Dart-side.
- `lib/src/features/agent_status/infra/codex_runtime_resource_sync.dart`
  - contains closely related fingerprint/copy/delete logic.

### Problem

This work is syscall-heavy and allocation-heavy and can block the Flutter isolate. The Claude/Codex implementations also duplicate responsibility.

### Proposed Rust API

Introduce one shared native reconciliation path, conceptually:

```text
reconcile_agent_runtime_resources(request)
  -> changed
  -> linked
  -> copied
  -> removed
  -> warnings/errors
```

Rust should own:

- recursive enumeration
- deterministic fingerprinting
- marker comparison
- symlink/junction handling where appropriate
- copy fallback
- stale resource removal
- atomic/guarded updates where feasible

### Migration plan

1. Add native fingerprint implementation with golden parity tests against the Dart result.
2. Add dry-run reconciliation that returns a plan without mutating files.
3. Compare Dart and Rust plans in tests/shadow mode.
4. Enable Rust mutation path.
5. Remove duplicated Claude/Codex recursive sync code.

### Success criteria

- Identical runtime resource results.
- No synchronous recursive traversal on the Flutter UI isolate.
- One implementation shared by all agents that use the same resource model.

---

## P0 - Git status projection, grouping, and reconciliation

### Current state

Git repository work is largely native/libgit2, and native grouping/tree projection is already in place. `main` through `b5630823` also removes the common allocation-heavy Dart reconciliation/rebinding paths and linearly merges the already-sorted groups used by Unified Changes without enlarging the FFI payload shape.

Relevant code:

- `lib/src/shared/infra/git/rust_git_backend_result_mappers.dart`
  - maps native entries to Dart objects.
  - deliberately yields while processing chunks.
  - calls Dart grouping logic afterward.
- `lib/src/shared/infra/git/git_change_group_models.dart`
  - staged/unstaged/untracked split.
  - sorting.
  - tree-row creation.
  - unified grouping.
- `lib/src/shared/infra/git/git_change_entry_models.dart`
  - reconciles old/new entry object lists.
- `lib/src/features/workbench/application/workspace_source_control_controller.dart`
  - combines status/repo-state/stash results and performs Dart reconciliation/rebinding.

### Problem

Large repositories can pay for:

```text
native status scan
-> serialize flat list
-> Dart object allocation
-> sort/group/tree projection
-> reconcile against previous Dart snapshot
```

### Proposed Rust model

Return a UI-ready projection, for example:

```text
GitStatusProjection
  revision/hash
  entries
  groups
  staged_tree_rows
  unstaged_tree_rows
  untracked_tree_rows
  unified_tree_rows
```

Longer term, the Rust repository service should own the previous snapshot and return deltas or stable IDs rather than forcing Dart to reconstruct the complete state every refresh.

### Migration plan

1. Port group classification/sort semantics to Rust with parity tests.
2. Port tree-row projection.
3. Preserve existing Dart projection in shadow mode and diff results.
4. Return native projection over FRB.
5. Add optional native snapshot/delta ownership only after projection parity is stable.

### Success criteria

- Same status ordering and grouping as current UI.
- Reduced Dart allocation and chunk-yield work on large repositories.
- Stable refresh performance as status entry count grows.

---

## P1 - Git diff aligned/full-file projection

### Current state

Git diff generation is native and `main` now also includes native full-file alignment/single-column projection plus lazy row materialization. Remaining work is to finish parity across all side-by-side/full-file paths and add paging/virtualization so very large diffs do not require eager Dart object graphs.

Relevant code:

- `lib/src/features/workbench/presentation/workspace_git_diff_surface_full_file.dart`
  - `_buildFullFileRows()` parses hunks, tracks line numbers, fills unchanged context, and constructs rows.
  - `_buildFullFileSideBySideRows()` splits old/new files, aligns context, queues deletions/additions, and allocates side rows.
- `lib/src/features/workbench/presentation/workspace_git_diff_surface_side_by_side.dart`
  - `_decodedDiffLines()` and `_buildSideBySideRows()` parse hunks, pair additions/deletions, and derive line numbers.

### Proposed Rust output

Keep the Rust model presentation-neutral:

```text
DiffRow
  old_line: optional integer
  new_line: optional integer
  old_text: optional string
  new_text: optional string
  kind: context | add | delete | replace | meta
```

For large files, do not return every row eagerly. Add paging/virtualization:

```text
diff_projection(diff_id, start_row, limit)
```

Rust may also own the parsed diff snapshot so paging does not reparse the patch on every request.

### Interaction with whitespace options

The native projection must preserve Alera's configurable whitespace/line-ending comparison semantics and the persisted user option. Filtering only in the Flutter renderer is insufficient if it causes row alignment to disagree with the actual comparison mode.

### Success criteria

- Same line alignment and hunk semantics as current views.
- Large diffs do not allocate all UI rows in Dart.
- Editable workspace-side comparison still maps edits to the correct workspace lines.

---

## P1 - Rust terminal emulator/core

### Current state

Alera already puts substantial terminal infrastructure in Rust:

- PTY/Terminal Host
- history/persistence
- resource handling
- orchestration/backpressure

But live terminal output ultimately returns to Flutter/xterm2, where Dart still performs VT/ANSI parsing and mutates the cell buffer.

Conceptually today:

```text
PTY bytes
  -> Rust Terminal Host
  -> Dart String/output chunks
  -> xterm2 Terminal.write()
  -> Dart VT parser/cell buffer
  -> Flutter renderer
```

### Candidate Rust terminal cores

#### Preferred PoC: official WezTerm `wezterm-term`

Use the official `wezterm-term` core from the WezTerm repository as the first feature-complete proof-of-concept candidate, while keeping xterm2 authoritative during shadow validation.

Why it fits:

- full terminal state model rather than a parser-only crate
- strong VT/xterm, Unicode, scrollback, hyperlink, shell-integration, and terminal-graphics coverage
- Sixel and iTerm2/Kitty-style graphics capabilities provide an upgrade path beyond the current Alera xterm2 fork
- Alera already owns PTY/Terminal Host infrastructure in Rust, so the emulator boundary is natural

Integration constraints:

- `wezterm-term` is not published as a stable crates.io API for external consumers
- upstream does not guarantee API stability outside WezTerm itself
- pin the Git dependency to an exact commit SHA and hide it behind an Alera-owned `AleraTerminalCore` adapter
- do not fork initially; fork only if a concrete upstream/API constraint requires it
- keep Flutter as the renderer/input/IME integration layer during the PoC

#### Fallback/benchmark candidate: `alacritty_terminal`

`alacritty_terminal` remains a useful fallback and performance comparison because it is published independently and exposes attractive damage/search primitives. Its protocol feature breadth is lower than WezTerm for Alera's current xterm2 compatibility target, so it is no longer the preferred feature-complete first PoC.

#### Secondary candidate: `termwiz`

Useful concepts include cell/surface/change/delta representations and escape handling, but its suitability as Alera's long-term emulator state owner should be evaluated separately. It may also inspire the Alera-native viewport-delta protocol without becoming the core dependency.

#### Lightweight parser crates

Crates that only parse ANSI/VT sequences are not sufficient to replace xterm2 by themselves. Alera needs a full terminal state model: grid, cursor, alternate screen, scrollback, selection semantics, resize/reflow behavior, Unicode width behavior, modes, hyperlinks, and compatibility behavior.

### Target architecture

```text
PTY bytes
  -> Rust Terminal Host
  -> Rust terminal emulator/model
       - grid
       - cursor
       - alternate screen
       - scrollback
       - modes
       - search index
       - hyperlinks/metadata
  -> viewport snapshot or changed-row delta
  -> Flutter renderer
```

The biggest win is not simply faster ANSI parsing. It is eliminating this repeated boundary:

```text
Rust bytes/state -> Dart strings -> Dart parser -> Dart cells
```

and replacing it with bounded presentation data.

### Required migration phases

#### Phase 0 - Profile gate

Follow `docs/performance.md` before changing the terminal protocol. Current project policy says protocol/architecture changes should be justified by profiling. Record:

- UI-isolate CPU in sustained output
- xterm parser time
- cell-buffer mutation/allocation time
- FFI serialization/transport cost
- raster time separately from parser/model work
- restore latency and memory

Do not attribute GTK/Linux composition overhead to the terminal parser.

#### Phase 1 - Shadow emulator

Feed identical PTY output to both:

```text
xterm2 (authoritative)
Rust terminal core (shadow)
```

Compare canonical state after bounded checkpoints:

- visible text
- cursor position/style
- screen dimensions
- normal/alternate screen
- attributes/colors where relevant
- title/mode events
- scrollback tail hashes

Coverage must include:

- shells
- Claude/Codex/Grok/AGY interactive output
- TUIs
- resize/reflow
- CJK/wide characters
- combining characters
- Unicode emoji where currently supported
- ANSI color/style
- OSC sequences
- bracketed paste/mouse modes where applicable

#### Phase 2 - Native state, Flutter viewport snapshots

Keep Flutter rendering, but make Rust authoritative for terminal state. Start with coarse snapshots for only the visible viewport plus small overscan.

#### Phase 3 - Changed-row/delta protocol

Once correctness is stable, return only dirty rows/cursor/metadata deltas instead of full visible snapshots.

#### Phase 4 - Native scrollback/search ownership

Unify terminal search, restoration, and history around the Rust-owned terminal state so Flutter no longer needs to scan xterm buffers.

### Renderer strategy

Do not combine terminal-model migration with a renderer rewrite. First preserve the existing visual output and Flutter integration. A custom `CustomPainter`/GPU-optimized renderer can be evaluated later using independent raster benchmarks.

### Rollback strategy

Keep an internal feature flag during migration so xterm2 remains available until parity and performance gates are satisfied.

---

## P1 - Terminal scrollback search/index

### Current state

Terminal search is currently Dart-side against the xterm buffer.

Relevant code:

- `lib/src/features/workbench/presentation/terminal_search_controller.dart`
  - refreshes visible/all ranges, scans lines, collects matches, and sorts/rebuilds match state.
- `lib/src/features/workbench/domain/terminal_search.dart`
  - performs per-line literal search.
  - case-insensitive search lowercases content/query and scans via `indexOf`.

### Proposed direction

If Rust owns terminal state, maintain an incremental searchable scrollback index in the Terminal Host.

Conceptual API:

```text
search_terminal(session_id, query, options, cursor/page)
  -> line_id / logical_line
  -> start/end columns
  -> match preview
```

Do not copy 100,000 terminal lines over FFI merely to search them in Rust; the host should own the searchable history/index.

### Benefits

- search background/off-screen terminals
- avoid repeated full Dart scans
- easier resume/reattach behavior
- same index can survive UI eviction if host state persists

---

## P2 - Diagnostics bundle ZIP

### Current state

`lib/src/features/diagnostics/infra/diagnostics_bundle_builder.dart` currently performs synchronous listing/reads and builds an archive in Dart memory before ZIP encoding.

### Proposed Rust API

```text
create_diagnostics_bundle(
  app_log_dir,
  runtime_log_dir,
  output_path,
  redaction_options
)
```

Implement streaming ZIP creation so logs do not need to be loaded into one large Dart-side archive object.

### Benefits

- lower peak memory
- less UI-isolate blocking
- simple, low-risk migration

Security/redaction behavior must remain identical or improve before switching the implementation.

---

## P2 - Editor syntax parsing/highlighting

### Current state

The workbench editor currently relies on Dart-side editor/highlighting infrastructure (`CodeForge` / `re_highlight` integration in `workspace_editor_surface.dart`).

### Candidate direction

Evaluate an incremental Rust parser, likely tree-sitter-based, that returns only changed highlight/syntax spans.

Possible future uses:

- syntax highlighting
- symbols/outline
- folding
- bracket matching
- structural selection

### Gate

Do not migrate this only because Rust is available. Profile large-file editing first. FFI transfer of very large span lists can erase parser gains.

Prefer:

```text
Rust owns parse tree
-> edit delta in
-> changed spans/symbol deltas out
```

over reparsing/transferring the full document on every edit.

---

## P3 - ICO/image decoding

`lib/src/features/workbench/application/workspace_image_decoding.dart` contains CPU-bound ICO-to-PNG decoding, but callers already move the decode off the UI isolate with `compute(...)`.

Rust/native decoding may still be faster and reduce Dart code, but the incremental product benefit is smaller than the P0/P1 work. Only prioritize if profiling shows image previews are material to startup/navigation latency or memory.

---

## P3 - Terminal buffer memory accounting

`lib/src/features/workbench/presentation/terminal_buffer_budget.dart` scans terminal lines to estimate allocated cell-buffer bytes.

Options:

- maintain incremental accounting in the current xterm fork
- expose accounting from a future Rust terminal state owner
- cache values and update only on row allocation/compaction changes

Do not migrate this independently unless it is shown to be a hot path.

---

## Explicit non-priorities

### Source-control action loops

Some source-control operations iterate a small number of paths in Dart (for example old/new paths for rename). These are not automatically good Rust migration targets because the path count is normally tiny and the actual Git operation is already native.

### Rewriting the entire terminal renderer immediately

The terminal emulator/model and the Flutter renderer are separate optimization problems. A native emulator PoC must not force Alera to abandon Flutter rendering in the same change.

### Moving every Dart loop into Rust

A small pure-Dart loop can be cheaper than an FFI call. Migrate when one or more of these hold:

- filesystem/system calls block the UI isolate
- large data is being parsed/transformed repeatedly
- state ownership belongs naturally in the Rust host
- Dart allocates large intermediate object graphs
- native paging/indexing can eliminate work
- profiling demonstrates a meaningful hot path

---

## Cross-cutting native API rules

### Prefer handles and snapshots over giant payloads

Examples:

```text
open_git_status_snapshot(repo) -> snapshot_id
get_git_status_projection(snapshot_id, view/options)

open_diff_projection(repo, file, options) -> diff_id
get_diff_rows(diff_id, start, limit)

terminal_attach(session_id) -> terminal_state_id
get_terminal_viewport(terminal_state_id, revision)
```

### Add revision IDs

Every native-owned mutable projection should expose a monotonically increasing revision/generation so Dart can reject stale responses without reconstructing state.

### Support cancellation

Long scans/projections must accept cancellation when the workspace/query/view generation changes, following the existing cancellable workspace-search pattern.

### Bound memory

Any native snapshot/index should have:

- explicit ownership/lifetime
- per-workspace or global caps where appropriate
- release on workspace/session close
- bounded caches
- metrics/diagnostics for retained bytes

### Keep UI models stable during migration

Where possible, add a native adapter behind current Dart interfaces first. Avoid forcing unrelated presentation changes into performance migrations.

---

## Benchmark and verification plan

Each migration should add a targeted benchmark or extend an existing one. At minimum measure:

- median and p95/p99 latency where applicable
- Flutter UI-isolate CPU
- Rust/host CPU separately
- app RSS and host RSS
- allocation/GC evidence when available
- FFI payload size/count
- frame count and raster time for visible UI paths

Use at least five comparable samples for optimization decisions as required by `docs/performance.md`.

### Suggested new harnesses

1. Agent resource sync benchmark
   - cold recursive scan
   - no-change sync
   - one-resource change
   - copy fallback path

2. Git status projection benchmark
   - 1k / 10k / 50k status entries
   - grouped/tree/unified projection
   - refresh with small delta

3. Git diff projection benchmark
   - medium diff
   - 100k-line file
   - whitespace-ignore on/off
   - paged side-by-side projection

4. Terminal emulator shadow benchmark
   - sustained output
   - escape-sequence-heavy restore
   - TUI repaint
   - CJK/wide-character workload
   - compare xterm2 vs Rust emulator CPU/RSS/latency

5. Terminal search benchmark
   - 10k/100k logical lines
   - literal/case-insensitive query
   - repeated incremental output between searches

6. Diagnostics ZIP benchmark
   - many small logs
   - several large logs
   - peak RSS and wall time

---

## Recommended execution order

### Batch 1 - high benefit / bounded risk

1. Agent runtime resource reconciliation -> Rust.
2. Git status grouping/tree projection -> Rust.

### Batch 2 - large repository/file scalability

3. Git diff aligned/full-file projection -> Rust.
4. Add native paging/virtualization for large diff projections.

### Batch 3 - terminal proof of concept

5. Measure xterm parser/model cost against the current terminal performance guardrails.
6. Add an official `wezterm-term` shadow-mode PoC pinned to an exact upstream Git SHA and hidden behind an Alera-owned adapter/feature flag if the profile gate justifies it.
7. Compare canonical xterm2/Rust terminal state across real Alera workloads.
8. Move terminal search/index into Rust host ownership once terminal-state ownership is proven.

### Batch 4 - lower-risk supporting work

9. Streaming diagnostics ZIP.
10. Re-profile editor large-file behavior; only then decide whether native incremental syntax parsing is justified.

---

## Terminal decision record

Current recommendation for a terminal-core PoC:

```text
official wezterm-term pinned to an exact Git SHA
  + AleraTerminalCore adapter
  + Alera-owned terminal viewport/delta protocol
  + existing Flutter renderer first
  + xterm2 retained as the authoritative shadow/rollback baseline
```

Reasons:

- avoids writing a VT emulator from scratch
- provides the strongest feature-completeness match for Alera's current xterm2 fork, including a path to richer terminal graphics protocols
- aligns with the existing Rust PTY/Terminal Host boundary
- lets terminal state/search/history become host-owned while keeping renderer/IME migration independent
- supports a safe xterm2 shadow/rollback path

Integration policy:

- pin the official WezTerm repository dependency to an exact commit because `wezterm-term` is not offered as a stable external crates.io API
- keep all upstream types behind an Alera-owned adapter so upstream API churn is localized
- do not fork initially; fork only for a demonstrated upstream/API constraint
- keep `alacritty_terminal` as the fallback and benchmark candidate if WezTerm maintenance cost becomes unacceptable

---

## Definition of done for any Rust migration

A migration is complete only when:

- behavior parity is covered by tests
- stale/cancelled operations are safe
- failure and fallback behavior are defined
- native state has bounded lifetime/memory
- performance improves in a reproducible benchmark, or the change removes demonstrated UI-isolate blocking with no material regression
- old Dart implementation is removed only after parity has been proven
- documentation and diagnostics are updated where ownership changes

The intended outcome is not "more Rust." It is less duplicated work, less UI-isolate blocking, smaller cross-language payloads, and clearer ownership of heavy state.
