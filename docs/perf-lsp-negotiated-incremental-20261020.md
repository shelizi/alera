# Alera negotiated incremental LSP didChange

Date: 2026-10-20
Scope: CodeForge LSP client initialization + Alera semantic provider adapter.
Branch: local main

## Protocol

After LSP initialize, retain the server's textDocumentSync change kind:
- LSP kind 2 (incremental): send range-based didChange with UTF-16 positions
- LSP kind 1 (full) or omitted/unknown value: retain full-document didChange
- LSP kind 0 (none): do not send textDocument/didChange
- For ccls, existing forceFullDocumentSync overrides kind 2 to kind 1

Interpret either numeric textDocumentSync or options object with change property;
invalid/missing capabilities never enable incremental ranges. CodeForge uses
UTF-16 by default (does not negotiate UTF-8 positionEncodings).

## Correctness and ordering

- One bounded incremental replacement spans the common UTF-16 prefix/suffix,
  expanding the changed span if needed to avoid splitting surrogate pairs
  (emoji) or CRLF line endings.
- Changed range start/end use zero-based UTF-16 line/character positions.
  Distinct CRLF, LF and CR line breaks are accounted for.
- Entire-document replacements keep full-text changes.
- Per-document Future send tails serialize consecutive text changes, so each
  patch applies to the last successfully sent text and versions increase
  monotonically. didSave and didClose await prior sends.
- A failed stdio notification leaves the local prior text untouched, and the
  next attempt uses full text instead of chaining a range edit from a
  possibly-missing prior patch.
- Missing/malformed server capabilities retain previous full-text behavior.

## Verification

Unit tests cover:
- CodeForge initialization capability decoding for numeric/object, unknown,
  invalid, none and ccls forced-full cases
- UTF-16 code positions around surrogate pairs, CRLF insertion and deletion
- Full-only and unknown server fallback
- 40,000-line file with a one-word change sends just the changed span
- Concurrent edits preserve strict v2/v3 order with a blocked flush
- Same-text no-op, no-sync behavior, failed transport full-text fallback,
  multi-document document opens, saves, semantic navigation and manager restart

Run:

    flutter test --no-pub test/unit/code_forge_lsp_sync_kind_test.dart test/unit/code_forge_semantic_provider_adapter_test.dart test/unit/language_intelligence_manager_test.dart

In this session the above suite reported 30 passed, 0 failed.

## Performance and limitations

A tiny edit in a 40k-line document reduces transmitted didChange TEXT payload
from entire document to a few characters. This is a payload-size test, not a
measurement of actual language server parsing, Flutter frames or end-to-end
response times. Computing the common prefix/suffix still scans O(file length)
Dart UTF-16 units. A future Rope edit-delta API can avoid this scanning if
profiles justify changing the editor/native boundary.

The synthetic transport fixtures cover protocol behavior. No real Rust
Analyzer, Go language server, or PHP process end-to-end latency measurement
was performed in this batch. Protocol fallback remains enabled.

Other uncommitted Explorer native snapshot/delta files are not part of this
commit; FRB bindings must be rebuilt and their separate tests must pass
before adopting the new Explorer API in a release.
