# Runtime RPC / DI refactor handoff

Updated: 2026-09-20

## Handoff target

- Worktree: `.worktrees/runtime-rpc-route-registry`
- Branch: `refactor/runtime-rpc-route-registry`
- Current HEAD when this handoff was written: `4d8ce75d` (`refactor(runtime): reuse automation agent policy handler`)
- Worktree was clean before adding this handoff document.
- Current `main`: `e414d78c4222fff69f32a6ecf413162bb2662ebc`
- Merge base with `main`: `3d8a1f8a59c927f8f494d7808f92e810d3b234e2`
- Divergence at handoff: `main` has 11 unique commits and this branch has 45 unique commits (`git rev-list --left-right --count main...HEAD`).
- Do not merge blindly. Reconcile current `main` before final integration and re-run current-head tests afterwards.

## Goal and architecture rules

Continue reducing `ServerActor` as a service locator without introducing a DI framework.

Follow `skills/alera-di-architecture/SKILL.md` and its review checklist:

- constructor-inject the smallest useful capability;
- keep pure functions direct instead of wrapping everything in interfaces;
- keep authentication, broadcasts, live client/session state, PTY/process ownership, cancellation and transaction/lifecycle boundaries in the owning actor;
- let store/query/mutation handlers own direct `RuntimeStore` access where that is their explicit dependency;
- do not create a giant `Dependencies` / `AppServices` bag;
- do not re-centralize agent-specific hook policy through a generic DI abstraction.

## What is already done

The branch has already moved the central request surface a long way toward explicit handlers and typed routing.

### Route policy / deferred execution

The branch includes typed route-policy work for serialized mutations, deferred reads/writes, coalesced reads, prompt image/file operations, workspace file operations, CLI registration, agent quota, AI text/dictation, and handler families. The first commits in the branch include:

- `56d4b0ef` centralize request route policy
- `4aaec3c5` type serialized mutation routes
- `4d1b6d16` inject deferred read handler
- `d7a4f050` unify deferred read execution
- `adad278e` inject project registration handler
- `de00ce38` type coalesced sidebar route
- `cd443ace` / `957bd956` / `0fbf97ab` type prompt/workspace-file operations
- `837ef6f9` / `672b9ee0` type CLI registration and agent quota operations
- `2ffc728b` / `d7c78efb` type AI text and dictation deferred jobs
- `3528377f` / `84971436` type workspace and terminal deferred jobs
- `4ab46f05` type conditional deferred routes
- `f8a8049c` / `0041df92` / `e6ed5729` centralize request families/cloud classification and carry handler family in route policy

### Store/domain handlers already extracted

Do not redo these. They already have constructor-injected handlers or store helpers on this branch:

- workspace sections: `073d9189`
- project rename: `eb34d6e4`
- workspace requests: `16cadbe0`
- project store/config: `3ae97b04`
- workspace artifacts: `aca37f59`
- workspace relations: `4fc64d4d`
- workspace tags: `c606bdfe`
- cascade preview: `c96be610`
- runtime metadata: `ce4df261`
- workspace tab list/find: `6fd7bb8f`
- tab rename: `ed14eaf1`
- tab upsert pre-read: `aa049eac`
- workspace activity: `f9ff5e08`
- workbench view prefs: `43c28f5b`
- workspace rename / pinning: `2196f6a6`, `150f3ee6`
- automation templates/tags/catalog transfer: `ce3451a0`, `e0f5a548`, `9a8fb1c2`
- configuration store handler: `8c0b754c`
- SSH target CRUD handler: `7e261d09`
- agent profile catalog/mutations: `d7c6f753`, `3914cb5e`
- runtime settings store handler: `3e5e3854`
- automation agent policy handler: `7c71823e`, `4d8ce75d`

At current HEAD, `requests.rs` has no direct `RuntimeStore` I/O. The only `self.runtime_store` references in the central dispatcher are the two explicit `clone()` calls used to construct deferred handlers. That is a valid composition-root boundary and does not need to be removed just to reach zero textual references.

## Remaining work

### P1 - Small, safe follow-up batches

These are the best next candidates because the persistence/effect boundary is visible and can be tested without moving lifecycle ownership.

#### 1. Agent hook settings query

File:

- `rust/alera-cli/src/terminal_host/server/agent_hook_events.rs`

Current direct store access:

- `self.runtime_store.agent_status_hook_settings().await`

Suggested direction:

- extract a narrow query/helper for hook settings, or inject only that capability into the hook-event logic;
- keep each agent adapter/policy where it is; do **not** create a generic agent-policy service that re-centralizes Claude/Codex/etc. behavior;
- add a direct unit test for settings lookup/fallback behavior without constructing a full `ServerActor` where practical.

#### 2. Agent-title tab persistence

Files:

- `agent_title_events.rs`
- `agent_title_generation.rs`

Current direct store operations include `find_workspace_tab` and `upsert_workspace_tab` around title state transitions.

Suggested direction:

- introduce/reuse a narrow workspace-tab persistence capability for `find` + `upsert` only;
- leave generation jobs, cancellation, stale-generation guards, broadcasts, conversation/revision checks and title policy in `ServerActor`;
- preserve current failure semantics: several writes are intentionally best-effort.

#### 3. Agent-profile launch tab persistence

File:

- `agent_profile_launch_requests.rs`

Current direct store operations include tab cleanup, lookup and update during launch/prompt delivery.

Suggested direction:

- share a narrow tab persistence helper with the agent-title work if the capability set remains small (`find`, `upsert`, `remove`);
- keep terminal/session creation, rollback order, prompt delivery and launch lifecycle actor-owned;
- test rollback/cleanup ordering before moving any code.

#### 4. Small global effects still visible in `requests.rs`

Current examples:

- `configure` toggles Sentry through `diagnostics::sentry_reporting::set_enabled(...)` and then applies live host config;
- `shellEnvironment.reload` calls `reload_login_shell_environment()` directly.

Suggested direction:

- only extract these if substitution materially improves tests;
- prefer tiny effect ports/adapters over a general host-services bag;
- keep `apply_config` and host lifecycle ownership in `ServerActor` unless there is a concrete testability benefit to splitting it.

### P2 - Medium-risk lifecycle persistence

Do these only after P1, one small batch at a time.

#### 5. Terminal/session tab persistence

Files with remaining direct store access:

- `terminal_session_requests.rs`
  - find/update tab during session request flow
- `terminal_spawn.rs`
  - list workspaces/tabs for recovery
  - remove failed/stale tabs
  - upsert one-shot payload cleanup
  - find tab during spawn/lifecycle work

Suggested direction:

- extract only the persistence/query capability;
- keep the session map, PTY/process owner, spawn/restart sequencing, rollback, one-shot prompt/command semantics and broadcasts in the actor;
- do not split the underlying `RuntimeStore` pool or process owner;
- add direct helper tests plus existing terminal lifecycle regression tests after every batch.

#### 6. AI assist spawned-task store ownership

Files:

- `ai_assist_requests.rs`
- `ai_assist_speech_message.rs`

These currently clone `RuntimeStore` into async work rather than doing central-dispatcher store I/O.

Suggested direction:

- treat this as lower priority than the agent/terminal persistence items;
- if refactored, inject a feature-scoped query/store capability into the async worker rather than passing the full actor;
- preserve cancellation/task ownership and avoid adding a broad AI service container.

### P3 - High-risk / defer until there is a concrete pain point

#### 7. Automation dispatch/run lifecycle and orchestration/coordinator flows

There are still direct store reads/writes in actor-owned automation/orchestration modules. Many are coupled to dispatch state, pending markers, retries, admission, task lifecycle and delivery ordering.

Do **not** mechanically replace every `self.runtime_store` occurrence.

If continuing here:

- first identify a data-only slice that can become a narrow handler/context;
- preserve transaction and lifecycle boundaries;
- avoid a giant `AutomationDependencies`/`OrchestrationDependencies` bag;
- add state-machine regression tests before moving writes or retry markers.

## Intentionally actor-owned for now

These should not be treated as unfinished simply because they are still in `requests.rs` or `impl ServerActor`:

- host shutdown / restart / promote-persistent lifecycle;
- terminal create/attach/restart, raw write/read/resize, reclaim, detach, terminate;
- live terminal driver/session maps and PTY/process ownership;
- mobile gateway replacement/bind lifecycle;
- client authentication, per-client filtering/projection and broadcast delivery;
- runtime mutation queue/admission ordering;
- orchestration/automation in-memory lifecycle state and cancellation ownership.

The goal is not to make `ServerActor` empty. The goal is to stop it from being the place where unrelated persistence/domain logic is implemented.

## Validation already observed during this refactor

For the earlier handler batches through `aa049eac`, focused tests were repeatedly run and passed, including:

- runtime metadata direct handler tests;
- workspace tab query/rename direct handler tests;
- `terminal_host::server::requests::tests::` (16/16 in the observed runs);
- `rustfmt` / format checks;
- `git diff --check`.

The branch subsequently advanced from `aa049eac` to `4d8ce75d` with additional DI commits already present when this handoff was prepared. Do not assume that the above observed test runs are sufficient evidence for the **current** HEAD. The receiver should re-run current-head validation before integration.

## Receiver start sequence

1. Open `.worktrees/runtime-rpc-route-registry` on `refactor/runtime-rpc-route-registry`.
2. Verify HEAD and clean status; if HEAD differs from the value above, read the newer commits before changing anything.
3. Read `skills/alera-di-architecture/SKILL.md` and `references/review-checklist.md`.
4. Inspect `git log --oneline main..HEAD` and current `main` divergence.
5. Start with **P1.1 agent hook settings** or **P1.2 agent-title tab persistence**; keep the batch small.
6. For each batch:
   - add a direct handler/helper unit test where possible;
   - run the focused module tests;
   - run `terminal_host::server::requests::tests::` when request dispatch changes;
   - run `rustfmt` on touched Rust files;
   - run `git diff --check`;
   - commit before starting the next batch.
7. Before merging to `main`, reconcile the 11+ main-side commits that were already outside this branch at handoff time, then re-run the relevant broader Rust regressions.

## Useful audit commands

```text
git status --short --branch
git log --oneline --decorate main..HEAD
git rev-list --left-right --count main...HEAD
rg -n "self\.runtime_store\." rust/alera-cli/src/terminal_host/server -g "*.rs" -g "!*test*.rs"
rg -n "self\.runtime_store\." rust/alera-cli/src/terminal_host/server/requests.rs
git diff --check
```

Remember: direct `RuntimeStore` use inside a narrow constructor-injected handler is expected. Audit for broad actor ownership, not for a global textual goal of zero `RuntimeStore` references.
