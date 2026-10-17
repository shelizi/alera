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

## Release-optimized Rust-only A/B (Windows)

Command:

```sh
cargo test --release --manifest-path third_party/code_forge/rust/Cargo.toml profile_native_rope_save_sizes --lib -- --ignored --nocapture
```

Five alternated samples per size after warm-up, using the same Rust-only paths as the Debug table. Timing excludes fixture reset, Flutter UI, Dart text copies, FFI and LSP.

| File size | Previous double-read median | New single-read median | Previous max | New max |
|---|---:|---:|---:|---:|
| 1 MiB | 22.027 ms | 22.402 ms | 30.231 ms | 23.090 ms |
| 4 MiB | 40.326 ms | 33.244 ms | 43.465 ms | 35.703 ms |
| 10 MiB | 78.886 ms | 61.737 ms | 86.265 ms | 66.780 ms |

Only five samples were collected, so the reported maximum is not a reliable p95. Release data favors the single-read native path for 4 and 10 MiB, but not for 1 MiB. No claim of full app performance follows from these measurements.

Re-run:

```sh
cargo test --manifest-path third_party/code_forge/rust/Cargo.toml profile_native_rope_save_sizes --lib -- --ignored --nocapture
```

## Pending gates

1. Windows Desktop GUI smoke passed for manual Save, Autosave, conflict cancel and explicit overwrite; encoding matrix and proven active-LSP fallback remain.
2. Release Rust-only A/B and Dart-to-Release-DLL FFI snapshot measurements are available; full Desktop p50/p95, frame timing, Dart heap and bytes crossing FFI remain unmeasured.
3. Verify backup/restore behavior for network shares and Windows failure cases beyond locked local NTFS files.
4. Ghostty source build works with Scoop Zig 0.16.0: 45 Diff widget, 41 editor/autosave/occurrence and 1 Windows FFI smoke test passed. Standalone Desktop GUI smoke covers manual Save, Autosave and conflict flows; full Release gating remains.

## Flutter FFI / Release CodeForge DLL snapshot (Windows, 21 samples)

This opt-in benchmark runs Flutter/Dart FFI against the optimized CodeForge DLL, measuring the native snapshot Save roundtrip including disk I/O. It does not include Flutter UI frames, LSP, Dart heap peaks, or an old-path full-app Save comparison.

| File size | Native Save p50 | p95 |
|---|---:|---:|
| 1 MiB | ~15.2 ms | ~17.3 ms |
| 4 MiB | ~28.4 ms | ~29.7 ms |
| 10 MiB | ~108.9 ms | ~191.9 ms |

These values are from an earlier 21-sample pass. In a subsequent rerun, 1 MiB (15.520/17.050 ms) and 4 MiB (29.076/39.588 ms) appeared, but no 10 MiB metrics were emitted despite a zero command exit. Treat 10 MiB repeatability as **not yet proven** and investigate the benchmark harness before declaring a release improvement.

## Windows desktop GUI smoke with isolated runtime (2026-10-18)

The opt-in Debug build used `ALERA_NATIVE_ROPE_SAVE=true`, an absolute `ALERA_E2E_SUPPORT_DIR`, and `ALERA_E2E_DESKTOP=1`. The last option assigns a separate Windows App ID to avoid forwarding launch to the existing user app. The application support override covers SQLite, logs, updater scratch, and the Runtime Host client/sidecar root. The test app showed **0 production projects** before adding one isolated fixture, and created its own Runtime Host `runtime.sqlite`. No production profile or `main` source was modified.

A local Git fixture under `build/native-rope-e2e-workspace` contained `manual_save.txt`, `autosave.txt`, `conflict.txt`, and `lsp_fallback.dart`.

| Desktop action | Evidence / result |
|---|---|
| Open and Save via toolbar | Pass: `manual_save.txt` was written (40 bytes), with original tab (09) and CRLF (0D 0A) preserved |
| Autosave, 1s debounce, no manual Save | Pass: `autosave.txt` on disk contained the GUI marker |
| External edit while editor dirty, Save | Pass: conflict dialog appeared |
| Cancel conflict | Pass: external disk bytes remained untouched; unsaved GUI text was not written |
| Repeat external edit, explicit Overwrite | Pass: dialog reappeared; clicking Overwrite replaced external bytes with editor version |
| Dart syntax file open, edit, toolbar Save | Pass: `lsp_fallback.dart` on disk contained GUI marker; whether language server attachment was active is **not verified** |

Caveats: This is a manual Debug Desktop UI smoke run, not a complete Release UI performance benchmark. Native-save opt-in remains default-off. The repeat Save interaction is sensitive to physical display coordinates: automation must use `desktop_displays` native dimensions, not resized screenshot pixels. An earlier ambiguous retry was cleared by a fresh conflict and explicit overwrite on the correct display coordinates.

### Outstanding verification

- Confirm a real active language-server document with `_languageIntelligenceDocumentPath != null` and verify fallback code path, beyond simply saving a Dart file.
- Full Release Desktop p50/p95, frame times, Dart heap, and bytes crossing FFI.
- Network-share and file-permission failure cases. A Runtime Host suite had one timeout-sensitive failure when run in a group; the exact timeout test passed alone and should be rerun before release.
