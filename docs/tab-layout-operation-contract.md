# Tab and layout operation contract

This table answers the eight transaction/replay questions from the architecture refactor handoff (section 13) for every `tab.*` and `layout.*` request handled by `ServerActor`. Companion to `orchestration-operation-contract.md`, `project-operation-contract.md`, and `workspace-operation-contract.md`; update the table and its pinning tests together.

Authority is `ServerActor` on the terminal-host mailbox, persisting through `RuntimeStore`. Tabs are special in two ways: their `payload` carries host-owned fields (`agentTitle*` state) that client writes must not clobber, and `kind = "terminal"` tabs with `payload.spawnOnCreate` own a live PTY that is spawned as part of the upsert.

## Tabs

| Operation | Instance check | Replay | Same id + different payload | Commit vs publish | Lost reply retry | Restart recovery | Stale owner / wrong actor |
|---|---|---|---|---|---|---|---|
| `tab.list` / `tab.find` | Workspace/tab id | Read; rows pass through `workspace_tab_for_client` so mobile and desktop get different projections (host-owned prompt/pulse fields redacted where required) | n/a | Read only | Safe | Persisted | Per-client projection prevents a mobile client from observing desktop-only fields |
| `tab.upsert` | Existing row's host-owned payload keys are preserved over the client payload (`preserve_host_owned_tab_payload`); a changed `agentTitleRevision` cancels the in-flight title job | Value write | Last write wins for client-owned fields; host-owned fields cannot be altered by the client | Row commits, then `spawnOnCreate` spawn runs; a spawn failure removes the tab row and terminates its sessions before the error is returned | Safe: retry re-upserts and re-attempts the spawn | `reconcile_spawn_on_create_tabs` respawns or removes spawn-on-create tabs at startup | Host-owned fields stay host-owned regardless of the writer |
| `tab.rename` | Tab must exist | Value write | Last write wins | Rename commits (after cancelling the title job), then `workspaceTabsChanged(workspace)` broadcast | Safe | Persisted | n/a |
| `tab.remove` | Runtime mutation; the title job is cancelled on the actor before enqueue | Idempotent: `RemoveTab` looks up the workspace id then deletes the row; a second call finds nothing and deletes nothing | n/a | Delete commits in the worker; `TabRemoved` effect then closes the session on the actor | Safe | Persisted | n/a |
| `tab.removeForWorkspace` | Runtime mutation | Idempotent: routes to `sleep_workspace`, deleting `workspaceTabs` + `workbenchLayouts` in one transaction | n/a | Cascade commits first; `WorkspaceTabsRemoved` effect follows | Safe | Persisted | n/a |

## Layouts

| Operation | Instance check | Replay | Same id + different payload | Commit vs publish | Lost reply retry | Restart recovery | Stale owner / wrong actor |
|---|---|---|---|---|---|---|---|
| `layout.find` | Workspace id | Read | n/a | Read only | Safe | Persisted | n/a |
| `layout.upsert` | One row per workspace (`ON CONFLICT(workspaceId) DO UPDATE`) | Value write | Last write wins | Commit, then `workbenchLayoutsChanged` broadcast | Safe | Persisted | n/a |
| `layout.remove` | Workspace id | Delete of a missing row is a no-op | n/a | Delete, then `workbenchLayoutsChanged` broadcast | Safe | Persisted | n/a |

Invariants worth keeping visible:

- A `tab.upsert` that fails its `spawnOnCreate` spawn leaves no tab row and no session; the rollback order is remove-row then terminate-sessions.
- `tab.remove` cancels the agent-title job on the actor before the mutation is enqueued, so an in-flight title write cannot resurrect a removed tab's title state.
- Startup reconciliation treats a failed spawn-on-create restore the same way: remove the tab row rather than leaving a tab with no backing session.
