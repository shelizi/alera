# Alera Heavy-Operation / Performance Parallel Work Plan

Status: active coordination plan
Date: 2026-09-20
Committed baseline reviewed: `main` @ `3ba4a9ffc417903db2fcebeef9fd6e530c23f241`

This revision re-syncs the plan against the current committed code after C6, T7, T8d, T8e P1-P3, A3V, and the recent Git diff / Quick Open work. It replaces the older allocation that still listed C6/T7/A3V as active.

The coordination rule for this phase is: **finish evidence closure first, then optimize only the paths that still have a measured user-visible or resource cost.**

## 1. Repository snapshot and coordination constraints

At re-audit time:

```text
branch: main
HEAD: 3ba4a9ffc417903db2fcebeef9fd6e530c23f241
upstream divergence: ahead 803 / behind 129
```

The primary main worktree is currently busy with unrelated staged/unstaged localization and workbench changes plus generated/build artifacts. In particular, multiple localization, Git diff/history, workbench sidebar/editor files are staged, and `workspace_git_diff_surface_editable.dart` plus a localization test have both staged and unstaged changes.

Therefore:

- do not reset, clean, restage, merge, or commit through the primary worktree as part of this performance planning batch;
- new optimization work must use dedicated worktrees from an agreed committed `main`;
- the plan itself is updated in an isolated docs worktree so it does not disturb the active main index;
- do not push unless explicitly requested.

The old Ghostty experiment remains quarantined in `alera/.worktrees/terminal-parser-worker-phase1`. Do not use it as a production terminal base.

## 2. Completed optimization ledger

### 2.1 C6 retained parser scheduling: COMPLETE

C6 is closed on main.

Relevant commits include:

```text
24053f1d perf: profile retained parser cold-start
a24f546b perf: cancel stale retained parses
b04e168e perf: defer large-file retained parse
ab6bdefe test: harden retained parse lifecycle
6df87dbc perf: close C6 Windows after-profile
986b5a74 Merge branch ''perf/editor-native-parse-scheduling''
```

Windows after-profile completed the full Rust/Dart/TypeScript x 2k/20k/50k/100k matrix plus rapid replacement. At 100k lines, native open -> first useful frame medians remained roughly 221-279 ms while retained syntax readiness remained 2.45-5.21 s. The UI/open path is therefore decoupled from the multi-second parse.

Do not open another parser architecture phase without new evidence.

Sources:

- `docs/editor-retained-parser-c6p-profile.md`
- `docs/editor-retained-parser-c6-handoff.md`

### 2.2 A3 / A3V runtime overlay reuse: COMPLETE

A3 repeated-unchanged reuse and the production FRB validation are complete.

Relevant commits:

```text
24353b5b perf(agent): reuse unchanged runtime overlays
6f4342cd test(agent): close A3 production FRB validation
```

Production FRB repeated-unchanged medians for 20 / 500 / 2000 files were approximately 8.532 / 36.265 / 550.038 ms, with zero mutation on reuse. No A4 work is justified by the current evidence.

### 2.3 T7 / T8d restore redesign: COMPLETE

T7 measured native restore at roughly 9.90 s median / 12.44 s p95 and selected direct worker hydration.

Relevant commits:

```text
b058d803 docs(perf): close T7 native terminal gate
b8144755 perf(terminal): hydrate snapshots directly in worker
2ed0b8a1 perf(terminal): profile post-T8d restore phases
```

Post-T8d clean native runs reached about 1.60 s median framework-ready, with all ten clean samples inside the 3 s gate. Worker-side snapshot parse remained the dominant restore phase, which led to T8e.

Sources:

- `docs/t7-terminal-post-t6-native-profile-report.md`
- `docs/t8d-terminal-direct-snapshot-hydration-report.md`
- `docs/t8-post-t8d-native-profile-report.md`

### 2.4 T8e P1-P3 structured eviction/resume: COMPLETE AND MERGED

Relevant commits:

```text
5273a98d perf(terminal): retain worker state across eviction
bc619483 perf(terminal): resume parked sessions from output cursor
c6790e6f perf(terminal): hard-evict eligible parser workers
1f16ccae merge: terminal T8e hard eviction
```

Current production behavior:

- P1: hidden parser-worker sessions can evict the duplicate UI replica buffer while retaining PTY + authoritative worker state;
- P2: parked sessions capture an absolute output cursor and resume only the retained host-ring delta when possible;
- P3: eligible parser workers can export compact structured emulator state, close the isolate, and recreate from retained state;
- unsafe/ambiguous emulator state fails closed to P2 soft eviction.

Important current code fact: after UI-buffer eviction, production automatically schedules P3 hard eviction for eligible parser-worker sessions.

Source: `docs/t8e-terminal-soft-eviction-report.md`.

### 2.5 Recent Git diff stabilization: LANDED, PERFORMANCE EVIDENCE STILL INCOMPLETE

```text
95c23cd6 perf(git): stabilize diff rendering and source control refresh
```

The recent cut already removes several obvious UI hot paths:

- no parent diff-tree rebuild on every editable keystroke;
- line counting avoids splitting the whole string repeatedly;
- width measurement lays out only the widest candidate instead of every line;
- full-file render is bounded at 2 MiB and editable full-file mode at 512 KiB;
- large full-file previews fall back to diff-only;
- full-file blob loading is skipped when the active content mode does not need it;
- single-column diff rendering uses a lazy list;
- source-control refresh behavior was tightened.

However, there is still no dedicated Git diff/stage performance benchmark. The recent fixes are correctness/perf motivated but not yet backed by a reproducible heavy-repo profile.

### 2.6 Quick Open indexing changes: LANDED, RESOURCE PROFILE NOT YET CLOSED

Recent Quick Open work now builds one index containing normal files plus Git-ignored entries, filters ignored entries at query time by default, permanently prunes common dependency/build directories, and uses separate normal/ignored index budgets.

This is the intended UX, but it increases the importance of measuring index build time and retained-memory cost on a large monorepo rather than assuming the larger index is free.

## 3. T8e P4 evidence: COMPLETE ON BRANCH, NOT MERGED TO MAIN

Branch/worktree:

```text
branch: perf/terminal-t8e-p4-native-profile
HEAD:   6a1fb7d40991fb52908b5b9626b6b241ed2fb2cb
worktree: alera/.worktrees/terminal-t8e-p4-native-profile
```

P4 measurement commits:

```text
c478b985 perf(terminal): profile T8e hard eviction
6a1fb7d4 docs(terminal): finalize T8e P4 native profile
```

P4 is measurement-only and compares P2 soft eviction with P3 hard parser-worker eviction.

### Direct parser-worker fresh-process median

| Metric | Soft | Hard |
| --- | ---: | ---: |
| hydrated RSS delta | 17.94 MiB | 18.56 MiB |
| RSS reclaimed after eviction | 0 MiB | -4.86 MiB |
| reveal median | 69.448 ms | 68.057 ms |
| reveal p95 | 89.281 ms | 96.341 ms |

### Runtime-level flutter_tester median

| Metric | Soft | Hard |
| --- | ---: | ---: |
| hydrated RSS delta | 59.81 MiB | 63.34 MiB |
| RSS reclaimed after eviction | -0.19 MiB | -9.51 MiB |
| reveal median | 17.741 ms | 23.280 ms |
| reveal p95 | 30.020 ms | 33.688 ms |

One hard sample had a severe 834.344 ms median / 939.382 ms p95 reveal outlier.

### Native Windows directional cross-check

| Metric | Soft | Hard |
| --- | ---: | ---: |
| hydrated RSS delta | 51.48 MiB | 57.39 MiB |
| RSS reclaimed after eviction | -2.13 MiB | -12.30 MiB |
| hard-evicted sessions | 0 | 4 |
| reveal median | 217.966 ms | 235.237 ms |
| reveal p95 | 1011.591 ms | 866.137 ms |

The workers really close in hard mode, but process RSS does not fall. The available evidence therefore **does not support P3 hard eviction as an RSS optimization**.

This does not prove hard eviction is useless: it may still reduce live Dart heap, isolate count, ports, GC roots, or idle scheduling cost that the process-RSS metric cannot see. Those are the next measurements if we want to justify keeping hard eviction as the default hidden-session policy.

P4 also found the Windows native-build root cause: stripped child processes were missing `SystemDrive`. Setting `SystemDrive=C:` fixes .NET Framework CommonApplicationData resolution and Visual C++ FileTracker. No Visual Studio reinstall or registry change is required.

Authoritative P4 evidence currently lives in the P4 worktree:

- `docs/t8e-p4-native-profile-results.md`
- `docs/t8e-p4-native-profile-handoff.md`

## 4. Current open optimization targets

### S2 - startup still waits for CodeForge before runApp

Current committed `lib/main.dart` still does:

```text
await RustLib.init()
await code_forge.RustLib.init()
...
runApp(...)
```

S1 already showed that simply starting both FRB init futures together is effectively noise (~0.4%). C6 is now complete, so the prior parser-readiness coordination blocker is gone.

The next justified startup question is whether CodeForge can move to:

1. post-first-frame prewarm; or
2. lazy initialization immediately before first editor/native use.

### T9 - hard-eviction resource policy is unproven

P4 disproved the expected process-RSS win. Because production currently hard-evicts automatically after UI-buffer eviction, we should now measure the resources that isolate teardown can actually change before keeping that extra state-export/import complexity as the default policy.

### G3 - Git diff/stage freeze path lacks a benchmark gate

Recent code addressed concrete freeze/rebuild issues, but there is no repeatable benchmark covering large full-file diff, side-by-side edit/selection, or stage/unstage refresh. Re-profile before adding more architecture.

### Q1 - larger Quick Open index lacks a resource budget

Quick Open now intentionally retains Git-ignored entries in the session index. Add a large-repo benchmark for build time, query time, and memory before increasing caps or moving more index work across boundaries.

### W1 - native Windows benchmark environment is fragile

Multiple performance phases lost time to the same stripped Windows child-process environment. The minimum required invariant is now known: native Windows benchmark/build children need a valid `SystemDrive` (and should sanity-check the standard common-program/application-data paths before launching Flutter/MSBuild).

This is developer-infrastructure work, not application runtime optimization, but it directly improves the reliability and throughput of every Windows performance gate.

## 5. Next-phase package summary

| ID | Priority | Work package | Start now? | Parallel ownership |
| --- | --- | --- | --- | --- |
| P4I | P0 closure | Selectively integrate T8e P4 measurement commits/docs | **Yes when main index is safe** | integration only |
| S2 | P0 | CodeForge post-first-frame / lazy-init A/B | **Yes** | startup/readiness |
| T9 | P1 gate | Hard-eviction live-resource + policy profile | **Yes** | terminal profile only |
| G3 | P1 gate | Git diff/stage heavy-path profile | **Yes** | Git benchmark/profile |
| Q1 | P2 gate | Quick Open index/search memory + latency profile | **Yes** | Quick Open benchmark/profile |
| W1 | P2 infra | Normalize Windows native benchmark environment | **Yes** | tooling only |
| S3 | evidence only | Production startup ordering change | **No; after S2** | startup implementation |
| T10 | evidence only | Terminal policy/allocator follow-up | **No; after T9** | terminal implementation |
| G4 | evidence only | Git diff architecture follow-up | **No; after G3** | Git implementation |
| Q2 | evidence only | Quick Open index architecture follow-up | **No; after Q1** | search implementation |

The four main performance lanes that can run immediately and independently are:

```text
S2 || T9 || G3 || Q1
```

W1 can run in parallel as infrastructure support. P4I is an integration closure, not another implementation lane.

## 6. P4I - integrate the completed T8e P4 evidence

Do not merge the entire old P4 branch blindly into current main. The branch history also contains earlier Quick Open compatibility fixes that current main has since evolved past.

Preferred integration when the primary main index is safe:

1. re-check current main;
2. selectively cherry-pick/rebase the measurement-only P4 commits `c478b985` and `6a1fb7d4`, resolving only genuine drift;
3. rerun the focused eviction gate / benchmark compile checks;
4. keep the final P4 conclusion unchanged unless current-main reruns materially disagree;
5. update this plan to mark P4I complete.

Do not perform this integration while the current main index contains unrelated staged work.

## 7. S2 - CodeForge post-first-frame / lazy-init A/B

**Priority: P0. Highest remaining app-startup opportunity.**

Suggested branch/worktree:

```text
perf/codeforge-lazy-init-profile
```

### Baseline variants

Use fresh processes and at least five samples per variant:

1. current baseline: Alera Rust + CodeForge Rust both awaited before `runApp`;
2. post-first-frame CodeForge prewarm;
3. lazy CodeForge init immediately before first editor/native use.

### Required measurements

- process start -> first Flutter frame;
- process start -> first interactive workbench frame;
- first editor open, warm CodeForge;
- first editor open, cold/lazy CodeForge;
- CPU contention around the first two frames;
- whole-app RSS/working set;
- immediate-editor-open race/readiness behavior;
- startup failure behavior if CodeForge init fails after `runApp`.

Reuse `AleraPerformanceTrace` and existing performance tooling where practical.

### Implementation boundary

Do not scatter `RustLib.init()` checks through widgets. Introduce one readiness owner/future so every first-use path shares the same idempotent initialization.

C6 is already complete, so S2 no longer waits on a parser-contract change. The editor must simply await CodeForge readiness before the first native CodeForge call.

### S2 decision

Keep a production S3 change only if the first-frame improvement is repeatable and the cold first-editor penalty/readiness complexity is acceptable. Otherwise keep the current pre-`runApp` initialization.

## 8. T9 - terminal hard-eviction resource/policy profile

**Priority: P1 evidence gate. No terminal architecture changes in T9.**

Suggested branch/worktree:

```text
perf/terminal-hard-eviction-policy-profile
```

P4 already answered the process-RSS question: hard eviction does not reclaim stable RSS in the tested environment.

T9 should instead measure the resources that isolate teardown can actually release.

### Scenarios

Use identical fresh-process workloads for P2 soft and P3 hard modes with at least:

- 4 / 16 / 32 hidden parser-worker sessions;
- moderate and deep scrollback/checkpoint sizes;
- settle windows such as immediate, 5 s, and 30 s after eviction;
- five fresh-process samples per meaningful decision case.

### Metrics

Where available, record:

- process RSS / working set;
- Dart VM heap used/capacity before hydration, after hydration, after eviction, after settle;
- isolate / isolate-group count;
- live object/allocation profile for terminal worker/row/cell structures;
- GC count/time around eviction and reveal;
- active receive ports / timers / task sources if observable;
- idle CPU consumption with hidden sessions parked;
- reveal median/p95 and severe-tail frequency;
- hard-eviction eligibility/blocker rate under representative shell/TUI traces.

### T9 decision tree

```text
hard eviction materially reduces live heap / idle resource cost
and reveal tail remains acceptable
  -> keep P3 as the default eligible-session policy

hard eviction has no meaningful live-resource benefit
  -> prefer P2 soft eviction by default
  -> retain P3 only behind explicit pressure/experimental policy, or remove it later

live Dart heap falls but process RSS stays retained
  -> allocator/VM retention is the target
  -> do not rewrite terminal state architecture again

hard reveal/tail regressions outweigh resource savings
  -> default to soft eviction even if some heap is released
```

Do not start T10 until this gate decides what problem actually remains.

## 9. G3 - Git diff/stage heavy-path profile

**Priority: P1 evidence gate.**

Suggested branch/worktree:

```text
perf/git-diff-stage-profile
```

The recent `95c23cd6` cut is directionally correct but lacks a reproducible performance report.

### Scenarios

Profile at least:

- single 100k-line text file with sparse changes;
- long-line file stressing horizontal layout/selection;
- file near the 512 KiB editable boundary;
- file near / above the 2 MiB full-file-render boundary;
- 10 / 100 / 1000 changed-file result sets;
- side-by-side full-file view;
- single-column diff-only view;
- editable selection/caret movement on long lines;
- stage/unstage one file while a large diff is open;
- repeated source-control refresh after staging.

### Metrics

- diff request -> first useful frame;
- full-file blob/decode wall time;
- row/projection build time;
- frame build/raster p95;
- UI isolate stall/jank;
- source-control refresh wall time;
- stage/unstage action -> stable refreshed UI;
- peak/steady memory for full-file contents and editable documents.

### G3 decision tree

```text
blob/decode dominates
  -> bound/defer full-file materialization further

row/projection/layout dominates
  -> improve virtualization / retained projection

stage/unstage refresh dominates
  -> incremental source-control invalidation / narrower refresh

no material hot path remains
  -> stop Git diff architecture work
```

Do not jump to a Rust/native diff renderer without evidence. The current bottleneck may still be Flutter layout/materialization rather than diff computation.

## 10. Q1 - Quick Open index/search resource profile

**Priority: P2 evidence gate.**

Suggested branch/worktree:

```text
perf/quick-open-index-profile
```

The new UX intentionally indexes normal + Git-ignored files once and filters ignored entries at query time. Validate the cost before increasing scope further.

### Scenarios

Use synthetic/real large repositories with:

- 10k / 50k normal entries;
- a large ignored tree up to the current ignored-entry budget;
- common pruned dependency trees;
- include-gitignored off/on queries;
- prefix, fuzzy, deep-path, and high-match-density queries;
- repeated open/search in the same index session.

### Metrics

- initial index wall time;
- time to first searchable result;
- retained index bytes / process memory proxy;
- query median/p95;
- normal-vs-ignored result filtering cost;
- index rebuild/invalidation cost;
- UI responsiveness while indexing.

Only create Q2 if the expanded session index is materially expensive.

## 11. W1 - Windows native benchmark environment guardrail

**Priority: P2 infrastructure.**

Add or extend a reusable development/performance helper instead of rediscovering the same host issue per phase.

The helper should:

- ensure `SystemDrive` is valid before native Flutter/MSBuild runs;
- validate `ProgramData`, common application-data and common-program-files resolution;
- report the selected `cl.exe`, `link.exe`, MSBuild/CMake toolchain;
- fail early with a precise environment diagnostic rather than a late FileTracker stack trace;
- avoid globally changing registry/VS installation state.

Prefer integrating with existing `tool/development/setup_windows.ps1` or performance tooling instead of creating a duplicate one-off script.

## 12. Deferred / stopped lanes

Keep these stopped unless fresh evidence changes the decision:

- another C7 parser/editor architecture phase;
- native regex engine;
- Git history F2 native projection;
- Workspace Search G2 native paging/projection under the current result cap;
- Ghostty production terminal backend;
- T8a/T8b/T8c protocol/renderer work without a new T9/T10 measurement target.

## 13. Parallel staffing

### Four primary performance workers

```text
Person 1 -> S2  CodeForge startup A/B
Person 2 -> T9  terminal hard-eviction policy profile
Person 3 -> G3  Git diff/stage profile
Person 4 -> Q1  Quick Open index/search profile
```

### Fifth worker

```text
Person 5 -> W1  Windows native benchmark environment helper
```

### Integration owner

P4I should be handled by whoever owns terminal integration after the main working tree is safe. It is not a separate architecture worker.

### Conflict rules

- S2 touches startup/readiness; keep it out of current unrelated UI/localization work.
- T9 should be benchmark/test/documentation only while profiling.
- G3 should begin benchmark-first and avoid production Git diff edits until the current main Git-diff/UI changes are committed and its profile selects a target.
- Q1 should begin benchmark-first; do not change index semantics/caps during measurement.
- W1 is tooling-only and independent.
- T10/G4/Q2/S3 each start only after their corresponding evidence gate.

## 14. Recommended order

```text
now:
  S2 || T9 || G3 || Q1 || W1

when primary main index is safe:
  P4I selective evidence integration

after evidence:
  S2 -> optional S3
  T9 -> optional T10 or soft-eviction default simplification
  G3 -> optional G4
  Q1 -> optional Q2
```

## 15. Definition of done

Each performance package must, where applicable, include:

- exact committed baseline;
- TDD/focused correctness coverage;
- five-sample evidence for performance claims;
- UI-isolate/frame/RSS/heap/worker evidence appropriate to the subsystem;
- stale/generation/lifetime safety;
- explicit fallback/error behavior;
- no accidental full-document/history reconstruction in a hot path;
- no unnecessary generated-binding churn;
- focused analyzer/lints/tests;
- `git diff --check`;
- exact-path commit(s);
- handoff with measured before/after values, known limitations, and the next decision gate.

## 16. Top-line next phase

```text
S2  Startup: move CodeForge off pre-runApp only if A/B proves a real win

T9  Terminal: decide whether hard eviction has any real resource benefit
    beyond process RSS; otherwise prefer the simpler soft-eviction policy

G3  Git diff/stage: profile the recently stabilized heavy UI path before
    another architecture cut

Q1  Quick Open: verify the larger normal+ignored index fits latency/memory
    budgets

W1  Make native Windows performance gates reproducible (SystemDrive/toolchain)

P4I Integrate completed P4 evidence when the active main index is safe
```

The current highest-value runtime opportunities are **S2 startup latency** and **T9 terminal policy simplification/resource validation**. The next Git/Quick Open work should be measurement-first, because recent code has already removed several obvious hot paths.
