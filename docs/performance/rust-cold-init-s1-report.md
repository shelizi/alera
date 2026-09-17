# S1 Process-cold Rust initialization profile

Date: 2026-09-17

Base: `f5e0cb2ffafd82e5a95d8c6fc8f0ae2fa022482c`

Branch/worktree: `perf/rust-cold-init-profile` / `.worktrees/rust-cold-init-profile`

Scope: evidence/profile only. No production startup behavior is changed by S1.

## Decision

Do **not** change the two startup calls to `Future.wait`/concurrent initialization based on this profile. Five fresh-process samples show no material reduction: sequential median `bothReady` was **22.855 ms** and concurrent median was **22.767 ms**, a difference of only **0.088 ms (~0.4%)**, which is within run-to-run noise.

If startup work is changed later, the next evidence gate should test **deferring CodeForge initialization until after first frame or before the first editor use**, not merely starting the two FRB init futures together. That change must be validated in a packaged/profile app before production adoption.

## What is on the startup critical path?

Current `lib/main.dart` startup awaits these in order before `runApp()`:

```text
await RustLib.init()
await code_forge.RustLib.init()
...
runApp(...)
```

Therefore both initializations are currently on the user-visible pre-first-frame critical path. There is no lazy initialization hiding either cost in the current startup sequence.

The Rust-side custom initializers are both trivial: they call `flutter_rust_bridge::setup_default_user_utils()`. The measured cost is therefore principally dynamic-library loading, FRB setup/wire initialization, and the first native call rather than application-specific Rust initialization logic.

## Prior production evidence

C3 recorded one-time process-cold initialization at approximately:

```text
Alera Rust library:     590.13 ms
CodeForge Rust library: 228.57 ms
```

Those numbers remain the relevant existing packaged-app/startup evidence. The S1 standalone probe below uses directly built **Rust dev DLLs** to decompose the load/FRB/first-call path and compare sequential vs concurrent behavior. Its absolute milliseconds must not be substituted for C3's packaged/profile figures.

## S1 probe

`tool/performance/rust_cold_init_probe.dart` runs one scenario per Dart process and emits a single JSON record. A new process is used for every sample so FRB one-shot state is never reused across samples.

The probe can measure:

- `ExternalLibrary.open` for Alera and CodeForge separately;
- `RustLib.init(externalLibrary: ...)` separately from library open;
- one lightweight first native call from each library;
- sequential Alera -> CodeForge startup;
- both FRB init futures started together after both DLLs are opened;
- process RSS checkpoints.

Rust DLLs were built directly from the same S1 worktree source:

```text
cargo build --manifest-path rust/Cargo.toml
cargo build --manifest-path third_party/code_forge/rust/Cargo.toml
```

## Windows five-sample results

Environment: Windows 11 build 26200. Each row is a separate process. Samples were executed serially to avoid cross-sample CPU/I/O contention.

### Sequential explicit-DLL initialization

| Sample | Alera open | Alera FRB init | CodeForge open | CodeForge FRB init | Both ready | Alera first call | CodeForge first call |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 1.757 ms | 18.639 ms | 0.683 ms | 3.294 ms | 24.691 ms | 5.407 ms | 1.043 ms |
| 2 | 1.702 ms | 17.494 ms | 0.385 ms | 2.992 ms | 22.855 ms | 5.583 ms | 0.927 ms |
| 3 | 1.670 ms | 17.334 ms | 0.362 ms | 2.789 ms | 22.432 ms | 5.718 ms | 0.869 ms |
| 4 | 1.546 ms | 18.217 ms | 0.400 ms | 2.922 ms | 23.354 ms | 5.448 ms | 0.939 ms |
| 5 | 1.837 ms | 17.151 ms | 0.421 ms | 2.925 ms | 22.617 ms | 5.610 ms | 0.979 ms |
| **Median** | **1.702 ms** | **17.494 ms** | **0.400 ms** | **2.925 ms** | **22.855 ms** | **5.583 ms** | **0.939 ms** |

On this dev-DLL decomposition, FRB initialization dominates DLL-open time. Alera is the larger component.

### Concurrent FRB-init attempt

Both DLLs are opened first because `ExternalLibrary.open` is synchronous. The two `RustLib.init(externalLibrary: ...)` futures are then started before awaiting `Future.wait`.

| Sample | Alera open | CodeForge open | Concurrent init | Both ready |
| --- | ---: | ---: | ---: | ---: |
| 1 | 1.514 ms | 0.279 ms | 20.743 ms | 22.767 ms |
| 2 | 1.556 ms | 0.337 ms | 20.119 ms | 22.244 ms |
| 3 | 1.603 ms | 0.294 ms | 20.686 ms | 22.860 ms |
| 4 | 1.601 ms | 0.352 ms | 20.516 ms | 22.712 ms |
| 5 | 1.617 ms | 0.323 ms | 20.817 ms | 23.078 ms |
| **Median** | **1.601 ms** | **0.323 ms** | **20.686 ms** | **22.767 ms** |

Sequential `bothReady` median: **22.855 ms**.

Concurrent `bothReady` median: **22.767 ms**.

Observed difference: **-0.088 ms (~-0.4%)**.

This is not a material improvement. The concurrent-init interval itself is also approximately the sum-scale of the sequential FRB work, indicating that simply starting both futures does not create useful parallel execution for this path.

## RSS observations

RSS was sampled before initialization and after both libraries were ready. The sequential samples had a median increase of roughly **1.13 MiB**; the concurrent samples had a median increase of roughly **1.26 MiB**. The Dart VM also performed memory reclamation around first calls, so these numbers are noisy and are useful only as a coarse guardrail.

There is no evidence here that concurrent init saves memory, and no large RSS regression was observed from merely changing scheduling in the probe. A post-first-frame/lazy production experiment should measure whole-app working set/RSS separately because the standalone Dart VM is not representative of the Flutter app's total memory footprint.

## Lazy/prewarm assessment

Current startup is not lazy: both libraries are awaited before `runApp()`.

Alera has broad application responsibilities and should remain the more conservative candidate for pre-first-frame initialization unless a packaged startup trace proves it can move safely.

CodeForge is editor-focused, so it is the stronger candidate for an explicit readiness gate with either:

1. post-first-frame prewarm, or
2. lazy initialization immediately before first editor/native-document use.

S1 intentionally does **not** implement either option. The plan requires evidence before adding startup prewarm/concurrency behavior, and the direct concurrency option has now failed that evidence gate.

## Platform/packaging limitation

The requested Windows packaged/profile A/B could not be run in this worktree because the current Flutter 3.47.2 + Visual Studio 2026 environment fails while generating the Windows runner: CMake does not establish `CMAKE_CXX_COMPILER` even though Flutter Doctor detects VS 2026 and `cl.exe` exists. This is an environment/toolchain compatibility issue before the S1 test process begins.

The direct DLL probe therefore answers the decomposition and overlap questions on Windows, but S1 does **not** claim an empirical Windows-vs-Linux/macOS packaging comparison. Generated FRB loader configuration is platform-aware, so a cross-platform conclusion requires equivalent packaged fresh-process traces on those platforms.

## Recommended next gate

No S1 production patch now.

If startup optimization is prioritized later:

1. restore/fix the Windows packaged/profile runner toolchain;
2. capture at least five fresh process startup traces for current startup;
3. prototype only a CodeForge post-first-frame/lazy-init variant behind an explicit readiness gate;
4. repeat first-frame/startup, first-editor-open, CPU contention, and whole-app RSS measurements;
5. keep the change only if perceived startup improves materially without moving an unacceptable stall into first editor open.

## Reproduction

Example individual probe:

```text
dart tool/performance/rust_cold_init_probe.dart \
  --scenario=alera_preopened \
  --alera-library=<path-to-alera_native.dll>
```

Example sequential/concurrent comparison:

```text
dart tool/performance/rust_cold_init_probe.dart \
  --scenario=sequential_preopened \
  --alera-library=<path-to-alera_native.dll> \
  --codeforge-library=<path-to-code_forge.dll>

dart tool/performance/rust_cold_init_probe.dart \
  --scenario=concurrent_preopened \
  --alera-library=<path-to-alera_native.dll> \
  --codeforge-library=<path-to-code_forge.dll>
```

Each invocation is one fresh-process sample.
