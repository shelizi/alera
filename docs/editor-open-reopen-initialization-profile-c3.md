# C3 editor open/reopen initialization profile

Date: 2026-09-17  
Branch: `perf/editor-open-profile`  
Base HEAD before C3: `e0aae19e`  
Scope: profiling/benchmark only. No production CodeForge/editor files were changed while C2 is active.

## Goal

C3 follows `docs/rust-heavy-operation-parallel-work-plan.md` section 7 and attributes the remaining editor open/reopen initialization cost. The B checkpoint reported roughly 347 ms wall, 37 ms build p95, and 1.86 ms raster p95, which suggested initialization/build work rather than sustained GPU raster work.

The C3 harness is `integration_test/editor_open_profile_benchmark.dart`. It uses a deterministic 50,000-line, 4,044,840-byte fixture, takes five samples per stage, and reports median / p95 / MAD. It separates disjoint production-path stages from overlapping diagnostic controls so overlapping measurements are not added together.

## Reproduce

```powershell
flutter test integration_test/editor_open_profile_benchmark.dart -d windows
```

On this Windows host the local `ghostty_vte` checkout initially forced a source native-asset build, but Zig is not installed. That terminal dependency is unrelated to this editor benchmark. A compatible DLL from the same local checkout was copied into the ignored worktree cache at `.prebuilt/windows-x64/ghostty-vt.dll`; the normal command above then built and ran successfully. The cache is covered by `/.prebuilt/` in `.gitignore` and is not part of C3.

Validation result: `All tests passed!`

## Measured results

### Disjoint recurring open-path stages

| Rank | Stage | Median | p95 | MAD | Interpretation |
| ---: | --- | ---: | ---: | ---: | --- |
| 1 | `production_native_editor_read` | 259.22 ms | 262.09 ms | 1.94 ms | Production native read/decode/normalization plus FRB payload back to Dart |
| 2 | `initial_rope_construction` | 257.82 ms | 274.11 ms | 5.91 ms | Full Dart `String` transferred into `RopeBridge.create` and native Rope construction |
| 3 | `first_codeforge_frame` | 59.28 ms | 61.03 ms | 1.75 ms | First CodeForge widget/state/layout frame; controller/Rope construction excluded |
| 4 | `controller_init` | 0.05 ms | 0.23 ms | 0.01 ms | `CodeForgeController` + `FindController` creation |

The independently measured full path, `full_open_to_first_frame`, was **544.27 ms median / 564.30 ms p95 / 18.16 ms MAD**. Its frame timings were build median 30.87 ms / p95 33.03 ms and raster median 2.39 ms / p95 3.09 ms.

Do not sum medians from independently sampled stages as an exact decomposition. Their magnitudes are still decisive: native editor read and the subsequent full-text Rope construction are both about 258-259 ms, while controller creation is negligible and the isolated first CodeForge frame is materially smaller.

### Diagnostic controls

| Stage | Median | p95 | What it tells us |
| --- | ---: | ---: | --- |
| `raw_file_read` | 3.70 ms | 6.01 ms | Warm OS-cache file I/O itself is small |
| `dart_utf8_decode` | 0.98 ms | 1.04 ms | Plain Dart UTF-8 decode alone is small |
| `native_decode_ffi` | 315.51 ms | 337.15 ms | Full bytes -> Rust decode -> Dart String round trip is expensive; overlaps other stages and is not ranked |
| `first_viewport_materialization` | 261.14 ms | 278.81 ms | Includes a fresh Rope; compared with Rope construction, viewport extraction itself is not the dominant cost |
| `syntax_setup_and_first_viewport_control` | 2.91 ms | 5.15 ms | Diagnostic control only; the 50k production large-file path disables `preHighlightLines` |
| `same_controller_rebuild` | 22.55 ms | 40.23 ms | Proxy for widget/provider fan-out while retaining the same Rope; secondary to full-text transfers |

`same_controller_rebuild` wall time has five samples. Only three iterations emitted usable frame timing groups because identical `pumpWidget` calls do not always schedule a raster frame; therefore its wall timing is the useful fan-out signal, while its build/raster sub-samples should not be treated as a five-sample distribution.

### One-time native initialization

The harness also measured process-cold initialization separately:

- Alera Rust library init: **590.13 ms**
- CodeForge Rust library init: **228.57 ms**

These are important for cold application startup, but they are not recurring reopen costs once both libraries are initialized, so they are deliberately excluded from the recurring contributor ranking.

## Attribution

The evidence moves C3 away from syntax/layout as the first target and toward **duplicate whole-document transfer/construction**:

1. The production read path returns the whole ~4.0 MB editor text through the native/FRB boundary and costs ~259 ms median.
2. Assigning that resulting Dart `String` to `CodeForgeController.text` immediately sends the whole text back through the CodeForge native bridge to construct a Rope, costing another ~258 ms median.
3. Initial CodeForge frame work is real (~59 ms isolated wall; full-path build p95 ~33 ms), but it is much smaller than either full-text stage.
4. Controller creation is effectively noise at 0.05 ms median.
5. Raw storage read, plain Dart UTF-8 decode, syntax setup control, and viewport-only work are not first-order contributors in this harness.

The B and C3 wall values are not an apples-to-apples regression comparison: C3 deliberately adds a production native file-read stage and uses a 4,044,840-byte fixture to split the pipeline. The useful comparison is the attribution inside C3, not 544 ms versus the earlier ~347 ms as a release regression claim.

## Recommended production follow-up after C2 merges

C3 must not modify C2-owned production CodeForge files. After C2 lands, the next optimization should target the boundary between workspace file loading and the native editor document/Rope so a large file is not materialized as a full Dart `String` and then immediately copied back into Rust.

Concrete follow-up candidates, in measured priority order:

1. Reuse or hand off a native document/Rope handle from the file-read path into CodeForge instead of native -> full Dart String -> native Rope round-tripping.
2. Avoid reconstructing a full Rope on reopen when the underlying document/content can be retained safely.
3. Re-profile first-frame state/layout and provider fan-out only after the two ~258 ms whole-document stages are reduced; they are currently secondary.
4. Keep process-cold Rust initialization as a separate startup optimization track rather than mixing it with editor reopen work.

Any production implementation touching CodeForge remains deferred until C2 merges, exactly as required by the parallel-work plan.
