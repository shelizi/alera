# T8e P4 Native RSS / Reveal Profile Handoff

Date: 2026-09-19  
Project: Alera  
Branch: `perf/terminal-t8e-p4-native-profile`  
Worktree: `.worktrees/terminal-t8e-p4-native-profile`

## Goal

T8e P1-P3 are already complete and merged to `main`. P4 is measurement only: compare P2 soft eviction against P3 hard parser-worker eviction, measure real RSS reclaimed, and measure the reveal/restart latency cost of rebuilding a worker from the compact retained checkpoint.

Do not start another terminal architecture change until this profile is complete.

## Completed T8e baseline

- P1 `5273a98d` — keep parser worker across UI-buffer eviction.
- P2 `bc619483` — park terminal-host output and resume from absolute output cursor.
- P3 `c6790e6f856e0e87a25f7c8f63d9ad7f662d6e01` — export compact retained emulator state, close eligible parser-worker isolate, recreate from retained state on reveal.
- P1-P3 merged to main at `1f16ccaebfacd0822893548174582565bdad9887` — `merge: terminal T8e hard eviction`.

P3 validation before merge:
- worker tests: 25/25
- parser-worker focused runtime: 14/14
- full terminal runtime: 137 passed + 2 Windows/POSIX skips
- analyzer clean on touched runtime paths
- diff-check clean

P3 keeps a fail-closed boundary: unsupported/ambiguous xterm state stays on P2 soft eviction instead of approximating parser state.

## P4 comparison design

Compare identical workloads in separate fresh processes.

### Soft mode

- UI replica is evicted.
- Parser worker stays alive.
- Represents P2 memory behavior.

### Hard mode

- UI replica is evicted.
- Eligible parser worker exports compact retained state and closes.
- Reveal recreates a fresh worker from retained state.
- Represents P3 memory behavior.

Measure:
- RSS baseline
- RSS after identical hydration
- RSS after eviction
- bytes reclaimed
- reclaimed fraction of hydrated delta
- reveal samples
- reveal median / p95
- direct-worker raw roundtrip / materialize / decode timing

Do not run soft then hard inside one long-lived process for final evidence. Allocator retention can contaminate the second mode.

## Current P4 profiling-only runtime changes

Tracked WIP:

- `lib/src/features/workbench/presentation/terminal_runtime_session_handle.dart`
- `lib/src/features/workbench/presentation/terminal_runtime_parser_worker.dart`
- `lib/src/features/workbench/presentation/terminal_runtime_testing.dart`
- `test/unit/terminal_buffer_eviction_cases.dart`

Added a testing-only hard-eviction switch:
- `_parserWorkerHardEvictionEnabledForTesting`
- `setTerminalParserWorkerHardEvictionEnabledForTesting(...)`

This allows the exact same runtime/workload to run with P3 hard eviction disabled or enabled.

Focused regression already passed:

```text
flutter test --no-pub test/unit/terminal_runtime_native_test.dart --plain-name "parser worker hard eviction can be disabled for profiling"
PASS: 1/1
```

## Benchmark files

All three are currently untracked and formatted.

### Native Windows integration benchmark

`integration_test/terminal_eviction_memory_benchmark.dart`

- Uses `ProcessInfo.currentRss`.
- Intended to measure the real desktop app process.
- Emits `T8E_P4_PROFILE {...}`.
- Supports mode/session/row/reveal/settle dart-defines.
- Test body now runs successfully from MCP when the child process is given
  `SystemDrive=C:`.

### flutter_tester runtime benchmark

`test/benchmarks/terminal_eviction_memory_profile_benchmark.dart`

- Runtime-level soft/hard comparison.
- Uses the same testing-only gate.
- Emits `T8E_P4_PROFILE {...}`.
- Intended fallback when the native desktop layer cannot be built from MCP.

### Direct parser-worker benchmark

`test/benchmarks/terminal_worker_eviction_memory_profile_benchmark.dart`

- Bypasses runtime/PTY/rendering.
- Measures parser-worker + retained-checkpoint memory directly.
- Soft mode keeps workers alive.
- Hard mode exports retained state, closes workers, then recreates from retained state for reveal profiling.
- Emits `T8E_P4_WORKER_PROFILE {...}`.

Current checks:

```text
dart format --output=none --set-exit-if-changed integration_test/terminal_eviction_memory_benchmark.dart test/benchmarks/terminal_eviction_memory_profile_benchmark.dart test/benchmarks/terminal_worker_eviction_memory_profile_benchmark.dart
PASS

git diff --check --ignore-submodules=all
PASS
```

All three benchmark surfaces now have soft/hard measurements. See
`docs/t8e-p4-native-profile-results.md` for the aggregate evidence and final
P4 decision.

## Unrelated current-main build blockers found during P4

These fixes were separated from profiling and committed independently before
the P4 measurement commit.

- `f20e41ef` — `fix(l10n): remove duplicate Quick Open translation`
- `48a786c3` — `fix(quick-open): align mobile indexing API`

### 1. Duplicate zh-TW Quick Open localization key

File:
- `lib/src/app/localization/alera_localizations_zh_settings.dart`

`Quick Open` already exists in the shell localization map. The settings map added the same key, and the const maps are combined, causing Dart compilation to fail on a duplicate key.

Local fix:
- remove the settings duplicate
- keep the existing shell translation

This is a compile fix, not a P4 terminal change.

### 2. Mobile Quick Open Rust callsites missed the new API

Files:
- `rust/alera-cli/src/terminal_host/server/mobile_workspace_file_requests.rs`
- `rust/alera-core/src/workspace_files/mod.rs`
- `rust/alera-core/src/workspace_files/quick_open.rs`

Quick Open now requires:
- permanent excluded directory names at index start
- `include_gitignored` at search time

Mobile callsites still used the older signatures.

Local fix:
- export `DEFAULT_QUICK_OPEN_EXCLUDED_DIRECTORIES` from alera-core
- mobile indexing uses the same permanent exclusions
- mobile search passes `false` for include-gitignored

Do not replace this with an empty exclusion list. That would re-index dependency/build trees such as `node_modules`, `vendor`, `target`, `.dart_tool`, etc.

Validation:

```text
cargo test -p alera-core workspace_files::quick_open -- --nocapture
PASS: 13/13
```

Exact Windows sidecar build after the fix:

```text
cargo build --locked -p alera-cli --profile dev
CARGO_TARGET_DIR=C:\c\cli
PASS / exit 0
```

Only pre-existing warnings remain.

## Native Windows MSBuild blocker — resolved

The native integration benchmark now reaches and completes the test body under
MCP. The prior Visual C++ MSBuild/FileTracker failure was caused by a stripped
Windows child-process environment, not by Alera, the Rust sidecar, Visual
Studio, or the F-drive VS2022 installation.

Observed failure stack:

```text
Microsoft.Build.Utilities.FileTracker
System.TypeInitializationException
System.ArgumentException: illegal path format
System.IO.Path.GetPathRoot(...)
Microsoft.Build.Utilities.FileTracker.InitializeCommonApplicationDataPaths()
Microsoft.Build.Utilities.FileTracker..cctor()
```

### Root cause

The decisive missing variable was:

```text
SystemDrive=
```

The registry remained correct in both 32-bit and 64-bit views:

```text
HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\Shell Folders
  Common AppData = C:\ProgramData

HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders
  Common AppData = %ProgramData%
```

Under the stripped MCP environment:

- `.NET Framework Environment.GetFolderPath(CommonApplicationData)` returned
  an empty string.
- `SHGetFolderPath` and `SHGetKnownFolderPath` failed path verification.
- With `DONT_VERIFY`, both returned
  `%SystemDrive%\ProgramData`, proving the unresolved environment expansion.
- Direct filesystem access to `C:\ProgramData` succeeded.

Setting only:

```text
SystemDrive=C:
```

for the child process fixed the same VS18 `ZERO_CHECK.vcxproj` that previously
failed, then allowed the full Flutter Windows integration benchmark to build
and execute. No registry changes and no Visual Studio reinstall are required.

### Native Windows results

One directional native-process sample per mode was completed:

- soft: reclaimed `-2,232,320 B` (-2.13 MiB), reveal median
  `217.966 ms`, p95 `1011.591 ms`
- hard: reclaimed `-12,898,304 B` (-12.30 MiB), reveal median
  `235.237 ms`, p95 `866.137 ms`, 4/4 sessions hard-evicted

These native results agree with the repeated direct-worker and
`flutter_tester` evidence: hard eviction does not produce process-RSS
release in this workload.

## Historical continuation order — completed

### A. Separate the unrelated build fixes

Prefer separate commits for:
1. zh-TW duplicate Quick Open localization fix
2. mobile Quick Open Rust API compatibility fix

Then re-run:
- Quick Open Rust tests
- P4 profiling-gate focused test
- exact alera-cli sidecar build
- diff-check

### B. Run direct-worker soft/hard profiling first

Use fresh test processes. Run at least 5 samples per mode.

Soft:

```powershell
flutter test --no-pub test/benchmarks/terminal_worker_eviction_memory_profile_benchmark.dart --dart-define=ALERA_TERMINAL_EVICTION_PROFILE_MODE=soft --dart-define=ALERA_TERMINAL_EVICTION_PROFILE_SESSIONS=4 --dart-define=ALERA_TERMINAL_EVICTION_PROFILE_ROWS=1000
```

Hard:

```powershell
flutter test --no-pub test/benchmarks/terminal_worker_eviction_memory_profile_benchmark.dart --dart-define=ALERA_TERMINAL_EVICTION_PROFILE_MODE=hard --dart-define=ALERA_TERMINAL_EVICTION_PROFILE_SESSIONS=4 --dart-define=ALERA_TERMINAL_EVICTION_PROFILE_ROWS=1000
```

Capture every `T8E_P4_WORKER_PROFILE` JSON line.

If RSS signal is too small/noisy, increase rows before increasing worker count.

### C. Run runtime-level flutter_tester profiling

Soft:

```powershell
flutter test --no-pub test/benchmarks/terminal_eviction_memory_profile_benchmark.dart --dart-define=ALERA_TERMINAL_EVICTION_PROFILE_MODE=soft --dart-define=ALERA_TERMINAL_EVICTION_PROFILE_SESSIONS=4 --dart-define=ALERA_TERMINAL_EVICTION_PROFILE_ROWS=1000 --dart-define=ALERA_TERMINAL_EVICTION_PROFILE_REVEAL_RUNS=5 --dart-define=ALERA_TERMINAL_EVICTION_PROFILE_SETTLE_MS=500
```

Hard: same command with `MODE=hard`.

Capture `T8E_P4_PROFILE`.

### D. Run native Windows integration from a normal interactive shell

Soft:

```powershell
flutter test integration_test/terminal_eviction_memory_benchmark.dart -d windows --dart-define=ALERA_TERMINAL_EVICTION_PROFILE_MODE=soft --dart-define=ALERA_TERMINAL_EVICTION_PROFILE_SESSIONS=4 --dart-define=ALERA_TERMINAL_EVICTION_PROFILE_ROWS=1000 --dart-define=ALERA_TERMINAL_EVICTION_PROFILE_REVEAL_RUNS=5 --dart-define=ALERA_TERMINAL_EVICTION_PROFILE_SETTLE_MS=500
```

Hard: same command with `MODE=hard`.

Run 5 fresh-process samples per mode.

### E. Aggregate evidence

For each mode calculate process-level medians for:
- hydrated RSS delta
- evicted RSS delta
- bytes reclaimed
- reclaimed fraction
- reveal median
- reveal p95

Do not base the final memory conclusion on one run.

If P3 has no stable RSS advantage, investigate isolate teardown / allocator retention before changing terminal architecture.

If P3 clearly saves RSS but reveal becomes material, profile worker startup/import separately before changing retained-state format.

## Historical Git state at original handoff

Branch:

```text
perf/terminal-t8e-p4-native-profile
```

HEAD:

```text
1f16ccaebfacd0822893548174582565bdad9887
merge: terminal T8e hard eviction
```

At the time this handoff was first written, no P4 commit existed yet.

Tracked WIP:
- P4 profiling-only gate and focused test
- duplicate localization compile fix
- mobile Quick Open compile/API fix

Untracked WIP:
- native integration benchmark
- flutter_tester runtime benchmark
- direct worker benchmark
- this handoff document

`main` itself only has the pre-existing untracked `.worktrees/` and `docs/history-session/`.

At handoff time main reports:

```text
ahead 782, behind 127 relative to origin/main
```

No push was performed.

## Decisions to preserve

1. P4 is measurement, not a new architecture phase.
2. Compare identical soft/hard workloads in fresh processes.
3. Do not relax P3 eligibility/blockers just to improve RSS.
4. Do not claim memory savings before multi-run evidence.
5. Keep Quick Open build fixes separate from terminal profiling.
6. Do not edit Windows registry for the MCP FileTracker issue; registry values are already correct.
7. MCP Windows native builds must preserve the standard
   `SystemDrive=C:` environment; otherwise .NET Framework
   `CommonApplicationData` resolution breaks and Visual C++ FileTracker
   fails before compilation.

## Completion status

P4 profiling is complete. The authoritative evidence summary is:

`docs/t8e-p4-native-profile-results.md`

Final conclusion: P3 hard parser-worker eviction has no demonstrated RSS
advantage in the repeated direct-worker/runtime profiles, and the native
Windows desktop-process cross-check agrees with that direction. Do not start
another terminal architecture cut on the assumption that hard isolate teardown
will reduce process RSS.
