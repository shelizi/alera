# Native Rope save migration

Status: design only, production save path unchanged.

The editor now opens a workspace file directly into CodeForge Rust Rope, but
Save and autosave still copy the full Rope text to Dart, then into the separate
root Rust library. The writer rereads the original bytes to preserve untouched
tab formatting and returns both raw/display text back to Dart.

Implementation direction: add a CodeForge RopeBridge save call that owns the
native text snapshot and writes the workspace file directly. Do not transfer
opaque Rust pointers between Alera and CodeForge dynamic libraries. Return
metadata only (token, size, encoding and revision). Retain the full-text writer
as a compatibility fallback.

Correctness requirements:
- Validate workspace root, canonical target and protected path semantics.
- Preserve expected content token and explicit overwrite-conflict behavior.
- Preserve original tab layout on unchanged lines and legacy text encodings.
- Ensure concurrent edits do not mark a newer editor revision as saved.
- Use guarded atomic replacement and preserve Windows error behavior.
- Keep LSP compatibility without forcing text snapshots on ordinary Save.

Before production cutover: Rust golden tests for UTF-8/UTF-16/BOM/Big5/GBK,
external-change conflicts, symlinks, interrupted writes, and edit-while-saving.
Windows benchmark must measure 1/4/10 MiB files, five samples, median/p95
latency, FRB bytes, heap allocation and UI stalls. Enable new path only after
parity and real end-to-end evidence.
