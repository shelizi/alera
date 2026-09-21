# Runtime RPC / DI refactor handoff

Updated: 2026-09-20

## 1. Handoff target / exact checkpoint

Continue from this exact state:

- Worktree: `.worktrees/runtime-rpc-route-registry`
- Branch: `refactor/runtime-rpc-route-registry`
- Code checkpoint immediately before this handoff-doc update: `396aae10e13bd9223b5df4be6a71a86a144db0ff`
  - `refactor(runtime): reuse workspace tab store for agent profile launch`
- Current `main`: `e414d78c4222fff69f32a6ecf413162bb2662ebc`
  - `fix git diff encoding detection`
- Divergence at this handoff:
  - `main` unique commits: **11**
  - DI branch unique commits before this handoff-doc commit: **53**
  - command: `git rev-list --left-right --count main...HEAD`
- Worktree was clean before this handoff document was updated.
- Do **not** blindly merge to `main`. Reconcile current `main` first, then re-run current-head Rust regressions.

Current main-side commits outside this branch are mostly Git diff/UI, localization, release and T8e/P4 work:

```text
e414d78c fix git diff encoding detection
133d3ab3 fix(diff): sync read-only side-by-side scrolling
daf4eae4 fix(diff): restore horizontal scrolling
43c5da16 fix(diff): isolate side-by-side text selection
8c703a5c feat(i18n): add Simplified Chinese and Japanese
56d3f607 Merge branch 'main' into perf/terminal-t8e-p4-integration
0da918a1 build(release): use rsync for remote sync
3b858a53 docs(perf): close P4 evidence integration
df622481 docs(terminal): finalize T8e P4 native profile
c4237748 perf(terminal): profile T8e hard eviction
76bad3b5 docs(perf): replan post-T8e optimization lanes
```

The subjects show little obvious overlap with the Rust RPC files, but still verify the actual merge result instead of assuming conflict-free integration.

## 2. Goal and non-negotiable architecture rules

The goal is **not** to eliminate every `RuntimeStore` reference and it is **not** to make `ServerActor` empty.

The goal is to stop `ServerActor` from acting as a service locator for unrelated persistence/domain logic while keeping real ownership boundaries explicit.

Follow:

- `skills/alera-di-architecture/SKILL.md`
- `skills/alera-di-architecture/references/review-checklist.md`

Rules that matter for this refactor:

1. Prefer constructor injection.
2. Inject the smallest useful persistence/effect capability.
3. Do not add a DI framework.
4. Do not create `AppServices`, `Dependencies`, or another giant service bag.
5. Keep pure parsing/mapping/validation functions direct unless substitution is useful.
6. Keep auth, broadcast, client projection, live session state, PTY/process ownership, cancellation and lifecycle ordering in the actor/owner that actually owns them.
7. Keep SQLite pool / `RuntimeStore` transaction ownership shared; splitting handlers does not mean splitting the underlying store.
8. Direct `RuntimeStore` access **inside a narrow constructor-injected handler** is expected.
9. Do not re-centralize Claude/Codex/other agent-specific behavior through a generic agent-policy service.
10. Every small refactor batch should be directly unit-testable without constructing the whole `ServerActor` where practical.

## 3. What this branch already completed

Do not redo these areas.

### 3.1 Typed request routing / request policy

The branch centralizes request routing and replaces large string-based branching with typed route policy for deferred/coalesced/specialized operations.

Key commits:

- `56d4b0ef` centralize request route policy
- `4aaec3c5` type serialized mutation routes
- `4d1b6d16` inject deferred read handler
- `d7a4f050` unify deferred read execution
- `adad278e` inject project registration handler
- `de00ce38` type coalesced sidebar route
- `cd443ace` type prompt image operations
- `957bd956` type prompt file operations
- `0fbf97ab` type workspace file operations
- `837ef6f9` type CLI registration operations
- `672b9ee0` type agent quota operations
- `2ffc728b` type AI text deferred jobs
- `d7c78efb` type AI dictation deferred jobs
- `3528377f` type workspace deferred jobs
- `84971436` type terminal deferred jobs
- `4ab46f05` type conditional deferred routes
- `f8a8049c` centralize request handler families
- `0041df92` centralize cloud request classification
- `e6ed5729` carry handler family in route policy

### 3.2 Store/domain handlers already extracted

Already constructor-injected or otherwise routed through narrow store handlers:

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
- workspace rename/pinning: `2196f6a6`, `150f3ee6`
- automation templates/tags/catalog transfer: `ce3451a0`, `e0f5a548`, `9a8fb1c2`
- configuration store handler: `8c0b754c`
- SSH target CRUD handler: `7e261d09`
- agent profile catalog/mutations: `d7c6f753`, `3914cb5e`
- runtime settings store handler: `3e5e3854`
- automation agent policy handler: `7c71823e`, `4d8ce75d`
- automation project policy get/set: `736757fd`
- mobile device/pairing CRUD: `cea338d0`
- SSH bootstrap plan query: `339675e6`
- agent hook settings query: `f6b8fd0b`
- agent-title workspace-tab persistence: `83460746`
- agent-profile launch workspace-tab persistence: `396aae10`

### 3.3 Current central dispatcher shape

`requests.rs` is now mostly a composition / ownership layer.

Valid remaining examples include:

- cloning `RuntimeStore` into `DeferredReadRequestHandler` / `ProjectRegistrationRequestHandler`;
- constructing `MobileDevicePairingRequestHandler`, `ProjectStoreRequestHandler`, `WorkspaceRequestHandler`, `WorkspaceTabStoreHandler`, `WorkspaceArtifactRequestHandler`, `WorkspaceRelationRequestHandler`, `SshBootstrapPlanRequestHandler`, runtime metadata/settings handlers;
- keeping client auth, mobile-vs-desktop behavior, broadcasts and response projection in `ServerActor`.

Do not try to remove these constructor calls just to reduce grep output.

## 4. Important current audit findings

Use this audit command, not only `self.runtime_store.`:

```text
rg -n "runtime_store" rust/alera-cli/src/terminal_host/server -g "*.rs" -g "!*test*.rs"
```

The narrower `self.runtime_store.` search misses multiline field access.

### 4.1 Direct storage still visible in `requests.rs`

Most `runtime_store` mentions are valid handler construction, but there are still two real persistence/lifecycle seams worth addressing.

#### Mobile settings update

Current flow:

```text
mobile.settings.update
  -> runtime_store.mobile_access_settings()
  -> apply_mobile_settings_update_resolved(...)
  -> apply_mobile_gateway_settings(current, next)
  -> broadcast mobileSettingsChanged
```

The raw settings read is still actor-owned even though persistence is not the actor's core responsibility.

#### Pairing create

Current flow:

```text
mobile.pairing.create / pairing.create
  -> runtime_store.mobile_access_settings()
  -> prepare_mobile_pairing_offer_settings_resolved(...)
  -> maybe restart_mobile_gateway()
  -> maybe apply_mobile_gateway_settings(...)
  -> create_mobile_pairing_offer_for_settings(&runtime_store, ...)
  -> broadcast mobilePairingsChanged
```

This is **not** the same as ordinary device CRUD. It crosses persisted settings plus live gateway lifecycle.

#### Terminal terminate idempotency receipts are intentionally different

`terminal.terminate` still passes `&self.runtime_store` to:

- `prepare_receipt(...)`
- `settle_receipt(...)`
- `remove_receipt(...)`

These calls are part of the terminal termination transaction / replay semantics around a live session lifecycle. Do not mechanically move them just because they appear in `requests.rs`.

Only refactor this if there is a concrete need to test receipt orchestration separately and the ordering semantics are fully pinned.

### 4.2 Configuration transfer still owns two direct reads

`configuration_requests.rs::configuration_transfer` still directly calls:

- `alera_account()` to verify the selected account owns the runtime;
- `configuration_snapshot(account)` when starting a snapshot transfer.

The buffer/transfer lifecycle itself is correctly owned by `self.configuration_transfers` and should stay actor-owned.

### 4.3 Automation policy storage is only partially finished

`AutomationAgentPolicyStoreHandler` and `AutomationProjectPolicyStoreHandler` now own policy get/set, but `automation_policy_requests.rs` still directly reads context:

- automation run by id;
- workspace by id;
- project by id;
- existing tab by id;
- project/workspace data used by repository declaration checks.

This is no longer a policy persistence problem. It is a **policy context query** boundary.

### 4.4 Agent-title tab persistence is done, settings query is not

The agent-title flow now uses `WorkspaceTabStoreHandler` for tab find/upsert, but still directly calls:

```text
runtime_store.effective_ai_assist_settings()
```

at multiple stages:

- request acceptance;
- automatic title queueing;
- prepare-before-generation;
- apply-result validation.

Jobs, cancellation, conversation/revision guards, session lookup and broadcasts should stay actor-owned.

### 4.5 Large remaining `RuntimeStore` usage is mostly lifecycle/orchestration code

The biggest remaining clusters are in:

- `coordinator_requests.rs`
- `coordinator_dispatch.rs`
- `coordinator_stall_policy.rs`
- `dispatch_context_continuations.rs`
- `dispatch_context_install.rs`
- terminal/session/spawn lifecycle modules
- host service / quota flows
- AI assist async work
- runtime mutation flows

These are not automatically "unfinished DI". Many accesses are coupled to in-memory state machines, retries, admission, session/process ownership, dispatch ordering, cancellation or rollback.

## 5. Recommended next work, in order

The next receiver should work in small commits and stop after each green batch.

### P0.1 - Finish mobile settings / pairing-create persistence boundary

This is the strongest remaining central-dispatcher cleanup.

Files:

- `rust/alera-cli/src/terminal_host/server/requests.rs`
- `rust/alera-cli/src/terminal_host/server/mobile_device_pairing_requests.rs`
- mobile access helpers/tests

Recommended shape:

1. Add or extend a narrow mobile persistence handler with only what is needed:
   - load current mobile access settings;
   - create/persist a pairing offer for already-resolved settings.
2. Keep these in `ServerActor`:
   - `apply_mobile_settings_update_resolved(...)` unless moving it has an actual testability benefit;
   - `restart_mobile_gateway()`;
   - `apply_mobile_gateway_settings(...)`;
   - live gateway replacement/bind lifecycle;
   - broadcasts;
   - mobile client disposal.
3. Do **not** turn the existing handler into a broad `MobileServices` bag.

Before moving code, pin these semantics with request-level tests:

- settings update failure does not leave runtime settings/gateway inconsistent;
- pairing create with unchanged settings restarts the gateway only when required;
- pairing offer creation failure does not silently lose the already-selected settings state;
- broadcasts happen only after the corresponding operation succeeds.

Direct handler test should use a temp `RuntimeStore` and avoid `ServerActor`.

Suggested validation:

```text
cargo test --manifest-path rust/alera-cli/Cargo.toml mobile_device_pairing_requests::tests::
cargo test --manifest-path rust/alera-cli/Cargo.toml terminal_host::server::requests::tests::
cargo test --manifest-path rust/alera-cli/Cargo.toml mobile_relay_presence
```

If a test name/filter differs, inspect current tests instead of inventing a new production abstraction around the filter.

### P0.2 - Extract configuration transfer data reads, keep transfer lifecycle in actor

Files:

- `configuration_requests.rs`

The current `ConfigurationStoreRequestHandler` already owns ordinary configuration settings/snapshot/apply/published operations.

Next safe batch:

- add narrow methods or a small query helper for:
  - runtime account ownership check;
  - configuration snapshot fetch.
- keep `configuration_transfers.start/read/chunk/cancel/take` in `ServerActor`.

A good end state is:

```text
ServerActor
  -> validates transfer/client ownership
  -> asks injected store handler for account/snapshot data
  -> owns transfer buffer lifecycle
```

Do not move the transfer buffer into the store handler.

Direct tests:

- selected account mismatch returns the same error;
- snapshot data can be loaded without constructing `ServerActor`.

Suggested validation:

```text
cargo test --manifest-path rust/alera-cli/Cargo.toml configuration_requests::tests::
cargo test --manifest-path rust/alera-cli/Cargo.toml terminal_host::server::requests::tests::
```

### P0.3 - Extract automation policy context reads

Files:

- `automation_policy_requests.rs`

Already done:

- agent policy get/set;
- project policy get/set.

Still direct context queries:

- `find_automation_run`
- `find_workspace`
- `find_project`
- `find_workspace_tab`

Recommended approach:

- first reuse `WorkspaceTabStoreHandler::find` for `target_profile_id` if that keeps dependencies simple;
- for automation-run/workspace/project context, introduce a **feature-scoped read-only context query** only if it materially reduces actor coupling;
- keep repository declaration checking and live target identity verification where they are unless separate substitution is clearly useful.

Avoid a generic `AutomationDependencies` bag.

Pin behavior before changing:

- run target identity mismatch;
- managed-agent actor resolution;
- missing workspace/project;
- folder project rejection for managed workspace automation;
- restrictive project policy without local approval;
- missing `alera.toml` automation declaration.

Suggested validation:

```text
cargo test --manifest-path rust/alera-cli/Cargo.toml automation_policy_requests::tests::
cargo test --manifest-path rust/alera-cli/Cargo.toml terminal_host::server::requests::tests::
```

### P1.1 - Inject effective AI-assist settings query into agent-title flow

Files:

- `agent_title_generation.rs`
- possibly `ai_assist_requests.rs` / `ai_assist_speech_message.rs`

Current tab persistence is already narrow. The remaining direct store dependency is mostly:

```text
effective_ai_assist_settings()
```

Recommended shape:

- add/reuse a tiny settings query handler/capability;
- keep title jobs, cancellation, stale job checks, session lookup and broadcasts in actor code;
- consider sharing the query with AI assist async code only if the capability stays exactly scoped to effective settings retrieval.

Do not create a broad AI service container.

Regression coverage should include:

- AI Assist disabled;
- automatic title generation disabled;
- manual regeneration;
- settings changing while a generation is in flight;
- late/stale generation result rejection.

### P1.2 - Optional tiny global effect ports in `requests.rs`

Current examples:

- Sentry live toggle in `configure`;
- login shell environment reload.

Only extract these if it buys real testability.

If done:

- use a tiny effect adapter/port;
- keep `apply_config` and host lifecycle ownership in actor;
- do not introduce a general `HostServices` bag.

This is lower value than P0/P1.1.

## 6. Medium-risk work after the safe batches

### P2.1 - Terminal/session persistence helpers

Remaining direct persistence appears in terminal/session/spawn flows.

Examples:

- tab find/update during terminal session requests;
- stale/failed tab cleanup;
- one-shot initial command/prompt cleanup;
- workspace/tab recovery lookup during spawn.

Only extract the data operation. Keep:

- PTY/process owner;
- session map;
- spawn/restart ordering;
- rollback;
- one-shot semantics;
- output flush/checkpoint;
- broadcasts.

Do one operation family at a time, with lifecycle regression tests before and after.

### P2.2 - AI assist async worker store ownership

`ai_assist_requests.rs` and `ai_assist_speech_message.rs` clone `RuntimeStore` into async work.

This is lower priority because it is already explicit async-worker ownership rather than central dispatcher persistence.

If refactored:

- inject a feature-scoped query/store capability into the worker;
- preserve cancellation and task ownership;
- avoid a shared AI dependency bag.

### P2.3 - Host-service / quota store reads

Some host-service and quota modules still clone/read the store.

Treat each as its own domain. Only extract when:

- the module currently needs a full actor solely for data access, or
- direct unit testing is materially blocked by the broad dependency.

Do not pursue these from grep count alone.

## 7. High-risk / defer until there is a concrete pain point

### P3.1 - Coordinator / orchestration state machines

Largest remaining clusters:

- `coordinator_requests.rs`
- `coordinator_dispatch.rs`
- `coordinator_stall_policy.rs`

These mix persistence with:

- orchestration task lifecycle;
- coordinator/stall state;
- gates;
- retries;
- message reconciliation;
- dispatch state;
- cancellation;
- delivery ordering.

Do not mechanically convert every store call.

If touching these later:

1. identify one data-only slice;
2. pin state-machine behavior first;
3. extract only that slice;
4. preserve transactional/lifecycle ordering;
5. run coordinator/orchestration regressions before the next slice.

### P3.2 - Dispatch context install/continuation flows

These flows intentionally coordinate persistence with live dispatch state and recovery.

Do not split them until a specific testability or ownership problem justifies it.

### P3.3 - Runtime mutation pipeline

`runtime_mutations.rs` already receives/owns a `RuntimeStore` as part of the mutation worker boundary.

This is not a priority DI smell.

Do not split the store purely for cosmetic consistency.

## 8. Intentionally actor-owned / not unfinished

Treat these as correct ownership unless a separate task proves otherwise:

- host shutdown/restart/promote-persistent;
- terminal create/attach/restart/read/write/resize/reclaim/detach/terminate;
- live terminal/session maps;
- PTY/process ownership;
- terminal termination idempotency receipt ordering;
- mobile gateway restart/bind/replacement;
- client auth and per-client projection/filtering;
- broadcasts and wire delivery;
- runtime mutation queue/admission ordering;
- automation/orchestration in-memory state and cancellation;
- SSH bootstrap live jobs (`start/cancel/jobs`);
- configuration transfer buffer/session ownership;
- agent-title generation job ownership.

Again: **zero `runtime_store` references is not the goal**.

## 9. Validation status at this handoff

Earlier batches repeatedly passed focused tests, including direct handler tests, request regressions, rustfmt and `git diff --check`.

However, the current HEAD is now `396aae10`, which includes several commits after the previous handoff:

```text
736757fd refactor(runtime): inject automation project policy handler
cea338d0 refactor(runtime): inject mobile device pairing handler
339675e6 refactor(runtime): inject ssh bootstrap plan handler
f6b8fd0b refactor(runtime): inject agent hook settings query
83460746 refactor(runtime): reuse workspace tab store for agent titles
396aae10 refactor(runtime): reuse workspace tab store for agent profile launch
```

Do **not** treat old green runs as sufficient final-integration evidence for this exact HEAD.

The next receiver should run current-head validation before merging.

## 10. Per-batch workflow

For every future batch:

1. Confirm worktree/branch/HEAD and clean status.
2. Read the exact current implementation before editing.
3. Identify:
   - actor-owned lifecycle/state;
   - data-only dependency;
   - pure logic.
4. Extract only the data/effect boundary that is actually useful.
5. Add direct handler/helper tests without a full `ServerActor` where practical.
6. Run focused module tests.
7. If request dispatch changes, run:
   - `terminal_host::server::requests::tests::`
8. Run rustfmt on touched files.
9. Run `git diff --check`.
10. Commit the batch before starting the next one.

Do not stack several unvalidated DI moves into one commit.

## 11. Final integration plan to `main`

At the code checkpoint before this handoff-doc commit, the branch had 53 unique commits and `main` had 11 unique commits. Recompute the numbers before integration; the handoff-doc commit itself will advance the branch by one commit.

Recommended sequence:

1. Verify feature worktree clean:
   ```text
   git status --short --branch
   ```
2. Record:
   ```text
   git rev-parse HEAD
   git rev-parse main
   git rev-list --left-right --count main...HEAD
   ```
3. Reconcile `main` into the DI branch or use a fresh integration worktree based on current `main`.
4. Resolve conflicts without discarding either side's unrelated work.
5. Re-run focused DI handler tests.
6. Run the central request regression:
   ```text
   cargo test --manifest-path rust/alera-cli/Cargo.toml terminal_host::server::requests::tests::
   ```
7. Run the main affected modules, at minimum:
   ```text
   cargo test --manifest-path rust/alera-cli/Cargo.toml automation_policy_requests::tests::
   cargo test --manifest-path rust/alera-cli/Cargo.toml mobile_device_pairing_requests::tests::
   cargo test --manifest-path rust/alera-cli/Cargo.toml configuration_requests::tests::
   cargo test --manifest-path rust/alera-cli/Cargo.toml agent_title
   cargo test --manifest-path rust/alera-cli/Cargo.toml agent_profile_launch
   cargo test --manifest-path rust/alera-cli/Cargo.toml ssh_bootstrap
   ```
8. Run broader terminal-host/server tests if practical before final merge.
9. Run rustfmt check on touched Rust files.
10. Run:
    ```text
    git diff --check
    ```
11. Inspect the final diff/tree against `main`.
12. Only then merge/fast-forward `main` according to the integration branch shape.
13. Do not push unless explicitly requested.

If full `cargo test --manifest-path rust/alera-cli/Cargo.toml` is practical on the machine, run it before final delivery. If it is too expensive, document exactly which targeted suites were run and which were not.

## 12. Receiver start sequence

A new conversation can resume directly with:

1. Read `docs/runtime-rpc-di-refactor-handoff.md`.
2. Open `.worktrees/runtime-rpc-route-registry`.
3. Verify the branch still contains code checkpoint `396aae10`; inspect every newer commit after it before continuing. A handoff-doc-only commit immediately after it is expected.
4. Read:
   - `skills/alera-di-architecture/SKILL.md`
   - `skills/alera-di-architecture/references/review-checklist.md`
5. Run:
   ```text
   git status --short --branch
   git log --oneline --decorate -15
   git rev-list --left-right --count main...HEAD
   rg -n "runtime_store" rust/alera-cli/src/terminal_host/server -g "*.rs" -g "!*test*.rs"
   ```
6. Start with **P0.1 mobile settings/pairing-create persistence boundary**.
7. Commit every green batch independently.
8. Before main integration, follow section 11 exactly.

## 13. Useful audit commands

```text
git status --short --branch
git log --oneline --decorate main..HEAD
git log --oneline HEAD..main
git rev-list --left-right --count main...HEAD

rg -n "runtime_store" rust/alera-cli/src/terminal_host/server -g "*.rs" -g "!*test*.rs"
rg -n "runtime_store" rust/alera-cli/src/terminal_host/server/requests.rs
rg -n "runtime_store" rust/alera-cli/src/terminal_host/server/automation_policy_requests.rs
rg -n "effective_ai_assist_settings" rust/alera-cli/src/terminal_host/server -g "*.rs"

git diff --check
```

When reviewing grep output, classify each reference into one of these buckets:

1. narrow injected handler/store access — usually correct;
2. composition-root construction — correct;
3. actor-owned lifecycle/transaction access — often correct;
4. data-only access embedded in actor logic — best DI candidate.

That classification is more important than the raw number of matches.
