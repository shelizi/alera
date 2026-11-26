# Classic Git, Worktree and Project performance — phase 1 (2026-11-26)

Base: main 0e87e230b, including all nine workspace-search phases. This phase is scoped to Dart workbench catalog reconciliation and initialization; no Rust/FRB or Git mutation API changes.

## Changes

- `WorkspaceService.ensureMainWorkspace` now preserves an unchanged main record, including its timestamp. It compares the actual fields owned by preparation (name, branch, path, kind, status, source branch and reuses-existing-branch flag). It does not rewrite the same record or trigger a redundant workspace.upsert event. Actual branch/name/path/kind repairs, same-checkout duplicate tab migration and legacy main-to-linked demotion still execute.
- `reconcile` reuses its last authoritative catalog list when no mutations occur after that read, instead of another workspace.list call. Uses already-resolved canonical keys for live worktree identity, while retaining fresh path checks and existing fail-closed pruning when the Git list fails or omits the main checkout. Mutation cases still read the persisted final list.
- The main workspace preparation coordinator invokes reconciliation once (reconcile already ensures the main workspace) and shares the actual in-flight Future per project. Concurrent different project preparations remain independent. Bootstrap completion no longer clears an error reported by the preparation callback.
- The controller test repository now publishes its initial snapshot on subscribe, matching both actual Runtime and Drift repositories. Its recovery fixture represents a live Git worktree after reconciliation, not a stale database-only row. Fixed a duplicate project ID in the 15-project startup test, which previously caused a legitimate legacy-main repair.

## Verified request counts

100 distinct temporary project paths, each with one main and three linked worktrees (400 total), in-memory repository and Git backend, one stable reconciliation per project. Old code paths were three explicit catalog reads and one unconditional main upsert per project. The actual new fixture asserts 200 catalog reads, zero upserts, zero removes, 100 currentBranch calls and 100 listWorktrees calls. All 400 workspace records retain original identity and timestamp. This is operation-count validation, not production elapsed time, CPU, IPC packet count or actual SQLite wall-clock benchmark. Real repository subscribers may incur further reads; avoiding writes also avoids their redundant change events.

15 distinct-project actual controller bootstrap test verifies exactly 15 currentBranch and 15 listWorktrees calls, zero workspace upserts and zero unchanged layout writes. Previously preparation called ensureMainWorkspace and then reconcile, which itself called ensureMainWorkspace. This halves those initialization branch checks for the direct preparation path. No claims about startup elapsed time.

## Validation

Initial nine-file run: 240 passed, three failed. Two layout hydration failures identified the fake repository's missing initial snapshot; the error test identified bootstrap completion clearing a preparation error. Second run: 242 passed, one failed; the reconnect fixture inserted a database-only worktree concurrent with reconciliation; fixture now waits for re-subscription's initial reconciliation and reports the checkout in the fake Git listing. Additional startup counting exposed an existing duplicate project-1 ID; corrected to 15 unique projects without suppressing real repair logic. Final 14-file Flutter regression: 273 passed, zero failed (including workspace service, preparation, controller, project sync/subscription, Git watcher, lifecycle monitor, auto-fetch, bootstrap orchestrator, workspace subscription/set/selection and Drift repository). Dart format and Git diff checks pass. No Rust or generated bridge source changed; no separate native build for this phase.

## Scope and follow-ups

No branch/worktree catalog freshness cache or suppressed actual Git listing. No packaged Release timing, real Runtime Host request benchmark, global watcher concurrency limit, or new Git graph/status algorithm. Next targets: bounded background worktree watcher scheduling and high-frequency Git status/repository metadata polling across many projects; measure before changing destructive or Git semantics. No main merge, push or deployment.
