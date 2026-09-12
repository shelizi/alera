# Shared workbench preferences operation contract

This table answers the eight transaction/replay questions from the architecture refactor handoff (section 13) for the shared-preference surface: `workbenchViewPrefs.*` and `workspaceActivity.*` handled by `ServerActor`. Companion to `orchestration-operation-contract.md`, `project-operation-contract.md`, `workspace-operation-contract.md`, and `tab-layout-operation-contract.md`; update the table and its pinning tests together.

Authority is `ServerActor` on the terminal-host mailbox, persisting into `runtimeMetadata` through `RuntimeStore`. The defining rule of this domain is asymmetric optimistic concurrency: the desktop is the authoritative writer and always last-write-wins, while mobile writers must pass `expectedRevision` matching the stored revision or receive a typed conflict telling them to refresh.

## View preferences

| Operation | Instance check | Replay | Same id + different payload | Commit vs publish | Lost reply retry | Restart recovery | Stale owner / wrong actor |
|---|---|---|---|---|---|---|---|
| `workbenchViewPrefs.get` | none | Read; absent row returns the default record | n/a | Read only | Safe | Persisted | n/a |
| `workbenchViewPrefs.update` | Mobile writer: `expectedRevision` must equal the stored revision or the write fails with a typed conflict; desktop writer: no revision check | Desktop retry is idempotent (same value rewritten); mobile retry after a lost reply fails the revision check because the first write already bumped it, so the client must refresh | n/a | Missing legacy keys (`sectionSort`, `collapsedSectionIds`, `othersSectionCollapsed`) are backfilled from the current record before the write; commit then `workbenchViewPrefsChanged` broadcast | Desktop: safe. Mobile: the retry reports the first write's revision bump as a conflict rather than applying twice | Persisted; `desktopInitialized`/`lastWriter` survive restart | A stale mobile revision is the typed conflict path; desktop always wins |

## Workspace activity

| Operation | Instance check | Replay | Same id + different payload | Commit vs publish | Lost reply retry | Restart recovery | Stale owner / wrong actor |
|---|---|---|---|---|---|---|---|
| `workspaceActivity.list` | none | Read | n/a | Read only | Safe | Persisted | n/a |
| `workspaceActivity.upsertAll` | none | Max-wins merge per workspace: an older timestamp never overwrites a newer one | n/a | Batch merge commits, then `workspaceActivityChanged` broadcast and the full map is returned | Safe: re-sending an older timestamp is ignored | Persisted | n/a |
| `workspaceActivity.remove` | Workspace id | Delete of a missing key is a no-op | n/a | Delete, then `workspaceActivityChanged` broadcast | Safe | Persisted | n/a |

Invariants worth keeping visible:

- The mobile revision check exists so a phone with a stale snapshot cannot silently overwrite desktop sorting/collapse state; the conflict message instructs a refresh rather than merging.
- Activity timestamps are monotonic per workspace, so out-of-order mobile/desktop writes converge instead of oscillating.
