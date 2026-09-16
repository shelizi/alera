# Alera Large-File Editor Performance Handoff

Status: active / ready for continuation in a new conversation  
Date: 2026-09-16  
Primary repo: `E:\Dropbox\work\alera\alera`  
Active worktree: `E:\Dropbox\work\alera\.worktrees\codeforge-large-scroll-fastpath`  
Worktree branch: `perf/codeforge-large-scroll-fastpath`  
Main integration policy: validate -> commit worktree -> re-check main -> cherry-pick -> continue  
Push policy: **do not push unless explicitly requested**

## 1. Purpose of this handoff

This document is intended to let a fresh ChatGPT/Coding Tools conversation continue the Alera large-file editor performance work without relying on prior-chat memory.

The current goal is not a broad editor rewrite. The goal is to make large files remain responsive during typing, cursor movement, selection, scrolling, rendering, and background editor features by systematically removing whole-document work, synchronous O(n) work, avoidable allocations, and repeated computation from hot paths.

The long-term direction remains:

```text
persistent document/parser/index state
  -> incremental edit deltas
  -> viewport + bounded overscan query
  -> only UI-needed spans / metadata
  -> Flutter paints visible content
```

Do **not** move work to Rust merely because it can be moved. First reduce the amount of work and data movement. Rust is useful when ownership/state can stay native and only a bounded projection crosses FFI.

## 2. User/workflow expectations

Use these rules while continuing this task:

1. Work autonomously in small verified batches.
2. Prefer TDD or focused regression tests when behavior changes.
3. For pure internal performance refactors with unchanged behavior, use focused analyzer/widget tests plus `git diff --check`; add tests when there is meaningful observable or invariant behavior to protect.
4. After a successful batch: commit it and continue to the next batch without asking for confirmation.
5. Preserve unrelated changes.
6. Never use `git add -A` for these batches; commit exact paths only.
7. Never push unless the user explicitly asks.
8. Before integrating into `main`, re-read `main` HEAD/status because `main` may change concurrently.
9. Do not reconcile or modify the existing `origin/main` divergence as part of this task.
10. Terminal core migration is currently out of scope / paused.
11. Prefer evidence from reachable Alera code paths over optimizing dormant upstream CodeForge capabilities.

## 3. Current Git state at handoff creation

### Main

At the time this handoff was written, the latest relevant integrated commit on `main` was:

```text
050ddfd9 perf(editor): prune offset caches by offset range
```

Immediately before that:

```text
f96c106e perf(editor): reduce gutter paint overhead
958605c5 perf(editor): avoid per-paint prune closures
3fd3edbc perf(editor): reduce per-line paint overhead
ced46ed5 perf(editor): bypass large-file line offset cache
5d40c251 perf(editor): remove full text IME fallbacks
9cbbe76a perf(editor): avoid full text reads for forward word ops
d9e56df7 perf(editor): use IME projection on pointer focus
06992f10 perf(editor): avoid full text reads for backward word ops
883fe03c perf(editor): avoid full text reads for word hit tests
4fb6ad71 perf(editor): enter large-file mode before layout changes
7af51db2 perf(editor): avoid text snapshots for length checks
ca823a48 perf(editor): skip large-file layout map rebuilds
132fc1c1 perf(editor): disable folding for large files
e82c0386 perf(editor): skip large-file render highlighting
7d4090b3 perf(editor): add large-file CodeForge fast path
2bb927b1 build(editor): vendor code_forge 10.13.0
6c673b3c perf(editor): debounce large-file snapshots
3c0fb47b perf(editor): skip non-text controller snapshots
540e7329 perf(markdown): debounce live preview updates
```

Known `main` state when last checked:

```text
main...origin/main [ahead 617, behind 84]
?? .worktrees/
?? docs/history-session/
```

Those untracked paths are expected workspace artifacts. Do not delete or commit them as part of editor optimization.

### Worktree

Worktree path:

```text
E:\Dropbox\work\alera\.worktrees\codeforge-large-scroll-fastpath
```

Branch:

```text
perf/codeforge-large-scroll-fastpath
```

Before creating this handoff, the worktree code state was clean at:

```text
a0da3145 perf(editor): prune offset caches by offset range
```

This handoff document itself may be the only later worktree change/commit depending on when the next conversation starts.

## 4. Important repository layout

Primary Alera repo:

```text
E:\Dropbox\work\alera\alera
```

Performance worktree:

```text
E:\Dropbox\work\alera\.worktrees\codeforge-large-scroll-fastpath
```

Vendored editor package:

```text
third_party/code_forge
```

Primary file currently being optimized:

```text
third_party/code_forge/lib/code_forge/code_area.dart
```

Alera-side editor surface / profile logic is under the normal `lib/src/features/workbench/...` editor files.

Performance roadmap:

```text
docs/rust-performance-optimization-roadmap.md
```

Workspace history/checkpoint file is **outside the Git repo** at workspace root:

```text
E:\Dropbox\work\alera\docs\history-session\39.md
```

Do not confuse it with the repo-local untracked `alera/docs/history-session/` directory.

## 5. CodeForge vendoring decision

Alera vendors CodeForge 10.13 so hot-path changes can be controlled locally.

Root dependency override:

```yaml
dependency_overrides:
  code_forge:
    path: third_party/code_forge
```

Relevant upstream facts already checked:

- CodeForge 10.13 upstream tag commit: `62d2fe085c7fa2e4c5dd7f1678f04c9e0b364367`
- CodeForge 10.14 upstream commit checked during investigation: `0d75fa298b368bc6f47b0daa82aa6bee2b900815`
- Both still contained full `controller.text` behavior in the relevant edit hot path; upgrading alone would not solve Alera's large-file problem.
- CodeForge is MIT licensed.

Therefore: continue using focused local vendor patches rather than assuming a package upgrade eliminates the bottleneck.

## 6. Large-file mode thresholds and behavior

Current Alera large-file profile thresholds:

```text
workspaceEditorLargeFileLineThreshold = 5000
workspaceEditorLargeFileCharacterThreshold = 512 * 1024
```

Large-file profile currently disables:

- line wrap
- guide lines
- syntax highlighting
- folding

and enables CodeForge `largeFilePerformanceMode`.

This is a defensive interim mode, not the intended final architecture. The roadmap explicitly allows plaintext/no-highlight fallback until incremental parser + viewport-span delivery is proven stable.

The final desired behavior is to support syntax highlighting for large files without full-document reparsing or span transfer.

## 7. Guiding architecture

`docs/rust-performance-optimization-roadmap.md` establishes this rule:

```text
Avoid:
Rust computes
  -> FFI sends a large flat payload
  -> Dart allocates objects
  -> Dart sorts/groups/parses/reconciles
  -> Flutter renders

Prefer:
Rust owns snapshot / index / parser state
  -> Rust derives requested projection or delta
  -> FFI sends only UI-needed data
  -> Flutter renders
```

For editor syntax/parser work, the roadmap already states that the desired implementation is an incremental Rust parser, likely tree-sitter based, where Rust owns parse state for the lifetime of the document and returns changed ranges / viewport-first syntax data.

The large-file effort therefore has two phases:

### Phase A - reduce existing Dart/CodeForge hot-path waste

Remove:

- full rope -> `String` materialization
- full-document layout rebuilds
- whole-file word operations
- full-document IME projections
- avoidable map/string/list allocations per frame
- global collection scans in paint when avoidable
- cache invalidation/pruning with incorrect key semantics

### Phase B - incremental parser + viewport spans

Only after Phase A is sufficiently bounded and profiled:

- Rust retains parser/tree per open editor document
- edit deltas update native parser state
- query syntax spans for viewport + bounded overscan
- return only changed/visible spans
- include document/parser version in responses
- reject stale responses on Dart side
- share parse structure for syntax, symbols, folding, and bracket-related features where practical
- never send a giant whole-document span snapshot merely to claim the parser moved to Rust

## 8. Completed optimization batches

### 8.1 Markdown live preview debounce

Commit:

```text
540e7329 perf(markdown): debounce live preview updates
```

Behavior:

- 75 ms latest-only dirty preview update
- timer cancelled on path switch/dispose

Reason:

Large-file editing could trigger downstream markdown preview work too aggressively.

### 8.2 Avoid redundant editor surface refreshes

Earlier integrated change:

```text
92d1c9be perf(editor): avoid redundant surface refreshes
```

Key behavior:

- `EditorDocumentSession.updateCurrentText` returns whether text actually changed
- downstream refresh is skipped when unnecessary
- editor profile cache avoids rebuild unless dirty/profile changed

### 8.3 Gate controller snapshots by document version

Commit:

```text
3c0fb47b perf(editor): skip non-text controller snapshots
```

Before this change, cursor/selection/repaint notifications could trigger `controller.text` and materialize the whole rope.

Now `_handleControllerChanged` first checks CodeForge `documentVersion`.

Helper introduced:

```dart
@visibleForTesting
bool workspaceEditorShouldSyncControllerText({
  required int previousDocumentVersion,
  required int currentDocumentVersion,
}) {
  return previousDocumentVersion != currentDocumentVersion;
}
```

### 8.4 Debounce large-file document snapshots

Commit:

```text
6c673b3c perf(editor): debounce large-file snapshots
```

Large-file `EditorDocumentSession` full snapshots are now 75 ms latest-only rather than eagerly materialized on every edit.

Important preserved semantics:

- dirty state
- autosave behavior
- programmatic text replacement suppresses redundant handler work
- profile checks use cheap controller length/lineCount

### 8.5 Vendor CodeForge 10.13

Commit:

```text
2bb927b1 build(editor): vendor code_forge 10.13.0
```

This enables local hot-path changes without waiting for upstream changes.

### 8.6 CodeForge large-file edit fast path

Commit:

```text
7d4090b3 perf(editor): add large-file CodeForge fast path
```

New API:

```dart
final bool largeFilePerformanceMode;
```

Large-file mode now:

- does not read `controller.text` per content edit
- skips syntax highlighter `applyDocumentEdit`
- keeps rope-backed line/layout/cache invalidation
- clears transient AI response/ghost state
- keeps caret visibility
- sets `_lastProcessedText = null`
- still updates content version

When leaving large-file mode, materializing a baseline current text is intentional.

### 8.7 Skip render highlighting in large files

Commit:

```text
e82c0386 perf(editor): skip large-file render highlighting
```

Large-file mode bypasses:

- `preHighlightLines`
- synchronous cache-miss syntax highlighting

### 8.8 Disable folding for large files

Commit:

```text
132fc1c1 perf(editor): disable folding for large files
```

Large-file profile disables folding to keep layout uniform and avoid expensive fold computation/layout paths.

### 8.9 Uniform large-file layout fast path

Commit:

```text
ca823a48 perf(editor): skip large-file layout map rebuilds
```

When:

```text
largeFilePerformanceMode && !lineWrap && !enableFolding
```

Alera can use arithmetic line positioning rather than rebuilding the full layout map.

### 8.10 Avoid snapshots for simple length/empty checks

Commit:

```text
7af51db2 perf(editor): avoid text snapshots for length checks
```

Several call sites switched from `controller.text` to `controller.length`.

### 8.11 Enter large-file mode before layout feature changes

Commit:

```text
4fb6ad71 perf(editor): enter large-file mode before layout changes
```

Order matters. Large-file mode is enabled before folding/wrap changes so threshold crossing does not cause a useless full layout rebuild first.

### 8.12 Word hover/selection no longer reads full document

Commit:

```text
883fe03c perf(editor): avoid full text reads for word hit tests
```

Affected operations use line-local APIs:

```text
controller.getLineAtOffset
controller.getLineStartOffset
controller.getLineText
```

instead of whole-document `controller.text`.

### 8.13 Backward word keyboard operations

Commit:

```text
06992f10 perf(editor): avoid full text reads for backward word ops
```

Affected:

- `_deleteWordBackward()`
- `_moveWordLeft()`

These are now line/buffer local.

### 8.14 Pointer-focus IME projection

Commit:

```text
d9e56df7 perf(editor): use IME projection on pointer focus
```

Pointer focus no longer creates a whole-document `TextEditingValue` from `controller.text`; it uses the bounded current IME projection.

### 8.15 Forward word keyboard operations

Commit:

```text
9cbbe76a perf(editor): avoid full text reads for forward word ops
```

Affected:

- `_deleteWordForward()`
- `_moveWordRight()`

Helpers:

```text
_deleteWordForwardEnd(int caret)
_wordRightBoundary(int caret)
```

Important behavior already preserved:

1. Ctrl+Delete regex semantics: `^(\s*\w+|\s*[^\w\s]+)`
2. If remaining content is whitespace-only and regex fails, delete exactly one character as before.
3. Ctrl+Right on newline advances exactly one character.
4. Other token grouping uses the existing word/non-word/whitespace semantics.

Important offset detail:

CodeForge rope length/offsets are scalar-based, not UTF-16 string indices. Conversion helpers such as `scalarToStringIndex` must be respected.

### 8.16 Remove unreachable whole-document IME fallbacks

Commit:

```text
5d40c251 perf(editor): remove full text IME fallbacks
```

`currentTextEditingValue` is now non-null and always uses the bounded IME projection.

Removed unreachable `controller.text` fallbacks from:

- focus listener
- initial post-frame attach
- `_resetImeConnection`
- pointer-down focus path

Remaining `controller.text` usages in `code_area.dart` were intentionally narrowed to cases such as initial text load, leaving large mode baseline, and normal/small-file full-document tracking.

### 8.17 Bypass string-key line offset cache in uniform large mode

Commit:

```text
ced46ed5 perf(editor): bypass large-file line offset cache
```

`_getLineYOffset()` now returns arithmetic offset directly in uniform large-file layout:

```dart
if (_usesUniformLargeFileLayout) {
  return targetLine * _lineHeight;
}
```

This avoids repeated string allocation/map lookup/cache growth during large-file scroll.

### 8.18 Reduce per-line paint overhead

Commit:

```text
3fd3edbc perf(editor): reduce per-line paint overhead
```

Frame-stable values were hoisted out of the visible-line loop:

- RTL state
- wrap state
- folding-enabled state
- padding values
- content width
- paragraph width
- effective horizontal scroll

Also changed repeated `containsKey + []` patterns to a single nullable lookup for line/paragraph caches.

Fold lookup is skipped entirely when folding is disabled.

### 8.19 Avoid per-paint prune closures

Commit:

```text
958605c5 perf(editor): avoid per-paint prune closures
```

Local helper closures inside `_pruneViewportCaches()` were replaced with renderer methods so every paint does not construct closure/function objects just to check cache sizes.

Current general cache policy remains:

```text
keep margin: visible viewport +/- 400 lines
prune threshold: 3000 entries
```

### 8.20 Reduce gutter paint overhead

Commit:

```text
f96c106e perf(editor): reduce gutter paint overhead
```

This batch addressed multiple real per-frame costs:

1. Diagnostics are no longer copied + severity-sorted on every repaint. `_sortedDiagnostics` is prepared when diagnostics change.
2. Gutter reuses viewport information already computed by `paint()` instead of rediscovering it.
3. Diagnostic line severity expansion is clipped to the visible viewport instead of expanding every range across the entire file each frame.
4. Added bounded line-number paragraph cache.
5. Line-number `TextStyle.copyWith(color: ...)` is no longer repeated for each visible line; active/inactive/error/warning styles are prepared once per frame and then selected.
6. Fold state is passed into gutter rather than repeatedly asking `_hasActiveFolds` during the visible-line loop.

This is important for large files with many diagnostics or large diagnostic ranges.

### 8.21 Correct offset-key cache pruning

Commit:

```text
050ddfd9 perf(editor): prune offset caches by offset range
```

A structural bug was found in cache pruning:

```text
_bracketCache
_caretInfoCache
```

use document scalar **offsets** as keys, but the generic viewport prune code treated their keys as **line numbers**.

Once those maps exceeded the pruning threshold, most useful entries could be discarded simply because their character offsets were numerically outside the line-number keep range.

The new logic:

1. Computes the same viewport +/-400 line keep range.
2. Converts its first/last line boundaries to document scalar offsets.
3. Prunes `_bracketCache` and `_caretInfoCache` using those offsets.
4. Leaves true line-keyed caches on the existing line-number pruning path.

This preserves cache locality near the viewport rather than repeatedly throwing away useful caret/bracket calculations.

## 9. Current paint flow after the completed work

The relevant large-file render structure currently looks conceptually like:

```text
_checkDocumentVersionAndClearCache()
  -> derive visible viewport
  -> _pruneViewportCaches(firstVisibleLine, lastVisibleLine)
  -> _scheduleVisibleSemanticTokens(...)
  -> syntax pre-highlight only when not large-file mode
  -> search highlights
  -> line decorations
  -> transient line highlight
  -> folded line highlights
  -> document highlights if non-empty
  -> selection
  -> indent guides if enabled
  -> paint visible text lines
  -> diagnostics
  -> document colors if non-empty
  -> inlay hints if visible/non-empty
  -> ghost/AI text if present
  -> virtual removed lines if present
  -> gutter
  -> bracket highlight when focused
  -> IME composition
  -> caret/etc.
```

The central visible line loop is already viewport-bounded and caches line text / paragraphs.

## 10. Known reachable candidates for the next optimization batches

The next conversation should **start here**, not re-audit all completed work.

### 10.1 Search highlights

`_drawSearchHighlights(...)` currently returns quickly when `controller.searchHighlights` is empty, which is good.

When non-empty, it iterates the global highlights collection and maps each highlight's offsets back to lines before rejecting off-screen ranges.

Investigate:

- typical size and lifecycle of `searchHighlights`
- whether Alera's search can produce hundreds/thousands of matches for large files
- whether highlights are rebuilt or sorted elsewhere
- whether a line/offset indexed representation already exists in controller/search code

Potential direction if profiling/reachability supports it:

```text
sorted highlight ranges by start offset/line
  -> binary search first viewport candidate
  -> iterate until start > viewport end
```

Do not add a complex index if Alera only ever stores a small bounded result set.

### 10.2 Line decorations

`_drawLineDecorations(...)` returns quickly if empty, but when non-empty it scans every decoration and for each visible-overlapping decoration loops the visible line range.

This can become:

```text
O(number_of_decorations * visible_lines)
```

Investigate actual Alera call sites / collection size before optimizing.

Potential direction:

- sort/index decorations by line range
- first reject whole viewport using range index
- render only overlapping decorations
- avoid rebuilding `Paint` per line/decorator if stable

### 10.3 Diagnostics viewport indexing

Two per-paint costs were already removed:

- sorting
- whole-file range expansion in gutter

However `_drawDiagnostics()` still iterates `_sortedDiagnostics` and rejects diagnostics whose ranges are outside the viewport.

If projects can produce very large diagnostic lists, consider maintaining an index keyed by line or sorted start line and finding the visible subset directly.

Do not blindly index unless the collection size can realistically make the scan material.

### 10.4 Bracket highlight

Current bracket highlighting is only called while the editor has focus.

`_getBracketPairAtCursor()` already uses rope-local one-character reads and `_findMatchingBracket()` delegates matching into Rust/native fold logic, with a cache.

After `050ddfd9`, the cache is pruned using the correct offset unit.

Do not rewrite bracket matching unless profiling shows it still matters. A possible cheap improvement is replacing remaining `containsKey + []` lookups with single lookups where null-cache semantics permit it, but note `_bracketCache` intentionally stores nullable values, so `containsKey` is semantically meaningful and cannot simply be replaced by `cache[pos] ?? ...`.

### 10.5 Gutter paragraph/cache follow-up

A bounded `_lineNumberParagraphCache` now exists. Verify over real scrolling that:

- inactive line numbers reuse cached paragraphs
- active/current line style churn only invalidates the expected one/few lines
- line number cache stays bounded by viewport pruning

If cache hit rate is poor because `TextStyle` equality/identity changes every frame, move the four derived styles to longer-lived cached fields keyed by relevant theme/style inputs. Do this only after measuring or inspecting equality behavior.

### 10.6 Paint object allocation

Several helpers still construct `Paint()` objects during paint. This is often cheap relative to paragraph/layout work, but after larger costs are removed, profiles may show measurable allocation/GC churn.

Potential follow-up:

- reuse stable `Paint` instances when color/style inputs are stable
- do not over-engineer this before verifying it is hot

## 11. Paths already investigated and intentionally deprioritized

### 11.1 Virtual removed blocks

`_findVisibleLineByYPosition()` can become O(n) when virtual removed blocks are present.

However Alera's `lib/` and `test/` had no references to `virtualRemovedBlocks` / `VirtualRemovedBlock` during this audit. This appears to be dormant CodeForge capability for normal Alera workspace editing.

Do not optimize this path unless new reachability evidence appears.

Potential correctness nuance also exists around `firstVisibleLineY` with virtual removed blocks; do not "fix" it speculatively as part of performance work.

### 11.2 Semantic tokens

`paint()` calls `_scheduleVisibleSemanticTokens(...)`, but the method returns immediately when `lspConfig == null` or `filePath == null`.

Current `WorkspaceEditorSurface` does not pass an LSP config in the normal path examined, so semantic token scheduling is not a current large-file hotspot.

Keep the viewport-based design, but do not optimize dormant behavior first.

### 11.3 Fold-specific layout

Large-file profile disables folding, so wrapped/folded layout loops are not part of the normal large-file hot path.

Do not optimize fold-only code under the assumption it helps the current large-file case.

### 11.4 GPU rendering

Flutter is already GPU-composited. The observed large-file issues are dominated by Dart/native text layout, object allocation, document materialization, parsing/highlighting, and data movement.

Do not treat this as "GPU acceleration is disabled" unless profiling proves a raster/compositor bottleneck.

## 12. Important correctness constraints

### 12.1 Scalar offset vs UTF-16 offset

CodeForge rope offsets are scalar-based:

```dart
int get length => _rope.lenChars().toInt();
```

Dart `String` / Flutter text APIs often use UTF-16 indices.

Use existing conversion helpers such as:

```text
CodeForgeController.scalarToStringIndex(...)
CodeForgeController.scalarToUtf16Offset(...)
```

Do not compare or slice these coordinate spaces casually.

### 12.2 Nullable cache values

`_bracketCache` is:

```dart
Map<int, int?>
```

A cached `null` means "we already checked and there is no matching bracket". Therefore `containsKey` has semantic value and cannot always be collapsed to a nullable lookup.

### 12.3 Fold state

`enableFolding = false` does not necessarily erase all existing fold range state.

Do not clear fold structures merely for performance without checking transition-back behavior.

### 12.4 Large-file fallback behavior

Plain text / no-highlighting is currently deliberate protection for editing latency.

Do not re-enable highlighter work for large files until an incremental viewport-bounded path is ready and validated.

### 12.5 Main branch divergence

The local main branch is intentionally far ahead/behind `origin/main`.

This task must not rebase, merge remote main, reset, or otherwise resolve that divergence unless the user explicitly asks.

## 13. Validation commands and known environment behavior

Use focused validation after each CodeForge editor batch:

```text
dart format third_party/code_forge/lib/code_forge/code_area.dart
dart analyze third_party/code_forge/lib/code_forge/code_area.dart
flutter test test/widget/workspace_editor_surface_test.dart
git diff --check
```

Expected widget suite result at the current baseline:

```text
14/14 passed
```

Important execution notes:

- Run Flutter tests sequentially on Windows. Parallel runs can race native cleanup/build artifacts.
- Fresh CodeForge/native builds can take several minutes. If a command is already running/detached, reattach/poll the same operation instead of restarting it.
- Alera full-project analyzer can hang or report unrelated existing issues; use the focused `dart analyze` path for these vendor-only editor batches unless a broader change requires more.
- `rg` exit code 1 with empty output means no match; do not treat that as infrastructure failure.
- `git diff --check` must pass before commit.
- Formal validation is only successful when the command actually exits with code 0.

## 14. Git integration procedure

For every small batch:

1. Confirm worktree status and current HEAD.
2. Modify only the intended file(s).
3. Format/analyze/test/diff-check.
4. Read worktree Git status again.
5. Commit exact paths with guarded expected HEAD/fingerprint.
6. Read `main` status/HEAD again.
7. If main only has the expected unrelated workspace artifacts, cherry-pick the verified worktree commit.
8. Do not push.
9. Continue to next batch automatically.

If main has acquired real tracked modifications from another task, do not overwrite them. Re-evaluate integration safely.

## 15. Recommended next execution sequence

The next conversation should begin with the following concrete sequence.

### Batch 1 - reachable collection scans

1. Read worktree status/HEAD.
2. Inspect exact implementations and all Alera call sites for:
   - `controller.searchHighlights`
   - `controller.lineDecorations`
   - `_diagnostics`
3. Determine realistic maximum collection sizes and whether they are sorted/indexed already.
4. If a collection can be large, add the smallest useful viewport-bounded lookup/index.
5. Avoid changes for collections that are demonstrably small/bounded.
6. Validate, commit, cherry-pick.

### Batch 2 - gutter/cache effectiveness

1. Inspect `_lineNumberParagraphCache` hit behavior structurally or with a lightweight benchmark/profile.
2. If styles are recreated in a way that defeats equality/cache reuse, stabilize style caching.
3. Consider caching only the stable inactive style/paragraphs and let current/error/warning variants remain sparse.
4. Validate, commit, cherry-pick.

### Batch 3 - profiling checkpoint

Before deeper rewrites, capture a reproducible large-file profile covering:

- fast vertical scrolling
- horizontal scrolling across long lines
- caret movement
- Ctrl+Left / Ctrl+Right
- selection dragging
- typing bursts
- search with many matches
- diagnostics-heavy file if available

Record:

- UI isolate CPU
- raster time
- allocation/GC pressure
- frame jank
- document snapshot frequency
- paragraph construction count if observable

This is the point where the remaining bottleneck should determine whether more Dart cleanup is worthwhile or Phase B should start.

### Batch 4 - incremental parser design spike

Do not implement a giant parser migration in one batch.

First define a minimal stateful API, conceptually:

```text
open_editor_document(document_id, text/version)
apply_editor_edits(document_id, version, edits)
query_syntax_spans(document_id, version, start_line, end_line)
close_editor_document(document_id)
```

Requirements:

- native parser state remains alive across edits
- edits use bounded deltas
- query uses viewport + overscan
- response contains document/parser version
- stale responses are ignored
- no full-document spans returned on scroll/edit
- instrumentation measures parse/update/query/FFI time separately

Only then choose parser implementation/tree-sitter integration details.

## 16. Suggested incremental parser response shape

Keep the first version intentionally small:

```text
SyntaxViewportResult
  document_id
  document_version
  parser_version
  start_line
  end_line
  spans[]

SyntaxSpan
  line
  start_column
  end_column
  token_kind
```

Possible later additions:

- changed ranges
- fold ranges
- symbol outline data
- bracket metadata

Do not overload the first implementation with all features.

## 17. Suggested stale-result protection

Every async parse/query response must be checked against current document version.

Conceptually:

```text
request version N
  -> user edits to N+1
  -> response for N arrives
  -> discard response
```

This is essential because syntax work must never block or visually overwrite newer edits.

## 18. Performance success criteria

The large-file work should be considered successful only when cost remains approximately viewport/change-bounded as file size increases.

Target properties:

- scrolling does not scan from line 0 to viewport in normal large-file mode
- typing does not materialize the entire rope on each key event
- cursor/selection-only notifications do not create full document snapshots
- word movement/deletion operates on local line/buffer data
- IME projection remains bounded
- syntax work is incremental and viewport-first
- caches stay bounded without destroying useful locality
- diagnostics/search/decorations do not scale linearly with the entire global collection every frame when those collections can be large
- Rust -> Dart payload size scales with requested viewport/changes, not document size

## 19. What not to do next

Do not:

- replace CodeForge wholesale before proving the current renderer cannot meet targets
- migrate the terminal core while this editor task is active
- re-enable syntax highlighting for large files using the current full-document path
- optimize virtual removed blocks without evidence Alera uses them
- rewrite every `Paint()` allocation before measuring
- introduce large whole-document Rust snapshots
- treat Flutter GPU composition as the root problem without raster evidence
- change user-visible editor semantics merely to gain speed without explicit product approval
- modify `origin/main` divergence
- push automatically

## 20. Known test/environment details

Ignored local dependencies in the worktree have previously been populated and should not appear as tracked changes:

```text
third_party/xterm
third_party/dart_terminal
```

Known revisions during this work:

```text
third_party/xterm: 0c43af05aac9613b44c7d88e70f83cd1a8133591
third_party/dart_terminal: 7bfb6c57f8da75742891ac27b568cc651f77501a
```

Do not accidentally stage these or worktree metadata.

## 21. Session/history continuity

Workspace history session for this optimization thread:

```text
docs/history-session/39.md
```

Stable session key used by the current conversation:

```text
v1/4FxJBkgCfS8Uo7pKW5X6ZEnXweuARgk58DpHTdxYPu6KtOrc6sShwaGCWLz7wQWQULzJuUIyFMkJ
```

When using the Coding Tools MCP history checkpoint in the continuation, keep the returned session key/path stable for that conversation and checkpoint after meaningful completed batches.

The Markdown handoff you are reading is the durable project-side source of truth for the next conversation; history files are supporting execution context.

## 22. Ready-to-use prompt for the next conversation

Use this as the opening instruction in the new chat:

> Continue Alera large-file editor performance optimization using `docs/editor-large-file-performance-handoff.md` as the source of truth. Use `@home-node`, work in `.worktrees/codeforge-large-scroll-fastpath`, follow small verified batches, commit each successful batch, cherry-pick to `main`, preserve unrelated changes, do not push, and continue automatically without asking after every batch. Start from the "Recommended next execution sequence" and first inspect reachable `searchHighlights`, `lineDecorations`, and diagnostics collection scans before deciding whether an index is justified.

## 23. Immediate expected starting state

The continuation should expect the following relevant code baseline to already be present on `main`:

```text
050ddfd9 perf(editor): prune offset caches by offset range
f96c106e perf(editor): reduce gutter paint overhead
958605c5 perf(editor): avoid per-paint prune closures
3fd3edbc perf(editor): reduce per-line paint overhead
ced46ed5 perf(editor): bypass large-file line offset cache
```

Before modifying anything, verify the actual HEAD/status rather than assuming no concurrent work occurred after this document was written.

---

End of handoff.
