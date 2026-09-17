# Agent runtime overlay A2 validation

Date: 2026-09-17
Branch: `perf/agent-overlay-production-benchmark`
Scope: A2 from `docs/rust-heavy-operation-parallel-work-plan.md`

## Scope

A2 validates the existing native agent runtime overlay path. It does not move more production behavior to Rust and does not change overlay reconciliation semantics.

The validated path covers recursive mirror/copy/delete/link work, managed-file reconciliation, Flutter Rust Bridge result projection, Windows link behavior, and Dart launch-path isolation from recursive filesystem work.

## Validation additions

- Expanded Rust coverage for actual-platform repeated reconciliation and copy-fallback warnings.
- Replaced the previous small/large-only manual benchmark with a five-sample small/medium/large matrix covering deterministic link-success and forced copy-fallback paths.
- Added Windows Flutter integration coverage for native success, cleanup warning projection, and preparation error projection.
- Added a manual production FRB benchmark that records first and repeated-unchanged latency, UI-isolate heartbeat activity, maximum heartbeat gap, RSS delta, and actual linked/copied counts.
- Added a unit regression test that prevents the `agent_runtime_overlay_*` launch path from reintroducing recursive Dart filesystem operations.

No production overlay implementation was changed.

## Deterministic Rust benchmark

Command:

```text
cargo test --manifest-path rust/Cargo.toml benchmark_overlay_production_matrix --lib -- --ignored --nocapture --test-threads=1
```

Each case uses five samples. Link-success uses an injectable hard-link implementation. Copy-fallback deliberately fails linking and exercises the native recursive copy path. Source files are flat 16-byte files so link behavior is deterministic across the benchmark.

| Size | Files | Mode | First median | Repeated unchanged median | First samples (us) | Repeated samples (us) |
| --- | ---: | --- | ---: | ---: | --- | --- |
| small | 20 | link-success | 2.854 ms | 4.304 ms | 2544, 2580, 2854, 2869, 2965 | 4156, 4204, 4304, 4646, 4787 |
| small | 20 | copy-fallback | 19.531 ms | 19.757 ms | 13524, 17035, 19531, 19585, 41952 | 17512, 19240, 19757, 20502, 20599 |
| medium | 500 | link-success | 43.684 ms | 83.263 ms | 42656, 43197, 43684, 45233, 46026 | 80929, 82432, 83263, 83423, 83743 |
| medium | 500 | copy-fallback | 438.172 ms | 476.391 ms | 377696, 399120, 438172, 508113, 514951 | 453687, 472026, 476391, 553303, 570306 |
| large | 2000 | link-success | 188.476 ms | 335.256 ms | 186139, 187166, 188476, 189500, 195697 | 328449, 329135, 335256, 337833, 344348 |
| large | 2000 | copy-fallback | 2373.591 ms | 2070.771 ms | 2279370, 2306168, 2373591, 2387845, 2946246 | 1899697, 1982681, 2070771, 2098502, 2201117 |

Copy-fallback warning projection is bounded at 64 warnings for the medium and large cases.

## Production FRB benchmark on Windows

Command:

```text
flutter test integration_test/agent_runtime_overlay_benchmark.dart -d windows --dart-define=ALERA_RUN_AGENT_OVERLAY_BENCHMARK=true
```

The benchmark calls the public generated FRB API `prepareAgentRuntimeOverlay` and therefore exercises the actual production bridge and Windows link implementation. A 2 ms Dart timer runs while each first native call is in flight to provide UI-isolate responsiveness evidence. `ProcessInfo.currentRss` is sampled before and after the first call.

On this Windows host all three sizes used link-success with `copied=0` and `warnings=0`.

| Size | Files | First median | Repeated unchanged median | Heartbeat median | Max heartbeat gap median | RSS delta median | Actual mode |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| small | 20 | 6.297 ms | 7.404 ms | 2 | 4.068 ms | 94,208 B | 20 linked, 0 copied |
| medium | 500 | 115.438 ms | 155.768 ms | 57 | 3.942 ms | 598,016 B | 500 linked, 0 copied |
| large | 2000 | 478.278 ms | 636.891 ms | 237 | 4.259 ms | 159,744 B | 2000 linked, 0 copied |

Five-sample production FRB latency evidence:

- small first: 6111, 6278, 6297, 6368, 10229 us
- small repeated: 6837, 7199, 7404, 7519, 7579 us
- medium first: 112820, 113263, 115438, 117998, 118920 us
- medium repeated: 153167, 155482, 155768, 156415, 158909 us
- large first: 458343, 468414, 478278, 480467, 486828 us
- large repeated: 622515, 624862, 636891, 637974, 641066 us

For the large case, the Dart heartbeat ran a median 237 times during the native call and the median maximum timer gap was 4.259 ms. This demonstrates that the FRB call yields the Dart event loop instead of blocking the UI isolate for the full native filesystem duration.

## Repeated unchanged finding

Repeated unchanged preparation is not currently a no-op. The implementation removes and reconstructs overlay content on the repeated call. The Rust test asserts `removed_count > 0`, and the benchmark records the resulting cost.

This is most visible in the large link-success case:

- deterministic Rust: 188.476 ms first, 335.256 ms repeated unchanged
- production FRB on Windows: 478.278 ms first, 636.891 ms repeated unchanged

The copy-fallback path is substantially more expensive at large scale, with a Rust median of 2373.591 ms first and 2070.771 ms repeated unchanged.

A future optimization can evaluate unchanged-source reuse or a safe fingerprint/reconciliation shortcut. That is intentionally outside A2 because A2 is validation-only.

## Bridge and boundary evidence

Windows integration tests verify:

- successful native overlay reconciliation through FRB
- cleanup warnings are returned to Dart for invalid containment
- preparation failures cross FRB as Dart errors

The unit boundary test scans the five `agent_runtime_overlay_*` launch files and rejects known recursive Dart filesystem patterns such as recursive list/delete helpers and old mirror/copy helpers. This assertion is intentionally scoped to the agent runtime overlay launch path, not unrelated runtime-resource synchronization code.

## Validation commands

Passed:

```text
cargo test --manifest-path rust/Cargo.toml agent_runtime_overlay --lib
cargo test --manifest-path rust/Cargo.toml benchmark_overlay_production_matrix --lib -- --ignored --nocapture --test-threads=1
flutter test test/unit/agent_runtime_overlay_native_boundary_test.dart
flutter analyze integration_test/agent_runtime_overlay_native_test.dart integration_test/agent_runtime_overlay_benchmark.dart test/unit/agent_runtime_overlay_native_boundary_test.dart
flutter test integration_test/agent_runtime_overlay_benchmark.dart -d windows
flutter test integration_test/agent_runtime_overlay_native_test.dart -d windows
flutter test integration_test/agent_runtime_overlay_benchmark.dart -d windows --dart-define=ALERA_RUN_AGENT_OVERLAY_BENCHMARK=true
```

The default benchmark invocation is intentionally skipped unless `ALERA_RUN_AGENT_OVERLAY_BENCHMARK=true` is supplied, so normal test discovery does not initialize the native library or run a filesystem benchmark.

The Windows build emits existing `alera-cli` unused/dead-code warnings. They are unrelated to A2 and do not fail the integration tests.

A repository-wide `cargo fmt --manifest-path rust/Cargo.toml -- --check` also exposes a pre-existing formatting difference in `rust/src/api/git_diff_blob_tests.rs` around line 114. The A2 Rust file itself was formatted with `rustfmt --edition 2024 rust/src/api/agent_runtime_overlay.rs`; the unrelated baseline file was not modified.

## A2 conclusion

The agent runtime overlay launch path remains native for recursive filesystem work, the public FRB path stays responsive on the Dart UI isolate during large Windows overlay preparation, warning/error projection is covered, and reproducible five-sample link/copy evidence is now available. The main follow-up opportunity is repeated-unchanged reconciliation, which currently rebuilds rather than taking a no-op fast path.
