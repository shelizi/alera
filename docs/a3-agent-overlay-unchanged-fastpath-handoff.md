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

The actual Windows Flutter integration rerun could not start on this node because CMake has no available C++ compiler:

```text
CMake Error at CMakeLists.txt:3 (project):
  No CMAKE_CXX_COMPILER could be found.
```

The worktree-local submodule dependencies were temporarily made available to confirm dependency resolution, then restored without repository changes. `cl.exe` is not available in the current environment and a Visual Studio C++ installation could not be located. Therefore the pre-A3 production-bridge baseline in the plan remains the latest completed FRB measurement; the new FRB assertions/benchmark should be rerun on a Windows environment with the Visual Studio C++ desktop toolchain installed.

## Merge notes

- No FRB/generated files changed.
- No terminal/editor ownership areas changed.
- Expected conflict risk with C4/T4/T5/S1 remains low.
- Commit exact A3 paths only; do not stage unrelated workspace formatting drift.
