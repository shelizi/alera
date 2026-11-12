# Agent native history synchronization (2026-11-11)

Status: integrated into local Classic and GPUI branches; experimental and not released.

## Scope

Keep agent authentication and process state isolated while allowing native
history transcript files to be shared within one agent provider and one local
Alera Runtime Host. Classic and Native GPUI will eventually read the same Host
settings. Never copy JSONL across Codex / Claude / Antigravity formats.

## Modes

| Mode | Policy | Default |
| --- | --- | --- |
| `separate` | Each account has independent history; no new copies are made | Yes |
| `all` | All registered accounts of the **same** provider on one Host share finished transcript files | No (opt-in) |
| `groups` | Only accounts with the same nonempty group label and same provider share files | No (opt-in) |

Grouping stores `provider:accountId -> groupName`, separately per host.
Unassigned accounts in grouped mode are always independent. Changing modes
does not erase any previously copied history.

Explicitly saved `historySync.mode=all` or `groups` settings remain selected
across upgrades. An absent `historySync` key or absent `mode` defaults to
`separate` (no new cross-account copies). Existing mirrored files are never
deleted when choosing Separate; disabling a previously enabled policy does
not retroactively revoke already copied history.

## Initial implementation

- Host-managed setting in `RuntimeAgentQuotaSettings.historySync`. Existing
  settings without this key default to `separate`; settings use the Runtime Host
  update protocol and preserve multi-window revision validation.
- Classic Settings: selectable history sync policy and per-account group label
  editors for Codex, Claude and the currently known AGY default account.
- Rust Host worker (3-minute cadence) discovers known Codex `sessions/` and
  Claude `projects/` trees for configured accounts and copies only `.jsonl`
  transcript files from stable, real directories.
- A source file must have stopped changing for at least two minutes. Files
  larger than 16 MiB or empty files are ignored. A pass has a bounded budget.
- No existing target history file is ever overwritten, deleted, appended to,
  or merged; conflicts are retained in both directories. Source and target
  filesystem symlinks / overlapping history roots are rejected.
- No token, login, OAuth, config, hook file, or SQLite provider database is
  copied. The worker operates locally on the owner host only.
- Stopping sharing does not retroactively revoke local copies; display this
  limitation prominently to users.
- AGY automatic file copying is **not enabled** pending verification of its
  history layout, keyring/profile isolation, and cross-account native resume.
  The mode and group data model can already represent AGY accounts.

- Stale-history detection: when a session continues after initial mirroring,
  an exact complete-line prefix is reported as stale in Host warnings.
  The mirror never overwrites either account's native transcript.

## Private immutable updates for continued conversations (2026-11-12)

- After a transcript was copied once, an append-only, quiet source that has
  continued growing is classified as stale. The native target remains
  untouched: a live or resumed provider session may hold that file open.
- The Host saves the full newer version as immutable, content-addressed JSONL
  under its private `agent-history-snapshots/` directory, outside Codex,
  Claude and AGY account directories. Distinct source/target account pairs
  and session paths receive distinct hashed namespaces.
- Repeated scans of the same content deduplicate. Storage is limited to
  16 immutable versions up to 16 MiB each per mirrored session; versions are
  never deleted automatically. Capacity and write errors appear in Host logs.
- A private snapshot is **not** an automatically imported provider session,
  and there is **not yet** a GPUI history viewer for these versions. Native
  Resume can still see an old copy; native cross-account continuation needs
  separate provider-specific acceptance tests.
- Conversation payloads can contain secrets even though authentication files
  are never copied. Snapshots remain on the private local Runtime Host.

- `alera runtime --json history-sync get` now exposes bounded, metadata-only
  `archiveSummary` counts (sessions, versions, truncated) after Host auth; no
  raw conversation content or profile paths enter this settings response.
- GPUI History Sync Settings displays these Host-owned counts. A real history
  viewer and native resume handoff are still separate release gates.

## Explicit limitations / gates

1. Copying a transcript is **not proof** native CLI resume will accept it on a
   different authenticated account; Codex app-server indexes and remote
   provider-owned session authorization may be required.
2. Native transcript files which grow after a prior copy are not automatically
   merged: existing files remain untouched to protect any ongoing session.
   A future version must implement a per-session version/lineage mechanism.
3. Providers may change their history formats; avoid guessing unknown paths.
4. Test with temporary fake account homes, then with real signed-in pairs in
   isolated test profiles **before** merging or enabling for normal users.
5. The current implementation handles file copying, not native history index
   refresh in either provider; UI should not promise full cross-account resume
   until end-to-end verification.
6. Remote Host history is not silently moved between machines. Cloud sync
   and cross-provider handoff are separate features.

## GPUI/Classic shared settings integration (2026-11-12)

- GPUI adds Settings > History Sync, including all/separate/groups and
  per-account group editors. It does not duplicate the policy in Native's
  independent preferences JSON.
- Native GPUI invokes the reviewed `alera runtime --json history-sync` CLI
  surface. The CLI attaches only to an already-running, authenticated Runtime
  Host; read/update use the authoritative `runtimeSettings` RPC.
- Writes use `expectedRevision` to fail on concurrent Classic or GPUI edits
  and verify the Host's returned `historySync` before reporting success.
- GPUI displays Host version/offline errors without falling back to a
  divergent Native policy. Press Refresh after external settings changes.
- Group names are edited for the known Codex/Claude accounts and AGY
  default account. AGY mirroring remains unverified and disabled.
- UI and Rust bridges are experimental until GPUI build checks and a
  disposable Host / two-client read-update-readback smoke test pass.

## TDD checklist

- Rust policy default migration, separate and grouped matching, serde roundtrip.
- Rust copying tests: new transcript, same-provider only, no credentials,
  existing-target conflict, quiet-period and overlapping source/target safety.
- Flutter model roundtrip and mode-selection tests, runtime settings persistence.
- Production-ready acceptance additionally requires end-to-end native session
  list/read/resume tests for Codex and Claude and actual AGY profile isolation.
