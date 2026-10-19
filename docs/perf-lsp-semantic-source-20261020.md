# Alera semantic-source and LSP no-op synchronization optimization

Date: 2026-10-20
Branch: local main

## Scope and behavior

The editor sends language-server source text from the current CodeForge
document snapshot. Previously every changed source (including files with no
Tab expansion) split raw text, original display text and current display
text into complete line arrays before building the outgoing text. Source
conversion cost grew with all file lines and repeated after the sync debounce.

In this batch:

- If current display content is unchanged, return original raw content as
  before (preserves original Tabs and CRLF).
- If original raw content equals original display content, return current
  text directly. There is nothing to restore from the original snapshot,
  so avoid three full file line lists and StringBuffer rebuilding.
- When raw and display differ (Tab or other transformations), keep the
  existing unchanged-line raw restoration path byte-for-byte.
- The LSP adapter skips redundant textDocument/didChange when new content
  equals its last sent text, rather than serializing the entire JSON text
  and advancing the document version needlessly.
- Changed content still sends the same full-text didChange payload; this
  does not claim support for LSP incremental range updates until server
  textDocumentSync capability negotiation has been implemented/validated.

## Old/new reference benchmark

From repository root:

    flutter test --no-pub benchmark/editor_semantic_sync_benchmark.dart

Seven alternating warm runs of exact previous full-text Dart conversion and
new conversion, checking identical output for each case. Medians are pure Dart
conversion CPU microseconds; no FFI, server process, editor text extraction
cost, LSP JSON serialization or network/stdio transfer is measured.

| Mode | Lines | Old (us) | New (us) | Speedup |
| --- | ---: | ---: | ---: | ---: |
| No Tabs, raw/display equal by value | 10,000 | 1,863 | 16 | 116.4x |
| No Tabs, raw/display equal by value | 40,000 | 14,249 | 94 | 151.6x |
| Original Tabs to preserve | 10,000 | 1,539 | 1,259 | approx. 1.2x (noise) |
| Original Tabs to preserve | 40,000 | 10,883 | 7,693 | approx. 1.4x (noise) |

The Tab-sensitive case intentionally keeps the original algorithm. Do not
extrapolate microbench speedups to complete LSP latency or frame rate.

## Regression validation

- Editor source tests cover no-transform identity reuse, original Tab
  preservation around changed lines, fallback for source/display disagreement,
  cursor mapping and Unicode cases.
- Adapter tests cover no-op same-text replacements before and after a real
  change, version stays at 1 until content changes and next event uses 2,
  didSave still sends latest text; existing multi-document and navigation
  tests are retained.
- LSP manager unit suite verifies unchanged document transport paths.
- No FRB generated code, Rust types or LSP protocol data shapes changed.

## Future gate

Do not send textDocument/didChange range payloads indiscriminately.
The server's declared textDocumentSync.change capability must be known
(full vs incremental). A future native Rope delta bridge needs explicit
UTF-16 range conversion, document version ordering, failure fallback and
real-server integration tests. Explorer native snapshot/delta is separate.
