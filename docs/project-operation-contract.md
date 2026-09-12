# Project operation contract

This table answers the eight transaction/replay questions from the architecture refactor handoff (section 13) for every `project.*`, `projectConfig.*`, `hostDirectory.*`, and clone request handled by `ServerActor`. Companion to `orchestration-operation-contract.md`; update the table and its pinning tests together.

Authority is `ServerActor` on the terminal-host mailbox, persisting through `RuntimeStore`. Registration splits into an off-mailbox prepare (`prepare_project_registration`, filesystem validation under the deferred Bulk budget) and an on-mailbox commit (`finish_project_registration`); the deferred split exists so directory probing cannot stall control requests. Destructive removal goes through the runtime-mutation worker so the mailbox only sees prepare/apply completion commands.

## Registration and metadata

| Operation | Instance check | Replay | Same id + different payload | Commit vs publish | Lost reply retry | Restart recovery | Stale owner / wrong actor |
|---|---|---|---|---|---|---|---|
| `project.register` | Path must exist as a directory; canonicalized before comparison | Idempotent by canonical path: an existing project at the same path returns `created: false` plus its ensured main workspace | Two registrations of the same path with different names keep the first project's name | Commit on the actor, then `projectsChanged`/`workspacesChanged`/`workspaceTabsChanged` broadcast, then the parked response | Returns the same project; no duplicate row | Persisted | n/a |
| `project.rename` | Project must exist; name must be non-empty | Value write: re-applying the same name is a no-op result | Last write wins | Upsert, then `projectsChanged` broadcast | Safe | Persisted | n/a |
| `project.upsert` | Caller supplies the whole `Project` row | Value write | Last write wins | Upsert, then `projectsChanged` broadcast | Safe | Persisted | n/a |
| `project.remove.preview` | Project id | Read | n/a | Read only | Safe | Persisted | n/a |
| `project.list` | none | Read | n/a | Read only | Safe | Persisted | n/a |
| `project.branches.list` | Project id; Git access runs under the deferred budget | Read | n/a | Read only | Safe | Persisted | n/a |
| `hostDirectory.roots` / `hostDirectory.list` | Directory path where applicable; listing runs under the deferred budget | Read | n/a | Read only | Safe | Persisted | n/a |
| `projectConfig.find` / `projectConfig.list` / `projectConfig.effective` | Project id where applicable; `effective` reads repo files under the deferred budget | Read | n/a | Read only | Safe | Persisted | n/a |
| `projectConfig.upsert` | Project id required | Value write | Last write wins | Upsert, then `projectConfigsChanged` broadcast | Safe | Persisted | n/a |
| `projectConfig.remove` | Project id required | Delete of a missing row is a no-op | n/a | Delete, then `projectConfigsChanged` broadcast | Safe | Persisted | n/a |

## Removal

| Operation | Instance check | Replay | Same id + different payload | Commit vs publish | Lost reply retry | Restart recovery | Stale owner / wrong actor |
|---|---|---|---|---|---|---|---|
| `project.remove` | Runtime-mutation request; the cascade runs inside the mutation worker, not the mailbox | Idempotent cascade: tabs, layouts, tag assignments, linked reviews, workspace relations, workspaces, project row, configs, and clone jobs are deleted in one transaction, so a second call deletes nothing | n/a | Cascade commits first; the `ProjectRemoved` effect then closes affected tabs/sessions on the actor | Safe; the second remove is a transaction of zero-row deletes | Persisted; a host restart mid-mutation leaves at most a partial cascade that the next `project.remove` completes | n/a |

## Clone jobs

| Operation | Instance check | Replay | Same id + different payload | Commit vs publish | Lost reply retry | Restart recovery | Stale owner / wrong actor |
|---|---|---|---|---|---|---|---|
| `project.clone.start` | Destination path validated (parent exists, name is a single safe component) | Not deduplicated: each call inserts a new `projectCloneJobs` row and spawns a runner | n/a | Job row committed and cancel channel registered before the clone task spawns; `projectCloneJobsChanged` broadcasts after | Duplicate job for the same destination; the second clone fails when the destination materializes, but two queued jobs can race the same destination path | `reconcile_interrupted_project_clones` sweeps `queued`/`cloning`/`registering` rows at startup | Cancel authority is the in-memory `project_clone_jobs` map, so a job row whose runner died reports `not running on this runtime` instead of a stuck cancel |
| `project.clone.list` | none | Read | n/a | Read only | Safe | Persisted | n/a |
| `project.clone.cancel` | Job must exist | Terminal jobs return the stored job row instead of erroring | n/a | In-memory oneshot signals the runner; the runner commits the cancelled state | Safe | Persisted | See above |

Gap: `clone.start` has no dedupe key on `(parent_path, directory_name)`. A retried or double-clicked start can queue two runners for one destination; the loser fails on filesystem state rather than a typed busy response. Acceptable while clone starts are user-initiated and rare; add a destination-unique pending-job check if automation ever drives this path.
