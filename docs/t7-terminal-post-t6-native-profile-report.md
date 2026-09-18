# T7 Post-T6 Native Windows Terminal Profile Report

Date: 2026-09-18
Branch: `perf/terminal-post-t6-native-profile`
Baseline: `c7947b42b623147465e1534844d340b3912d7152`
Status: **BLOCKED on native Windows integration build; portable five-sample profile evidence complete**

## Scope

T7 is an evidence-only gate after T6. It does **not** change production terminal behavior. The goal is to rerun the post-T6 production worker/reveal/render profiles, collect the native Windows restore and flush gates that T5 could not run, and use the combined evidence to decide whether any T8 architecture cut is justified.

The dedicated T7 worktree could not materialize its terminal submodules cleanly on this Windows host because the nested terminal dependency checkout hit the existing Windows long-path problem and the pinned xterm commit was not available from the current remote refs. The measurements below were therefore executed from the repository root at the exact same baseline commit, `c7947b42`, where the pinned terminal submodules were already materialized. Report and coordination-plan changes remain isolated on the T7 branch.

## Environment and native Windows recipe

Observed toolchain:

- Flutter 3.47.2 / Dart 3.13.2.
- Windows desktop device: `windows-x64`.
- Visual Studio Community 2026 18.6.1.
- Windows SDK 10.0.26100.0.
- The inherited MCP process PATH resolves `C:\Program Files\coreutils\bin\link.exe` before the MSVC linker and omits several standard Windows environment variables.

The most complete reproducible child-shell setup found during T7 was:

```bat
set CommonProgramFiles=C:\Program Files\Common Files&&
set CommonProgramW6432=C:\Program Files\Common Files&&
set CommonProgramFiles^(x86^)=C:\Program Files (x86)\Common Files&&
set ALLUSERSPROFILE=C:\ProgramData&&
set PUBLIC=C:\Users\Public&&
set TrackFileAccess=false&&
call C:\PROGRA~1\MICROS~2\18\COMMUN~1\VC\Auxiliary\Build\vcvars64.bat&&
C:\flutter\bin\flutter.bat test <benchmark> -d windows
```

The short Visual Studio path is intentional: the MCP-to-`cmd.exe` quoting layer did not preserve a quoted `call "C:\Program Files\...\vcvars64.bat"` command correctly. Loading `vcvars64.bat` restores the MSVC compiler/linker ordering. `TrackFileAccess=false` bypasses the earlier `GetOutOfDateItems` FileTracker call, but it does **not** suppress FileTracker use inside the MSBuild `CL` task itself.

Both native integration gates therefore still fail before the test process launches:

```text
Microsoft.Build.Utilities.FileTracker
System.ArgumentException: 不合法的路徑格式。
System.IO.Path.GetPathRoot(...)
FileTracker.InitializeCommonApplicationDataPaths()
Microsoft.Build.CPPTasks.CL.ComputeOutOfDateSources()
```

This is a Windows/MSBuild host-environment build failure, not a measured terminal regression. T7 does not report synthetic native restore/render numbers.

## Required run status

| Required run | Samples | Result |
| --- | ---: | --- |
| `flutter test integration_test/terminal_restore_benchmark.dart -d windows` | 0/5 | **BLOCKED** before test launch; Windows build failed in VS18/MSBuild `CL` -> `FileTracker.InitializeCommonApplicationDataPaths()`. Final attempt spent 1158.2 s in Windows build before exit 1. |
| `flutter test integration_test/terminal_flush_cadence_benchmark.dart -d windows` | 0/5 | **BLOCKED** by the same `CL` / FileTracker path-format exception. Cached follow-up attempt spent 240.6 s in Windows build before exit 1. |
| `flutter test test/benchmarks/terminal_production_worker_profile_benchmark.dart` | 5/scenario | **PASS** |
| `flutter test test/benchmarks/terminal_reveal_pipeline_profile_benchmark.dart` | 5/path | **PASS** |
| `flutter test test/benchmarks/terminal_streaming_render_profile_benchmark.dart` | 5 | **PASS** |

## Production worker / replica profile

All scenarios below use five measured samples.

| Scenario | Wall median / p95 | Worker roundtrip median / p95 | Replica apply median / p95 | Full repaints |
| --- | ---: | ---: | ---: | ---: |
| sustained compiler/log output | 307.76 / 2587.01 ms | 237.61 / 1699.21 ms | 24.08 / 365.03 ms | 0 |
| bursty agent output | 654.78 / 1516.86 ms | 579.63 / 1225.41 ms | 37.19 / 49.33 ms | 0 |
| full-screen TUI repaint | 243.73 / 304.25 ms | 187.10 / 280.81 ms | 4.80 / 34.85 ms | 0 |
| synchronized-update bursts | 293.41 / 506.12 ms | 258.91 / 387.64 ms | 5.90 / 42.99 ms | 0 |
| deep scrollback + ongoing output | 330.26 / 2085.51 ms | 275.14 / 1653.81 ms | 34.08 / 71.30 ms | 0 |
| hidden terminal output | 376.76 / 802.90 ms | 369.42 / 798.56 ms | 0.27 / 0.73 ms | 0 |
| reveal after large hidden backlog | 640.63 / 1790.98 ms | 263.65 / 1157.59 ms | 95.27 / 362.79 ms | 1 |
| resize storm | 10898.35 / 15541.56 ms | 9228.31 / 13528.53 ms | 521.55 / 659.88 ms | 40 |
| coalesced resize final size | 160.70 / 293.07 ms | 118.89 / 231.73 ms | 43.30 / 54.86 ms | 1 |

The ordinary output cases still avoid whole-buffer repaint. The structural cases remain distinct: reveal performs one full repaint for 8,001 rows / 1,238,175 cells, while an uncoalesced 40-resize storm performs 40 full repaints. The coalesced final-size scenario demonstrates why intermediate resize snapshots should stay collapsed.

## Packed reveal stage profile

The post-T6 packed-transfer path is the important comparison. Five measured samples produced:

```text
hidden input bytes:                    1,325,790
rows / cells:                          8,001 / 1,238,175
raw roundtrip median:                    59.39 ms
worker materialize median:               53.81 ms
isolate transfer/scheduling median:       0.82 ms
UI decode median:                       226.55 ms
replica apply median:                   156.86 ms
reveal end-to-end median:               431.22 ms
```

For comparison, the intentionally object-heavy nested reference path measured 3298.60 ms raw roundtrip, 2753.30 ms isolate transfer/scheduling, 443.50 ms UI decode, 198.57 ms replica apply, and 5698.40 ms end-to-end. That reference path confirms the packed transfer removed the previous object-graph/isolate-transfer problem; it is not a production recommendation.

Within the current packed path, the measured stage order is:

```text
UI decode/materialization 226.55 ms
> replica apply           156.86 ms
> worker materialize       53.81 ms
>> isolate transfer         0.82 ms
```

The standalone packed reveal profile is now 431.22 ms median, while the production worker-profile hidden-backlog reveal is 640.63 ms wall median on this run. The remaining latency is therefore still material enough to justify investigation, but the portable stage evidence points first at UI packed decode/materialization rather than another transport redesign.

## Streaming renderer signal

Five-sample `flutter_tester` result:

```text
writes/s median / p95:     9.2 / 13.4
flushes/s median / p95:    5.3 / 5.7
frames/s median:           4.9
build median / p95:        2.65 / 87.96 ms
raster median / p95:      81.68 / 503.41 ms
total frame median / p95: 109.32 / 516.19 ms
jank frames/sample:        16, 15, 15, 16, 15
RSS delta median / p95:    2.74 / 7.66 MiB
```

The `flutter_tester` raster tail is still present, but this cannot answer whether the native Windows compositor/GPU path confirms it because both native integration builds stop in MSBuild before app launch.

## T7 required interpretation

1. **Is native restore now a material user-visible bottleneck?**  
   Not measurable on this host. The native restore integration test never launches, so T7 does not select or reject T8d.

2. **Does the native Windows renderer confirm or reject the `flutter_tester` raster-tail signal?**  
   Not measurable on this host. The portable renderer signal remains strong, but native confirmation is still missing, so T7 does not select T8c.

3. **After packed snapshot transport, what dominates reveal?**  
   In the measurable portable pipeline, UI packed decode/materialization is largest at 226.55 ms median, followed by replica apply at 156.86 ms. Worker materialization is 53.81 ms and isolate transfer is only 0.82 ms.

4. **Is the remaining reveal worth another architecture cut?**  
   The current 431.22 ms standalone packed-pipeline median and 640.63 ms production hidden-backlog reveal wall median are still material. Portable evidence therefore makes **T8a** (keep packed storage longer / avoid rebuilding per-cell Dart objects) the leading implementation candidate, ahead of T8b. However, the T7 gate is incomplete until native Windows restore/render evidence exists, so T8 must not begin yet.

## Decision

T7 is **evidence-complete for the portable worker/reveal/render lanes but BLOCKED for the two required native Windows gates**.

- Do not begin T8 from this branch.
- Do not infer a native renderer or restore result from `flutter_tester`.
- When the Windows/MSBuild host environment can build Flutter desktop tests without the FileTracker exception, rerun the existing restore and flush integration benchmarks unchanged with five samples.
- If native evidence does not identify renderer or restore as the dominant cost, current portable evidence selects **T8a** as the next architecture experiment.
- No production terminal file was changed by T7.

