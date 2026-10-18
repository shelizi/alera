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

1. Windows GUI smoke passed for manual Save, Autosave, conflict cancel/overwrite and active Dart LSP fallback; additional language servers and the full encoding UI matrix remain.
2. Release Rust-only A/B and Dart-to-Release-DLL FFI snapshot measurements are available; full Desktop p50/p95, frame timing, Dart heap and bytes crossing FFI remain unmeasured.
3. Localhost SMB/UNC native Save smoke passed; real remote NAS/disconnect, network ACL changes, and backup/restore failure cases are pending.
4. Ghostty source build works with Scoop Zig 0.16.0: 45 Diff widget, 41 editor/autosave/occurrence and 1 Windows FFI smoke test passed. Standalone Desktop GUI smoke covers manual Save, Autosave and conflict flows; full Release gating remains.

## Flutter FFI / Release CodeForge DLL snapshot (Windows, 21 samples)

This opt-in benchmark runs Flutter/Dart FFI against the optimized CodeForge DLL, measuring the native snapshot Save roundtrip including disk I/O. It does not include Flutter UI frames, LSP, Dart heap peaks, or an old-path full-app Save comparison.

| File size | Native Save p50 | p95 |
|---|---:|---:|
| 1 MiB | ~15.2 ms | ~17.3 ms |
| 4 MiB | ~28.4 ms | ~29.7 ms |
| 10 MiB | ~108.9 ms | ~191.9 ms |

These values are from an earlier 21-sample pass. A later full-log run completed all 1/4/10 MiB samples, including 10 MiB 55.039 / 108.588 ms (p50/p95). The large-file tail varies significantly; neither run is a stable full-Desktop release benchmark.

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
| Active Dart LSP, edit and Save | Pass: Running / 1 active document and status LSP 1; saved file retained 2 Tab bytes, 2 CRLF pairs and GUI marker |

Caveats: This is a manual Debug Desktop UI smoke run, not a complete Release UI performance benchmark. Native-save opt-in remains default-off. The repeat Save interaction is sensitive to physical display coordinates: automation must use `desktop_displays` native dimensions, not resized screenshot pixels. An earlier ambiguous retry was cleared by a fresh conflict and explicit overwrite on the correct display coordinates.

### Outstanding verification

- Active Dart LSP fallback passed GUI Save byte checks; other providers, lifecycle and failure scenarios still need testing.
- Full Release Desktop p50/p95, frame times, Dart heap, and bytes crossing FFI.
- Network-share and file-permission failure cases. A Runtime Host suite had one timeout-sensitive failure when run in a group; the exact timeout test passed alone and should be rerun before release.

## 2026-10-18 explicit encoding and active Dart LSP validation

- CodeForge was eagerly invoking charset auto-detection even when the user explicitly selected a legacy codec. Some valid Big5/GBK sequences were rejected. The fix honors the selected codec and runs auto-detection only when none is supplied.
- Rust on-disk Save tests cover UTF-16 LE/BE, Big5, GBK, Shift-JIS, EUC-JP, EUC-KR and Windows-1252: byte-identical no-op, edited text preserving untouched Tab/CRLF/BOM and unrepresentable-character rejection. Full CodeForge suite: **56 passed, 0 failed, 2 ignored**.
- The Release CodeForge DLL was rebuilt. A real Dart/Flutter FFI regression passed UTF-16 LE/BE, Big5 and Windows-1252, including byte-identical no-op and append.
- In isolated Windows Desktop Alera, Dart Language Intelligence was enabled and settings showed **Running / 1 active document**, status bar **LSP 1**. A real Dart fixture with two Tab-indented CRLF lines was edited and saved; disk retained both original Tab bytes (09), both CRLF pairs (0D 0A), and the new GUI marker. Thus the active Dart LSP fallback path is exercised with actual bytes.
- The selection policy also has a regression: native Save requires opt-in, a native-backed document, and no active language-service document.

### Captured Release FFI rerun

The full Flutter test stdout was redirected to an ignored build log to avoid incomplete terminal projections. The process exited 0 and emitted all 1/4/10 MiB rows over 21 samples per size:

| Size | Native Save p50 | Native Save p95 | Rope-to-Dart text copy p50 |
|---|---:|---:|---:|
| 1 MiB | 14.214 ms | 16.322 ms | 2.724 ms |
| 4 MiB | 28.533 ms | 72.073 ms | 9.865 ms |
| 10 MiB | 55.039 ms | 108.588 ms | 23.589 ms |

Previous runs had higher 10 MiB timings (108.9 / 191.9 ms p50/p95). This remains directional: no full Flutter Desktop frame-time, memory-peak or old-path end-to-end speedup claim.

## Windows SMB/UNC loopback smoke (2026-10-18)

An opt-in Flutter-to-Release-CodeForge test mounted the existing local administrator share using a UNC path rooted inside the isolated test workspace. The test created only a temporary subdirectory and deleted it in finally.

- Native Rope open and Save succeeded across UNC/SMB and preserved unchanged Tab and CRLF bytes.
- An external modification triggered SaveStatus.conflict without overwriting disk bytes.
- Explicit overwrite saved the editor snapshot.
- Redirected Flutter test log reported 1 passed, 0 failed and exit code 0.
- No new share, OS permission change, production profile edit or user project operation was performed.

This verifies one loopback Windows SMB configuration, **not** a general remote NAS compatibility guarantee. Real remote file-server disconnect, ACL transitions, latency and backup failures remain open.
