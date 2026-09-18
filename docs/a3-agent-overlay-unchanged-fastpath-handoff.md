# A3 agent overlay repeated-unchanged fast path handoff

## Scope

- Work package: `A3` from `docs/rust-heavy-operation-parallel-work-plan.md`.
- Branch: `perf/agent-overlay-unchanged-fastpath`.
- Base: `f5e0cb2ffafd82e5a95d8c6fc8f0ae2fa022482c`.
- Public FRB API shape is unchanged; no generated bindings were regenerated.

## Implementation

`rust/src/api/agent_runtime_overlay.rs` now keeps a private, versioned reuse manifest under the declared overlay root. A repeated preparation returns without teardown, writes, relinking, or copying only when both of these still match:

1. request/input identity: normalized paths, mirror layout, managed-file semantics/content, generated wrapper content, source path and recursive source contents;
2. materialized overlay identity: actual filesystem contents plus the prior link/copy materialization counts and copy markers.

If either identity is stale, missing, corrupt, or uses another manifest schema, preparation falls back to the existing full safe rebuild path. Cleanup and failed preparation also clear the private reuse manifest.

## Regression coverage

Focused Rust suite:

```text
cargo test -p alera_native agent_runtime_overlay -- --nocapture
21 passed; 0 failed; 1 ignored
```

Coverage includes:

- link-success unchanged reuse;
- forced copy-fallback unchanged reuse;
- source content change;
- source add/remove/rename;
- managed/generated wrapper content change;
- manually corrupted or removed overlay target;
- removed copied-resource marker;
- stale manifest schema;
- existing Windows containment and path-safety cases.

`cargo build -p alera_native` also succeeds. `rustfmt` was run on the A3 Rust file and `dart format` on the two integration-test files.

Repository-wide `cargo fmt --all -- --check` is currently blocked by pre-existing formatting drift in unrelated `alera-cli` / `alera-core` files; A3 did not rewrite those files.

## Native Windows five-sample benchmark

The same 20 / 500 / 2000 file matrix was captured before and after A3 on this Windows node. Times below are medians for the repeated-unchanged call.

| Files | Mode | Before | After | Change |
| ---: | --- | ---: | ---: | ---: |
| 20 | link | 4.148 ms | 9.509 ms | small-case validation overhead |
| 20 | forced copy | 22.153 ms | 11.226 ms | 1.97x faster |
| 500 | link | 82.692 ms | 42.564 ms | 1.94x faster |
| 500 | forced copy | 4,815.203 ms | 62.601 ms | 76.9x faster |
| 2000 | link | 1,936.822 ms | 181.511 ms | 10.7x faster |
| 2000 | forced copy | 13,852.686 ms | 278.280 ms | 49.8x faster |

Every post-A3 repeated sample reported zero removed/written/linked/copied entries and no warnings.

The 20-file link case is intentionally not optimized with weaker mtime-only validation: A3 keeps content/integrity checks so source or target corruption cannot be silently reused. The measured target is the medium/large heavy-operation path, where the teardown/copy savings dominate.

## Production FRB validation status

`integration_test/agent_runtime_overlay_native_test.dart` and `integration_test/agent_runtime_overlay_benchmark.dart` were updated so the production bridge expects repeated unchanged preparation to be a zero-mutation reuse.

The A3V closure reran both tests through the actual Flutter/Windows production bridge after repairing the desktop build environment. The working environment was Visual Studio Community 2026 18.6.1 / MSVC 14.51.36231. The MCP shell needed the standard Windows system-path variables restored and the Coreutils `link.exe` entry removed from `PATH` so MSVC/MSBuild/FileTracker could configure normally.

Production bridge correctness:

```text
flutter test integration_test/agent_runtime_overlay_native_test.dart -d windows
3 passed; 0 failed
```

This verifies link-success reconciliation plus the public cleanup-warning and preparation-error projections to Dart.

Production FRB benchmark:

```text
flutter test integration_test/agent_runtime_overlay_benchmark.dart -d windows \
  --dart-define=ALERA_RUN_AGENT_OVERLAY_BENCHMARK=true
1 passed; 0 failed
```

The benchmark executes five samples for each 20 / 500 / 2000-file scenario. The original 30-second test-package timeout was too short for the full 15-sample production matrix, so A3V raises only this benchmark harness timeout to five minutes; no production behavior changed.

| Files | First preparation median | Repeated unchanged median | Heartbeat ticks median | Max heartbeat gap median | RSS delta median |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 20 | 8.937 ms | 8.532 ms | 3 | 3.590 ms | 176,128 B |
| 500 | 161.476 ms | 36.265 ms | 77 | 11.595 ms | 520,192 B |
| 2000 | 2,615.648 ms | 550.038 ms | 1087 | 44.704 ms | 360,448 B |

Five-sample timing sets:

- 20 first: `[8.416, 8.576, 8.937, 9.367, 14.001] ms`; unchanged: `[6.813, 7.962, 8.532, 8.611, 10.141] ms`.
- 500 first: `[148.385, 151.763, 161.476, 695.931, 762.693] ms`; unchanged: `[35.172, 35.608, 36.265, 125.860, 182.536] ms`.
- 2000 first: `[2,505.971, 2,605.404, 2,615.648, 3,084.639, 3,884.541] ms`; unchanged: `[520.268, 543.520, 550.038, 637.596, 729.260] ms`.

Every repeated-unchanged production-FRB sample asserted zero removed/written/linked/copied entries and no warnings. Every first preparation used the Windows link path (`linked == file count`, `copied == 0`, no warnings). The 2000-file scenario also kept the Dart heartbeat active, confirming the async FRB call yields while the native filesystem workload runs.

A3 is therefore closed end-to-end. A4 is not justified by this validation run.

## Merge notes

- No FRB/generated files changed.
- No terminal/editor ownership areas changed.
- Expected conflict risk with C4/T4/T5/S1 remains low.
- Commit exact A3 paths only; do not stage unrelated workspace formatting drift.
