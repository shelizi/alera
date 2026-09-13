# Workspace operation contract

This table answers the eight transaction/replay questions from the architecture refactor handoff (section 13) for every `workspace.*`, `workspaceTag.*`, `workspaceRelation.*`, `workspaceSection.*`, `workspaceActivity.*`, `linkedReview.*`, and `workspaceCascade.*` request handled by `ServerActor`. Companion to `orchestration-operation-contract.md` and `project-operation-contract.md`; update the table and its pinning tests together.

Authority is `ServerActor` on the terminal-host mailbox, persisting through `RuntimeStore`. Three execution surfaces exist:

- Mailbox CRUD: small store reads/writes handled inline in `requests.rs` / `workspace_sidebar_requests.rs` / `workspace_pinning.rs` / `workspace_section_requests.rs`.
- Deferred jobs: `workspace.createManaged`, `workspace.runSetup`, `workspace.storageImpact` spawn onto `tokio` tasks and reply through `ServerCommand` completions; `managed_workspace_jobs` gates the idle shutdown timer while any of them run.
- Runtime mutations: `workspace.remove`, `workspace.removeForProject`, `workspace.removeManaged`, `workspace.switchBranch`, `workspace.sleep`, `tab.remove`, `tab.removeForWorkspace` go through the mutation worker so destructive work and external preflight stay off the mailbox; the actor only sees `PrepareRuntimeMutation` / `RuntimeMutationFinished` commands.

## Workspace records

| Operation | Instance check | Replay | Same id + different payload | Commit vs publish | Lost reply retry | Restart recovery | Stale owner / wrong actor |
|---|---|---|---|---|---|---|---|
| `workspace.list` / `workspace.listAll` / `workspace.find` | Workspace id where applicable | Read | n/a | Read only | Safe | Persisted | n/a |
| `workspace.upsert` | Caller supplies the whole `Workspace` row | Value write | Last write wins | Upsert, then `workspacesChanged` broadcast | Safe | Persisted | n/a |
| `workspace.rename` | Workspace must exist | Value write | Last write wins | Rename commits, then `workspacesChanged(project)` broadcast | Safe | Persisted | n/a |
| `workspace.setPinned` | Workspace must exist | Value write | Last write wins | Flag write, then `workspacesChanged(project)` broadcast | Safe | Persisted | n/a |
| `workspace.repositoryWebUrl` | Workspace must exist | Read; Git remote read runs under `spawn_blocking` in the deferred budget | n/a | Read only | Safe | Persisted | n/a |

## Managed workspace lifecycle

| Operation | Instance check | Replay | Same id + different payload | Commit vs publish | Lost reply retry | Restart recovery | Stale owner / wrong actor |
|---|---|---|---|---|---|---|---|
| `workspace.createManaged` | Existing workspace id, branch, or path each fail closed before the worktree is created | Not idempotent by request: a retry re-runs validation and fails on the id/path the first attempt created | n/a | Store row commits inside `create_managed_workspace`; `workspacesChanged` broadcasts when the spawned job replies | Fail closed: the identical retry re-validates and returns the duplicate id/branch/path state error | `reconcile_spawn_on_create_tabs` and managed-workspace startup reconciliation sweep partial state | n/a |
| `workspace.runSetup` | Workspace must exist | Copy rules re-apply; not destructive but also not deduplicated | n/a | Filesystem copies only; no store commit, no broadcast | Re-runs the copy rules | Persisted | n/a |
| `workspace.storageImpact` | Active-workbench and live-session blockers computed on the actor; automation-owner check runs inside the spawned job | Read-only measurement | n/a | Read only | Safe | Persisted | n/a |
| `workspace.switchBranch` | Runtime mutation; `switch_managed_workspace_branch` validates the workspace and target branch inside the worker | Retry re-validates current state; switching to the same branch succeeds as a no-op write or fails on branch rules | n/a | Store update commits in the worker; `WorkspaceBranchSwitched` effect then refreshes sessions/broadcasts on the actor | Safe | Persisted | n/a |
| `workspace.sleep` | Runtime mutation | Idempotent: deletes `workspaceTabs` + `workbenchLayouts` in one transaction, second call deletes nothing | n/a | Cascade commits first; `WorkspaceSlept` effect then closes sessions on the actor | Safe | Persisted; partial cascade finishes on the next call | n/a |
| `workspace.remove` | Runtime mutation; `cascadeTabs` flag honored | Idempotent cascade: tabs (optional), linked reviews, layouts, tag assignments, relations, workspace row in one transaction | n/a | Cascade commits first; `WorkspaceRemoved` effect then closes tabs/sessions | Safe; second call is a transaction of zero-row deletes | Persisted | n/a |
| `workspace.removeForProject` | Runtime mutation | Idempotent per workspace; loops `remove_workspace(cascade=true)` | n/a | Same ordering as `workspace.remove` per workspace | Safe | Persisted | n/a |
| `workspace.removeManaged` | FS/Git/automation preflight in the mutation worker; actor re-checks live session/pending shutdown at `PrepareRuntimeMutation` and may close sessions first | Retrying after a successful remove is a no-op; retrying after a preflight failure re-validates | n/a | Destructive commit preceded by a final `managed_workspace_removal` revalidation; `ManagedWorkspaceRemoved` effect publishes after | Safe | Persisted; pending-shutdown state survives restart through the shutdown ledger | Live sessions gate the mutation; the actor-owned check cannot be bypassed by a stale client |

## Tags, relations, sections, activity, reviews, layouts

| Operation | Instance check | Replay | Same id + different payload | Commit vs publish | Lost reply retry | Restart recovery | Stale owner / wrong actor |
|---|---|---|---|---|---|---|---|
| `workspaceTag.list` | none | Read | n/a | Read only | Safe | Persisted | n/a |
| `workspaceTag.create` | Name must be non-empty and globally unique case-insensitively | Duplicate names are rejected with typed conflict `workspace_tag_name_conflict` | n/a | Upsert, then `workspaceTagsChanged` + `workspacesChanged` | A retry with the same name is rejected with typed conflict `workspace_tag_name_conflict`, including the existing tag id | Persisted | n/a |
| `workspaceTag.upsert` / `workspaceTag.remove` | Tag id | Value write / delete of a missing row is a no-op | Last write wins | Commit, then `workspaceTagsChanged` + `workspacesChanged` | Safe | Persisted | n/a |
| `workspaceTag.assign` / `workspaceTag.unassign` / `workspaceTag.setForWorkspace` | Workspace and tag ids | Assignment ops are idempotent set writes | Last write wins | Commit, then `workspacesChanged` | Safe | Persisted | n/a |
| `workspaceRelation.list` / `link` / `unlink` | Parent/child workspace ids | Link is idempotent; unlink of a missing edge is a no-op | n/a | Commit, then `workspaceRelationsChanged` + `workspacesChanged` | Safe | Persisted | n/a |
| `workspaceSection.*` | Section and workspace ids per op | See `workspace_section_requests_tests.rs` for the ordering contract | n/a | Commit, then section broadcast | Safe | Persisted | n/a |
| `workspaceActivity.list` / `upsertAll` / `remove` | Workspace id where applicable | Batch upsert and remove are set writes | Last write wins | Commit, then `workspaceActivityChanged` | Safe | Persisted | n/a |
| `linkedReview.find` / `upsert` / `remove` | Workspace id | Value write / delete of a missing row is a no-op | Last write wins | Commit, then `linkedReviewsChanged` | Safe | Persisted | n/a |
| `layout.find` / `upsert` / `remove` | Workspace id; one layout row per workspace (`ON CONFLICT(workspaceId)`) | Value write / delete of a missing row is a no-op | Last write wins | Commit, then `workbenchLayoutsChanged` | Safe | Persisted | n/a |
| `workspaceCascade.preview` | Workspace ids and tag ids | Read | n/a | Read only | Safe | Persisted | n/a |

Known gaps:

- Closed: `workspaceTag.create` enforces globally unique tag names case-insensitively; a duplicate name is rejected with typed conflict `workspace_tag_name_conflict`, including the existing tag id.
- Dispositioned: `workspace.createManaged` is fail-closed on retry by contract (see `docs/wire-schema-codegen-evaluation-2026-09-13.md` §2). A client that times out or loses the connection must treat the outcome as unknown and must not expect repeating the identical request to return the workspace created by the first attempt. The host re-validates the requested workspace id, branch, and path, and returns the existing duplicate state error when one of those values was persisted. Partial filesystem or Git state, or a persisted workspace whose later relation or setup step failed, may remain and must be reconciled through normal workspace listing or cleanup rather than silently treated as a new successful create. Clients may retry with a newly generated identity only when the product flow explicitly chooses a new workspace identity.
