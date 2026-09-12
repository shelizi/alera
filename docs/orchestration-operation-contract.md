# Orchestration operation contract

This table answers the eight transaction/replay questions from the architecture refactor handoff (section 13) for every `orchestration.*` request handled by `ServerActor`. It is the contract surface that dispatch, replay, and recovery tests pin down; when an operation's behavior changes, update the table and the tests together.

Authority for all rows is `ServerActor` on the terminal-host mailbox, persisting through `RuntimeStore`. No operation currently accepts a client-supplied idempotency key; replay safety comes from state-transition guards and the stored `contextToken` hash, not from dedupe tables.

## Messages

| Operation | Instance check | Replay | Same id + different payload | Commit vs publish | Lost reply retry | Restart recovery | Stale owner / wrong actor |
|---|---|---|---|---|---|---|---|
| `send` | Recipients must resolve; lifecycle types rejected | Not deduplicated; each call inserts a new row | n/a (no op id) | Insert, then deliver + wake waiters + broadcast | Duplicates the message | Persisted rows survive; waiters are in-memory | Group fan-out shares one thread id |
| `reply` | `id` must name an existing message | Marks original read, inserts a new row each call | n/a | Mark read + insert reply, then deliver + wake | Duplicates the reply | Persisted | Reply direction fixed to the original sender |
| `ask` | Single handle required; group addresses rejected | Not deduplicated | n/a | Insert question, then park waiter | Duplicates the question | Waiter lost; message persists | `from` rerouted to the active dispatch's coordinator when one exists |
| `check` | `terminal` required | Read; `--all` never mutates | n/a | Consume marks read inside the read path | Safe; read side effect is monotonic | Persisted read state | Waiter wakes only for the registered handle |
| `inbox` | none | Read | n/a | Read only | Safe | Persisted | n/a |

Gap: `send`, `reply`, `ask`, and `escalate` have no idempotency key, so a retry after a lost reply writes a duplicate row. This is accepted for now (humans and agents tolerate duplicate mail) but is the first candidate if a client op id is ever added.

## Tasks

| Operation | Instance check | Replay | Same id + different payload | Commit vs publish | Lost reply retry | Restart recovery | Stale owner / wrong actor |
|---|---|---|---|---|---|---|---|
| `taskCreate` | Run must exist, be running, match workspace and coordinator when `run` is given | Not deduplicated; server-generated task id | n/a | Create + optional stage bind, then respond | Duplicates the task | Persisted | `stage` is validated against the run's approved plan, not trusted from the payload |
| `taskList` / `taskShow` | `taskShow` requires existing id | Read | n/a | Read only | Safe | Persisted | n/a |
| `taskCancel` | Task must exist; terminal states (`completed`/`failed`) rejected | Idempotent: already `cancelled` returns the task | n/a | Cancel + audit event in one store call, then notify assignee | Safe second call | Persisted | Non-`force` calls must pass `actor` = task coordinator; `force` writes an audited `task.cancel.force` event |
| `taskRecover` | Task must exist and be `stalled`; target status limited to `ready`/`failed`/`cancelled` | Rejected once the task leaves `stalled` | n/a | Transition + audit event | Typed `task is not stalled` error | Persisted | Same coordinator/`force` rule as cancel |
| `transferCoordinator` | Exactly one of `task`/`run`; a task owned by a run must be transferred through the run | Store-level transition with audit event | n/a | Transition + audit; run transfer also rewrites the in-memory coordinator handle | Re-applies the transition; audit log records each attempt | Persisted; in-memory coordinator handle resyncs on next store read | `actor` required; non-forced calls must be the current coordinator |

## Dispatch lifecycle

| Operation | Instance check | Replay | Same id + different payload | Commit vs publish | Lost reply retry | Restart recovery | Stale owner / wrong actor |
|---|---|---|---|---|---|---|---|
| `dispatch` | Task must be `ready`; assignee must not already hold an active dispatch; coordinator `from` must own the task; injection requires a live session in the task workspace with an idle detected agent (or `--assume-agent`) | State-guarded: a committed dispatch moves the task to `dispatched`, so a retry fails closed with `task is not ready` | n/a | Dispatch row committed before the deferred context install runs; the response is only written after install completion lands | Typed error, not a second dispatch | `awaiting_acceptance` rows are swept to `startup_failed` on host start; `dispatched` rows wait for stall detection | `contextToken` hash is stored at commit and re-validated on every worker call |
| `dispatchShow` | `task` required | Read | n/a | Read only | Safe | Persisted | n/a |
| `dispatchAccept` | Active dispatch for `terminal` must exist; stored token hash must match | Idempotent: re-accepting keeps the first `accepted_at` | Wrong token rejected before state is read | Single UPDATE guarded by assignee + token + status | Safe; the second accept is a no-op transition | Accepted dispatches survive restart and age into stall detection | `dispatch acceptance rejected: stale context or wrong assignee` |
| `dispatchInterrupt` | Dispatch must exist and be `dispatched`/`stalled` | Audited each call; the control bytes queue again | n/a | Interrupt bytes queued, then audit event | Harmless repeated interrupt | n/a | Same coordinator/`force` rule as taskCancel |

Gap: `dispatch` has no client op id, so a retry after a lost prepare-phase error cannot distinguish "my dispatch committed" from "a new dispatch". The state guard keeps retries safe in practice: a committed dispatch leaves the task non-`ready`, so the retry errors out instead of double-dispatching.

## Worker completion path

| Operation | Instance check | Replay | Same id + different payload | Commit vs publish | Lost reply retry | Restart recovery | Stale owner / wrong actor |
|---|---|---|---|---|---|---|---|
| `context` | Active dispatch + valid token | Read | Wrong token rejected | Read only (may compose the task prompt) | Safe | Persisted | Token guard + active dispatch lookup |
| `heartbeat` | Active dispatch + valid token; store only records activity while `dispatched` | Rejected once the dispatch is inactive | Wrong token rejected | Single UPDATE | Typed `heartbeat rejected for inactive dispatch` | Inactive after restart sweep | Token guard + `dispatched` status check |
| `escalate` | Active dispatch + valid token | Inserts a new escalation row per call | n/a | Insert message + record activity, then push + wake coordinator | Duplicates the escalation | Persisted | Assignee fixed to the dispatch owner; `from` is rewritten to the assignee |
| `complete` | Active dispatch, or the latest completed dispatch for replay; valid token; result must match the task's result schema | Idempotent: replay on a completed dispatch returns the stored outcome without re-running the terminal policy | A replayed `failure` after a success, or a replay under a newer dispatch's token, is rejected | Completion + task transition in one store transaction; terminal policy applied after | Safe; replay returns the committed dispatch id | Persisted | `dispatch completion rejected: inactive context or wrong assignee`; stale token rejected |
| `workerDone` | Dispatch id + task id + terminal must all match the stored dispatch | Idempotent through the same store transition | n/a | Same transition as `complete` | Safe | Persisted | `worker-done authority rejected` when any of the three ids disagree |

## Runs and gates

| Operation | Instance check | Replay | Same id + different payload | Commit vs publish | Lost reply retry | Restart recovery | Stale owner / wrong actor |
|---|---|---|---|---|---|---|---|
| `run` / `runStop` | Run lifecycle owned by the coordinator module | Server-generated run id | n/a | Run row + coordinator handle committed together | Duplicates spawn a new run | Persisted runs are the startup recovery source | `runStop` requires the run's coordinator or `force` |
| `runList` / `runShow` / `status` | `runShow`/`status` require an existing run | Read | n/a | Read only | Safe | Persisted | n/a |
| `gateCreate` | `task` required | New gate per call | n/a | Insert, then push notification | Duplicates the gate | Persisted | n/a |
| `gateResolve` | Gate must exist and be `pending` | Rejected once resolved | Second resolve with a different resolution is a typed `not pending` error | Resolve + unblock the task in one transaction | Typed `decision gate is not pending` | Persisted | n/a |
| `reset` | none | Wipes scope again (already empty is a no-op) | n/a | Coordinator stop + store wipe | Safe | n/a | Destructive; no ownership check beyond host auth |

## Waiters

| Mechanism | Contract |
|---|---|
| `check --wait` / `ask` / `terminalWait` / `taskWait` | Parked as `MessageWaiter` entries keyed by `(client_id, request_id)`; each gets a spawned timeout that posts `OrchestrationWaitTimeout` back to the actor. A wake that races another consumer re-parks the same waiter id so the original deadline still expires it. Client disconnect drops its waiters. Waiters are in-memory only: a host restart silently drops them, and the client must retry. |
