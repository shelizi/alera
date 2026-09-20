# Runtime RPC / DI refactor handoff

Updated: 2026-09-20

## Handoff target

- Worktree: `.worktrees/runtime-rpc-route-registry`
- Branch: `refactor/runtime-rpc-route-registry`
- Code checkpoint immediately before the handoff docs: `4d8ce75d` (`refactor(runtime): reuse automation agent policy handler`)
- First handoff-doc commit: `85f2f575` (`docs(runtime): hand off remaining DI refactor`)
- Worktree was clean before adding this handoff document.
- Current `main`: `e414d78c4222fff69f32a6ecf413162bb2662ebc`
- Merge base with `main`: `3d8a1f8a59c927f8f494d7808f92e810d3b234e2`
- Divergence at the code checkpoint before the handoff-doc commit: `main` had 11 unique commits and this branch had 45 unique code commits (`git rev-list --left-right --count main...HEAD`).
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

The central dispatcher is much thinner, but it still has some direct `RuntimeStore` usage. An earlier audit used `rg "self\\.runtime_store\\."`, which missed accesses split across lines (for example `self` on one line and `.runtime_store` on the next). Use `rg -n "runtime_store" ...` for the handoff audit instead.

Current categories in `requests.rs` are mixed:

- valid composition-root injection into deferred/domain handlers;
- actor-owned transaction/lifecycle persistence such as terminal idempotency receipts;
- **still-unextracted request families**, especially mobile access and SSH bootstrap-plan lookup.

Do not pursue a textual goal of zero `runtime_store` references. The remaining work below is ordered by architectural value, not grep count.

## Remaining work

### P0 - Direct request-family persistence still worth extracting

These are higher priority than the older P1 audit list because they are still visible on the central request path and have clear data/effect boundaries.

#### 0.1 Automation project policy persistence

File:

- `rust/alera-cli/src/terminal_host/server/automation_policy_requests.rs`

The branch already extracted `AutomationAgentPolicyStoreHandler`, but the project-policy side still directly uses `RuntimeStore` for:

- `set_automation_project_policy(...)` in `kind = "project"` handling;
- `automation_project_policy(...)` in effective-policy loading;
- workspace/project lookups used to evaluate policy context;
- automation-run lookup in live actor resolution.

Recommended first batch:

- add a narrow `AutomationProjectPolicyStoreHandler` for project-policy get/set only;
- add direct handler tests that do not construct a full `ServerActor`;
- keep authorization, live target identity, managed-agent actor resolution and repository declaration checks in the actor/domain flow.

Only after that batch is green should the receiver consider a separate context loader for run/workspace/project reads. Do not combine all policy dependencies into a general `AutomationDependencies` bag.

Focused validation:

```text
cargo test --manifest-path rust/alera-cli/Cargo.toml automation_policy_requests::tests::
cargo test --manifest-path rust/alera-cli/Cargo.toml terminal_host::server::requests::tests::
```

#### 0.2 Mobile access request family

File:

- `rust/alera-cli/src/terminal_host/server/requests.rs`

Still-visible direct storage/helper calls include:

- `mobile.settings.update`
- `mobile.pairing.create` / `pairing.create`
- `mobile.pairing.cancel`
- `mobile.device.list`
- `mobile.device.pair`
- `mobile.device.revoke`
- `mobile.device.delete`
- `mobile.device.rename`

The request layer currently calls `mobile_access_settings`, pairing helpers and device CRUD helpers with `&self.runtime_store` directly.

Recommended split:

- first extract a narrow mobile device/pairing persistence handler for list/rename/delete/revoke/pair/cancel;
- keep auth and broadcasts in `ServerActor`;
- keep `dispose_mobile_clients_for_device(...)` actor-owned;
- treat settings update and pairing-create separately because they cross the persisted-settings / gateway lifecycle boundary.

For `mobile.settings.update` and pairing-create, pin failure/rollback semantics before moving code. `restart_mobile_gateway()` and `apply_mobile_gateway_settings(...)` must remain in the actor/lifecycle owner unless a tested transaction coordinator replaces them.

Existing regression starting points include `mobile_relay_presence_tests.rs`, mobile allowlist tests in `requests/tests.rs`, and lifecycle wire fixtures.

#### 0.3 SSH bootstrap plan lookup

`sshTarget.list/upsert/remove` already use `SshTargetRequestHandler`, but `sshTarget.bootstrap.plan` still calls:

```text
build_ssh_bootstrap_plan(&self.runtime_store, ...)
```

This is a small, safe follow-up:

- either extend the existing SSH request handler with a plan-query capability;
- or add a narrow bootstrap-plan handler.

Keep `bootstrap.start`, `bootstrap.cancel` and `bootstrap.jobs` in `ServerActor`; they own live job lifecycle. Existing regression coverage is in `server_ssh_bootstrap_tests.rs`.

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
5. Start with **P0.1 automation project policy get/set**. The next strong candidate is the low-risk subset of **P0.2 mobile device/pairing persistence**. Keep every batch small.
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
rg -n "runtime_store" rust/alera-cli/src/terminal_host/server -g "*.rs" -g "!*test*.rs"
rg -n "runtime_store" rust/alera-cli/src/terminal_host/server/requests.rs
git diff --check
```

Remember: direct `RuntimeStore` use inside a narrow constructor-injected handler is expected. Audit for broad actor ownership, not for a global textual goal of zero `RuntimeStore` references. Also avoid the narrower `self.runtime_store.` grep as the sole audit because multiline field access can evade it.
