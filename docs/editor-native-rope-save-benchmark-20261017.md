# Native Rope Save benchmark — 2026-10-17

## Status

Feature branch: `perf/rust-batch-diff-occurrences-20261017`.
Native Save remains **opt-in** via `ALERA_NATIVE_ROPE_SAVE`; production default is unchanged.

## Windows correctness

- `ReplaceFileW` is used on Windows with strict ACL/attribute merging (no ignore flags), preserving file creation time and explicit DACL entries.
- Windows tests verify creation time, `icacls` explicit ACE/SDDL, locked-file rollback and scratch cleanup, conflict and overwrite behavior.
- The subsequent save-path change reads the original document once, then requires the same disk token at replacement, even after user-confirmed overwrite.

## Directional Rust-only timings

Build: `cargo test` Debug (not Release). Files are in a temporary Windows directory.
Each 1/4/10 MiB size has five timed samples, alternating execution order after warm-up. Source-file restoration between samples is excluded from measurement. These samples include Rust decoding, text encoding and disk replacement; **they exclude Flutter, bridge transfers, LSP and UI events**.

| File size | Previous double-read Rust path (median) | Updated single-read Rust path (median) |
|---|---:|---:|
| 1 MiB | 73.1 ms | 66.3 ms |
| 4 MiB | 187.4 ms | 112.0 ms |
| 10 MiB | 2,008.0 ms | 868.8 ms |

Large-file variance is substantial (10 MiB old-path maximum 11,682.5 ms; new-path maximum 5,260.4 ms). This is **not sufficient evidence of a stable end-to-end speedup**. The direct native-only profile in a separate run measured 43.9 / 115.7 / 248.8 ms median for 1 / 4 / 10 MiB respectively, which also demonstrates environmental variance.

Re-run:

```sh
cargo test --manifest-path third_party/code_forge/rust/Cargo.toml profile_native_rope_save_sizes --lib -- --ignored --nocapture
```

## Pending gates

1. Real Windows Flutter desktop tests for opt-in manual Save, Autosave, conflict resolution, UTF-8/UTF-16/legacy encodings and LSP fallback.
2. Release-mode old-vs-new end-to-end p50/p95, UI frame timing, Dart heap and bytes crossing FFI.
3. Verify backup/restore behavior for network shares and Windows failure cases beyond locked local NTFS files.
4. Do not reuse the existing July 2026 Ghostty DLL without confirming ABI compatibility with the currently pinned later submodule revision; the available environment has no Zig in PATH.
