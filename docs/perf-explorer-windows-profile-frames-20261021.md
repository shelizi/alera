# Explorer Windows Profile frame timing — 2026-10-21

## Scope

This is the first isolated **Windows Flutter Profile/AOT frame-timing**
measurement for the production `WorkspaceExplorer` widget and the native
Rust FFI *versioned session* path. It is **not** a before/after A/B test
and does not claim an application-wide FPS speedup.

- Host: Windows 10 Pro for Workstations (Build 26200), NVIDIA GTX 1080 Ti.
- Command: `flutter drive --profile -d windows --no-pub --target integration_test/explorer_frame_benchmark.dart --driver test_driver/explorer_frame_benchmark_test.dart`.
- Build with `ALERA_E2E_DESKTOP=1` so the Windows runner receives its
  non-production App ID rather than forwarding to an installed/running Alera.
- On this development machine, `ALERA_E2E_REUSE_CLI=1` permits reusing the
  verified locally built CLI sidecar instead of rebuilding it. This is optional;
  **do not set it** unless that binary is already present.
- The fixture uses 100, 300, or 1,000 files **at the root** (not 1,000
  *loaded directories*), then toggles 150-file additions/removals and a
  one-file metadata-only change. Five measured iterations after warm-up.
- Actual Explorer rows/controller, Rust projection sessions and FRB are
  exercised. File listings, Git state and watcher signals are deterministic
  in-memory fixtures: disk I/O, native watcher latency, and full real
  repository activity are deliberately excluded.

## Measured frame p95 (milliseconds)

| Existing root files | Operation | Build p95 | Raster p95 | Total frame p95 | Frames measured | >16.7 ms |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 100 | add 150 | 1.233 | 1.869 | 4.514 | 17 | 0 |
| 100 | remove 150 | 1.227 | 1.867 | 4.497 | 16 | 0 |
| 100 | metadata only | 0.920 | 1.910 | 3.819 | 23 | 0 |
| 300 | add 150 | 1.256 | 1.885 | 4.373 | 15 | 0 |
| 300 | remove 150 | 1.122 | 1.725 | 3.619 | 15 | 0 |
| 300 | metadata only | 1.056 | 1.687 | 3.762 | 20 | 0 |
| 1000 | add 150 | 1.466 | 1.619 | 4.373 | 15 | 0 |
| 1000 | remove 150 | 1.251 | 1.577 | 4.393 | 15 | 0 |
| 1000 | metadata only | 1.189 | 1.613 | 3.587 | 19 | 0 |

Every fixture completed 19 native session updates with **zero stateless
fallbacks**. The highest observed per-frame total was under 5 ms.

## Interpretation and caveats

- Flutter `FrameTiming` Build/Raster/total spans measure the engine frame
  pipeline, not direct GPU utilization percentages. The host has a working
  NVIDIA WDDM graphics driver, but the `flutter devices --machine`
  `hardwareRendering:false` value describes local emulators, not a
  real Windows Desktop GPU support failure.
- The operation wall-clock p50 is around 35 ms in the benchmark. Its
  `_awaitProjection` loop waits at 30 ms intervals; these wall numbers
  are dominated by artificial polling and **not** intrinsic UI latency.
- The test reports rendering after scheduled watcher refresh; it is not a
  steady-state 60 FPS workload, nor does it measure scrolling performance,
  cross-directory search, 1,000 expanded folders, or a full repository scan.
- There are only 5 measured add/remove cycles; p95 over roughly 15–17
  frames should be considered an engineering regression indicator, not a
  statistically robust user-experience guarantee.

## Reproduction and artifacts

Use a **clean isolated worktree** (the benchmark is an integration test, not
a production app feature). On Windows PowerShell:

```powershell
$env:ALERA_E2E_DESKTOP = '1'
flutter drive --profile -d windows --no-pub --target integration_test/explorer_frame_benchmark.dart --driver test_driver/explorer_frame_benchmark_test.dart
```

The host-side driver writes the JSON response at
`build/benchmarks/explorer-profile/explorer_windows_profile.json`.
The captured initial output is also checked in next to this document as
`docs/perf-explorer-windows-profile-frames-20261021.json` for comparisons.

Benchmark source: `integration_test/explorer_frame_benchmark.dart`,
driver: `test_driver/explorer_frame_benchmark_test.dart`.

**Follow-up:** run a paired before/after measurement in the same worktree
and GPU environment, and add a stress case of 100/300/1000 *loaded
directories* rather than only root files to validate scaling of more
expensive structural changes.