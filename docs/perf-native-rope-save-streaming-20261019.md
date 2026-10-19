# Native Rope Save edited-document streaming optimization (2026-10-19)

## Scope and compatibility

Alera Native Rope Save remains **opt-in** (ALERA_NATIVE_ROPE_SAVE).
This optimizes the Rust implementation shared by ordinary text-file
extensions (Rust, Go, Python, TypeScript/JavaScript, Dart, PHP, etc.).
No per-language special cases or FFI interface changes.

Previous path: every edited Save created a full Rope String, split raw,
expanded and current content into line-segment vectors, reconstructed
another String, then encoded to UTF-8/UTF-16 output bytes.

New path:
- With no Tab expansion required, copy UTF-8 directly from Rope chunks to
  the output byte vector, optionally prefixing its UTF-8 BOM.
- UTF-16 LE/BE encodes Unicode scalars from Rope chunks directly and retains
  proper BOM and surrogate pair behavior.
- Legacy codecs still use encoding_rs but avoid line splitting and
  reconstruction when no tabs were expanded.
- The on-disk Save path borrows the decoded display text rather than
  allocating a second full-size display String when the source has no tabs.
- Documents with Tab expansion continue to use the original preserve-tabs
  algorithm (including untouched Tab and CRLF preservation).
- The previous no-op Save behavior, expected conflict token, Windows
  ReplaceFileW ACL and protected-path guards, and active LSP fallback
  remain unchanged.

## Release Rust encoding microbenchmark

Run: cargo test --release --manifest-path third_party/code_forge/rust/Cargo.toml profile_snapshot_encoding_mib --lib -- --ignored --nocapture

Deterministic 1/4/10 MiB no-tab documents with many short lines. Compare an
exact reference copy of the previous snapshot encoding function against the
new implementation. Byte parity checked before timing. Seven alternating
post-warmup timing batches; figures are median microseconds per call.

| Size | Encoding | Previous (us) | Optimized (us) | Speedup |
| --- | --- | ---: | ---: | ---: |
| 1 MiB | UTF-8 | 12,024.9 | 273.4 | 44.0x |
| 1 MiB | UTF-16 LE | 14,093.3 | 1,335.6 | 10.6x |
| 4 MiB | UTF-8 | 49,330.6 | 1,090.2 | 45.3x |
| 4 MiB | UTF-16 LE | 57,136.2 | 5,592.5 | 10.2x |
| 10 MiB | UTF-8 | 159,529.9 | 3,198.6 | 49.9x |
| 10 MiB | UTF-16 LE | 172,087.9 | 14,077.1 | 12.2x |

This is a synthetic newline-dense native CPU encoding benchmark, NOT the
end-to-end Save time. It excludes file I/O, FFI, Flutter, heap and UI frames.
It is not evidence that real-world Save is 10-50x faster. Files with actual
Tab indentation still take the preservation fallback.

## Full native save with disk replacement (Release)

Run: cargo test --release --manifest-path third_party/code_forge/rust/Cargo.toml profile_native_rope_save_sizes --lib -- --ignored --nocapture

Windows disk write, medians from five samples, fixture resets excluded:

| File size | Current double-read (us) | Current single-read (us) |
| --- | ---: | ---: |
| 1 MiB | 18,450 | 13,783 |
| 4 MiB | 29,867 | 19,224 |
| 10 MiB | 60,307 | 31,851 |

**Both variants use the newly optimized encoder.** This table measures
single-read vs double-read on the current code, not the effect of this
commit on total disk Save latency. Five samples are not a p95 guarantee.

## Verification

- Full CodeForge library suite: 60 passed, 0 failed, 3 ignored, including
  two separate manually invoked benchmarks.
- Byte-parity tests across all ten codecs, CR/CRLF, BOM, tabs, no tabs,
  empty contents, Unicode supplementary scalars and multi-chunk Rope.
- Real disk edited-save tests for multiple extensions and all supported
  encodings, validating exact bytes and returned content tokens.
- Rebuilt Windows Release CodeForge DLL and ran Flutter
  test/unit/native_rope_save_ffi_test.dart through actual FFI: 1 passed,
  0 failed. This checks text preservation and external-edit conflict flows.

## Remaining gates

Keep Native Save opt-in until full Windows Desktop end-to-end modified-Save
p50/p95, frame impact, heap peak, active language-service behavior,
real-world source distributions, and physical remote NAS/ACL/disconnect
failure cases are validated. The Release DLL built here was for testing;
the complete desktop application was not rebuilt or deployed.
