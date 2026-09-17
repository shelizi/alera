# NativeEditorDocument Phase 1 (work package C)

Date: 2026-09-17
Base: `354f309b` (`perf(editor): add large-file profiling gate`)
Scope: CodeForge FRB retained incremental parser / viewport syntax spans

## Result

Work package C establishes the Phase 1 native boundary requested by `docs/rust-heavy-operation-parallel-work-plan.md`.

`NativeEditorDocument` now retains:

- document id and monotonically increasing revision;
- a Ropey document snapshot owned by the native document;
- Tree-sitter `Parser` and retained `Tree` for supported languages;
- a compiled syntax-highlight query;
- line / byte / Unicode-scalar offset mapping through Ropey;
- explicit close/release state.

The initial document text crosses FFI once at open. Subsequent updates use edit deltas. Syntax queries return only the requested viewport plus fixed overscan and carry the document revision so Dart can reject stale work.

## API

Generated Dart bindings expose:

```text
NativeEditorDocument.open(documentId, revision, text, languageId)
NativeEditorDocument.applyEdits(expectedRevision, newRevision, edits)
NativeEditorDocument.querySyntaxSpans(expectedRevision, startLine, endLine, overscan)
NativeEditorDocument.info()
NativeEditorDocument.close()
```

Edit offsets and returned columns are Unicode-scalar offsets, matching the existing CodeForge Rope contract rather than Dart UTF-16 indices.

`applyEdits` requires both:

- `expectedRevision == current native revision`; and
- `newRevision > expectedRevision`.

Stale or non-monotonic edits are rejected without mutating native state. Stale syntax queries return `stale=true` and no spans.

## Parser implementation

This does **not** implement a custom parser. It uses Tree-sitter and maintained grammar crates.

Phase 1 native grammar support:

- Dart
- Rust
- JavaScript / JSX alias
- TypeScript
- TSX
- Python
- JSON / JSONC alias

Unsupported languages retain the document state but return `supported=false`, allowing the existing Dart `re_highlight` path to remain the compatibility fallback.

Highlight captures are projected to the existing Highlight.js-like CodeForge theme keys (`keyword`, `string`, `comment`, `function`, `type`, `variable`, `attr`, `tag`, `literal`, `operator`, etc.). No document-sized syntax-span collection is created or transferred.

## Correctness / lifetime coverage

Focused Rust tests cover:

- viewport + overscan range bounding;
- incremental edit + revision advance;
- stale edit rejection;
- stale query rejection;
- equal/backward revision rejection;
- Unicode scalar edit correctness using astral emoji;
- unsupported-language fallback;
- explicit close releasing retained Rope/parser/tree/query state.

The performance benchmark is intentionally ignored during ordinary unit tests and can be reproduced with:

```bash
cargo test benchmark_incremental_edit_and_viewport_query_scaling -- --ignored --nocapture
```

## Five-sample performance evidence

Synthetic Rust source was measured at 2k, 20k, and 100k logical lines in the normal debug test profile. Each size used five fresh retained documents. `open` is the full initial parse baseline; `edit` inserts one scalar near the end and incrementally reparses from the retained edited tree; `viewport query` asks for the last 41 lines plus 20 lines of overscan.

| Lines | open median | incremental edit median | viewport query median |
| ---: | ---: | ---: | ---: |
| 2,000 | 91.3 ms | 3.31 ms | 10.9 ms |
| 20,000 | 412.0 ms | 33.9 ms | 9.69 ms |
| 100,000 | 1,804.3 ms | 177.6 ms | 12.3 ms |

Raw samples (microseconds):

```text
2,000
open  [91521, 91341, 88887, 97289, 89400]
edit  [3623, 3588, 3270, 3301, 3308]
query [10588, 11160, 10237, 11682, 10927]

20,000
open  [405394, 427608, 412008, 421888, 408304]
edit  [35012, 33856, 33301, 36708, 33478]
query [9805, 10276, 9440, 9531, 9688]

100,000
open  [1829438, 1802637, 1809449, 1804313, 1799596]
edit  [175588, 177571, 176435, 178405, 177595]
query [13300, 12123, 12304, 12036, 12572]
```

### Interpretation

The viewport query satisfies the important bounded-query property: the 100k-line document is in the same ~10-13 ms band as the 2k-line document and transfers only viewport captures.

The retained incremental parse is substantially cheaper than a new full parse (about 3.6% of full-open time at 2k lines, 8.2% at 20k, and 9.8% at 100k in this synthetic debug run), but it is **not constant-time with file size**. Tree-sitter still incurs syntax-tree traversal/reconciliation cost as the retained tree becomes very large. This is an explicit Phase 1 limitation, not hidden as an O(changed-region) claim.

Because parsing runs through the asynchronous FRB API rather than the Flutter UI isolate, this establishes the native architecture and stale-safety required by C without reintroducing UI-isolate whole-document syntax work.

## Why Phase 1 does not replace the production renderer yet

The current CodeForge controller has an active-line edit buffer. In large-file mode the renderer deliberately avoids materializing `controller.text` and therefore does not currently have a reliable `(old scalar range, replacement)` stream for every buffered edit. Wiring the native parser directly in `code_area.dart` would either:

1. force a new full-document snapshot during editing, regressing the B large-file work; or
2. let the native retained snapshot temporarily diverge from the controller buffer.

Neither is acceptable.

Therefore C stops at the stable native FRB boundary plus generated Dart bindings. The next integration batch should expose committed scalar edit deltas from the controller/Rope mutation boundary (or coordinate the retained native document with the Rope handle), then make `SyntaxHighlighter.preHighlightLines` issue one viewport query per paint scheduling window. Unsupported/stale/error cases should continue to fall back to `re_highlight`.

This is deliberately safer than inserting a second full-text snapshot into the renderer just to demonstrate the new API.

## Follow-up integration contract

When enabling native highlighting in production:

1. create/open the native document once from an already-available initial document payload, not from paint;
2. serialize committed scalar edit deltas and native revisions;
3. never reconstruct deleted text by taking a whole-document snapshot in large-file mode;
4. request syntax spans once per viewport + overscan window, not per line;
5. cache spans by native revision and line;
6. reject responses whose revision no longer matches the controller;
7. merge LSP semantic-token styling after grammar spans as today;
8. close native state on document/controller disposal;
9. keep `re_highlight` as fallback for unsupported languages or native failures.

Potential later phases may reuse the same retained parse tree for folding, bracket matching, structural selection, or symbols, but those are intentionally excluded from Phase 1.

## Dependency / generated-surface notes

New CodeForge Rust dependencies are Tree-sitter plus the seven grammar crates. `tree-sitter 0.26.13` requires `regex ^1.11.3`, so Cargo resolves the previously loose `regex = "1.10.2"` dependency to a newer compatible 1.x release in `Cargo.lock`. This is required by Tree-sitter rather than an unrelated lockfile refresh.

Bindings were regenerated from `third_party/code_forge/flutter_rust_bridge.yaml` with `flutter_rust_bridge_codegen 2.13.0`; generated files were not hand-edited.
