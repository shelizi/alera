# C6 retained parser scheduling handoff

Date: 2026-09-18

## Status

Production implementation and correctness closure are complete on:

- branch: `perf/editor-native-parse-scheduling`
- worktree: `.worktrees/editor-native-parse-scheduling`
- base evidence commit: `24053f1d22de50b07b0c2792d49b86f2eda863f9`
- after-profile baseline head: `c00b8299` (`docs: add C6 retained parser handoff`)

The Flutter/Windows C6P **after** profile is now complete. The host already contained Visual Studio 2022 Community on `F:`; after repairing the Windows build environment/discovery path, the full Windows integration matrix and rapid-replacement workload ran successfully.

C6 is therefore performance-closed as well as correctness-closed.

## Baseline decision from C6P

C6P established that the expensive stage is the retained Tree-sitter **full cold parse**, not Rope clone, parser construction, or the bounded first-viewport query.

Representative five-sample medians from the final C6P native release matrix:

| Language | 20k full parse | 50k full parse | 100k full parse |
| --- | ---: | ---: | ---: |
| Rust | 260.443 ms | 4,706.590 ms | 6,275.008 ms |
| Dart | 587.386 ms | 1,887.317 ms | 3,824.708 ms |
| TypeScript | 750.133 ms | 1,846.181 ms | 4,780.498 ms |

The first 80-line viewport query stayed around the sub-millisecond to roughly 1 ms range.

The C6P rapid-supersession workload (20k Rust, three generations) measured:

- median total parse work: 5,860.679 ms
- median useful final-generation work: 1,949.679 ms
- median superseded/wasted work: 3,882.733 ms
- theoretical discarded-generation fraction: 66.7%

That evidence selected cancellation/supersession first, then large-file deferred admission.

Full baseline report:

- `docs/editor-retained-parser-c6p-profile.md`

## Commits

### 1. Cooperative cancellation

`a24f546bd86a94ffb86e55de68c4c0977eacba79 perf: cancel stale retained parses`

Key changes:

- adds FRB-exposed `NativeParseCancellation`;
- adds `NativeEditorDocument.openFromRopeCancellable`;
- wires Tree-sitter 0.26 `ParseOptions.progress_callback` to cooperative cancellation;
- new controller generations cancel the previous cold parse;
- reset/fallback/dispose cancel pending parse work;
- stale generation publication is still blocked by the existing generation guard;
- generated FRB bindings are regenerated from Rust source.

The in-flight cancellation regression starts a 100k Rust parse and cancels it from another thread after the parse has begun. The parse terminates with the explicit cancellation result rather than completing and publishing state.

### 2. Post-first-frame large-file admission

`b04e168ec29a23a44a5f6751c0dee7eada7d830f perf: defer large-file retained parse`

Key changes:

- reuses the existing Alera large-file policy instead of introducing another threshold:
  - >= 5,000 logical lines, or
  - >= 512 KiB content;
- small files preserve eager retained parsing;
- large files wait for `SchedulerBinding.instance.endOfFrame` before creating the cancellation token and starting the native cold parse;
- a generation replaced/closed during that wait returns before native parse admission;
- parse baseline is a copy-on-write `RopeBridge.deepClone()` tied to the captured revision;
- edits committed during the deferred/pending window remain queued and are applied after the retained parse becomes ready.

### 3. Lifecycle/correctness hardening

`ab6bdefe094fbfc666fc9550e80949d8b986de83 test: harden retained parse lifecycle`

Key changes:

- adds an executable native test proving a deferred Rope snapshot remains immutable while the live Rope changes, then applies the queued edit exactly once;
- file switches now invalidate/cancel retained parser work before replacing the controller's authoritative Rope;
- architecture boundary coverage verifies this ordering.

## Readiness contract after C6

The production flow is now:

```text
Rope ready
-> native parser generation configured
-> large file: wait until current frame ends
-> cancellation token created
-> full Tree-sitter parse starts
-> stale/closed/replaced generation can cooperatively cancel in-flight parse
-> retained document publishes only if generation still current
-> queued revision deltas synchronize
-> tree consumers use only matching current revision
```

Important properties:

- no whole-document Dart snapshot is introduced by C6;
- the deferred baseline clone is Ropey copy-on-write;
- no consumer starts an independent cold parse;
- viewport syntax owns the eventual async readiness path;
- bracket matching and structural selection return fallback/null while retained state is not ready;
- folding/outline are asynchronous and do not synchronously block paint/cursor/UI-isolate interaction on first parse;
- unsupported grammars continue to fall back cleanly.

## Correctness-gate coverage

| Gate | Evidence |
| --- | --- |
| edit while initial parse pending | `deferred_rope_snapshot_applies_pending_edit_exactly_once` plus queued revision stream |
| close/dispose while parsing | controller cancellation lifecycle + in-flight cancellation test |
| language/new-generation switch | new configure generation cancels prior token before replacement |
| reload/file replace | `openWorkspaceFile` reset path + file-switch reset/cancel ordering |
| stale completion | generation guard retained after cancellable open |
| unsupported grammar | existing `unsupported_language_retains_text_but_falls_back_cleanly` |
| parse cancellation/failure fallback | explicit cancellation result; current-generation errors enter native fallback |
| viewport before ready | async retained open path; no synchronous UI wait |
| folding before/after ready | async native query with legacy fallback; native folding regression coverage |
| bracket before/after ready | synchronous null/fallback before ready; native bracket regressions after ready |
| structural selection before/after ready | no-op before ready; retained-tree regressions after ready |
| outline before/after ready | LSP/native async path; no synchronous first-parse wait |
| Unicode scalar revision correctness | existing Unicode edit/selection/symbol tests |
| duplicate cold parses | one controller generation owns one open future/token; consumers synchronize to it |

## Validation completed

CodeForge native:

- `cargo test`: 30 passed, 0 failed, 1 ignored manual benchmark
- `rustfmt --check src/api/editor_document.rs`: passed

Flutter/Dart:

- `flutter analyze lib/code_forge/controller.dart lib/code_forge/code_area.dart`: passed during C6
- `test/unit/editor_native_document_integration_boundary_test.dart`: 6/6 passed
- `test/widget/workspace_editor_surface_test.dart`: 20/20 passed
- `git diff --check`: passed for each production batch

The full crate `cargo fmt -- --check` still reports pre-existing formatting drift in `rust/src/api/editor.rs`; C6-owned Rust source passes its focused rustfmt check.

## Windows after-profile closure

The benchmark instrumentation was executed through the Windows integration runner after making the existing `F:` Visual Studio 2022 toolchain visible to Flutter/CMake.

Five-sample after-profile medians:

| Language | Lines | Rope ready | Open -> first useful frame | Open -> syntax ready |
| --- | ---: | ---: | ---: | ---: |
| Rust | 2k | 5.624 ms | 42.588 ms | 232.355 ms |
| Rust | 20k | 49.957 ms | 94.664 ms | 1,128.307 ms |
| Rust | 50k | 118.534 ms | 158.193 ms | 2,760.323 ms |
| Rust | 100k | 237.994 ms | 279.086 ms | 5,214.875 ms |
| Dart | 2k | 4.641 ms | 41.704 ms | 163.321 ms |
| Dart | 20k | 37.746 ms | 74.391 ms | 902.805 ms |
| Dart | 50k | 96.647 ms | 122.134 ms | 1,363.531 ms |
| Dart | 100k | 190.147 ms | 221.303 ms | 2,895.852 ms |
| TypeScript | 2k | 4.614 ms | 40.604 ms | 174.727 ms |
| TypeScript | 20k | 44.671 ms | 70.275 ms | 563.640 ms |
| TypeScript | 50k | 105.494 ms | 137.477 ms | 1,263.913 ms |
| TypeScript | 100k | 226.783 ms | 253.106 ms | 2,445.289 ms |

This is the key C6 closure result: at 100k lines the editor reaches a useful first frame in about 221-279 ms median even though retained syntax readiness remains about 2.45-5.21 s. The multi-second cold parse is therefore no longer a prerequisite for first useful paint.

`WorkspaceSourceInfo` stayed bounded at 148-152 bytes across the matrix. RSS samples were collected but are noisy because process working-set movement includes allocator/GC reuse; they are not treated as precise per-document heap attribution.

Rapid replacement, 20k Rust with three successive generations:

- issue wall median: 7.536 ms
- final-generation syntax-ready median: 984.197 ms
- RSS delta median: 933,888 bytes

Final Windows integration result: `All tests passed.`

The benchmark harness also needed one correctness fix: its shared native-open helper had retained the C4-specific 50k minimum-line assertion, while the C6P matrix intentionally contains 2k and 20k cases. The helper now accepts a per-case expected minimum; C4 retains the 50k default.

Original direct command documented by C6P:

```powershell
flutter test integration_test/editor_open_profile_benchmark.dart -d windows --plain-name "profiles C6P retained parser readiness matrix"
```

Because this host's Flutter 3.47.2 test/path discovery interacts poorly with the MCP extended-path working directory and the unregistered portable VS2022 instance, the successful run used an equivalent temporary Windows harness to expose the same project at a normal non-root drive path and execute the existing integration benchmark. Those temporary host/build helpers are not production changes and are not part of the C6 commit set.

## Integration order / conflict notes

The C6 scheduling branch already contains the C6P evidence commit. From current `main` at `c7947b42b623147465e1534844d340b3912d7152`, integrating this branch includes:

1. `24053f1d` C6P evidence
2. `a24f546b` cooperative cancellation
3. `b04e168e` large-file deferred admission
4. `ab6bdefe` lifecycle/correctness hardening

There is an active parallel branch `perf/codeforge-large-file-fastpath` that also modifies:

- `third_party/code_forge/lib/code_forge/controller.dart`
- `third_party/code_forge/lib/code_forge/code_area.dart`

Therefore do not blindly merge both branches. Rebase or merge in a controlled order and re-run:

- CodeForge analyze
- retained native architecture boundary test
- workspace editor surface test
- CodeForge Rust tests

The currently active `perf/editor-large-file-highlight` and `perf/editor-change-refresh` branches did not show overlap with the C6-owned parser files in the latest overlap check.

## Next decision

No additional parser architecture phase is justified from the current evidence before the Windows after-profile closes.

C6 can be marked fully complete. The after-profile confirms that first-useful-frame latency is decoupled from retained syntax readiness for large files, bounded FRB metadata remains intact, and rapid replacement completes without a lifecycle failure.

No additional parser architecture phase is justified from this evidence. Query compilation/cache work remains lower priority than the cold full parse and should not be selected without new measurements.

