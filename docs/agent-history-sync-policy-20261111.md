# Agent native history synchronization (2026-11-11)

Status: feature worktree, not yet integrated into main or released.

## Scope

Keep agent authentication and process state isolated while allowing native
history transcript files to be shared within one agent provider and one local
Alera Runtime Host. Classic and Native GPUI will eventually read the same Host
settings. Never copy JSONL across Codex / Claude / Antigravity formats.

## Modes

| Mode | Policy | Default |
| --- | --- | --- |
| `all` | All registered accounts of the **same** provider on one Host share finished transcript files | Yes |
| `separate` | Each account has independent history; no new copies are made | No |
| `groups` | Only accounts with the same nonempty group label and same provider share files | No |

Grouping stores `provider:accountId -> groupName`, separately per host.
Unassigned accounts in grouped mode are always independent. Changing modes
does not erase any previously copied history.

## Initial implementation

- Host-managed setting in `RuntimeAgentQuotaSettings.historySync`. Existing
  settings without this key default to `all`; settings use the Runtime Host
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

## TDD checklist

- Rust policy default migration, separate and grouped matching, serde roundtrip.
- Rust copying tests: new transcript, same-provider only, no credentials,
  existing-target conflict, quiet-period and overlapping source/target safety.
- Flutter model roundtrip and mode-selection tests, runtime settings persistence.
- Production-ready acceptance additionally requires end-to-end native session
  list/read/resume tests for Codex and Claude and actual AGY profile isolation.
