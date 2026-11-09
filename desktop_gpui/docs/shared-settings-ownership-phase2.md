# Classic / GPUI Rust-owned settings: phase 2 ownership and acceptance

## Authoritative owner

Both desktop editions are **views of the same Rust RuntimeStore and Runtime Host**.
The field contract and CLI allowlist have one home:
`rust/alera-core/src/desktop_settings_contract.rs` (always-built, including GPUI).
No Classic or GPUI frontend should independently own a shared preference.

| Group | Field(s) | Authoritative Rust location | Previous frontend |
| --- | --- | --- | --- |
| Language | general.language | settings.desktop.local.v1 | Classic Drift |
| Git | general.autoGitFetchEnabled, general.autoGitFetchIntervalMinutes | settings.desktop.local.v1 | Classic Drift / Native JSON |
| Terminal | terminal.fontSize | configuration.portable.v1 | Classic portable / Native JSON |
| Editor | editor.autosaveEnabled, editor.tabSize | configuration.portable.v1 | Classic portable / Native JSON |
| Removal safety | general.confirmProjectRemoval, general.confirmWorkspaceRemoval | Existing settings.general.* runtime metadata | Classic Runtime / Native JSON |
| Worktree | general.autoArchiveWorkspacesAfterDays | Existing settings.general.autoArchiveWorkspacesAfterDays | Classic Runtime / Native JSON |
| Agent | agents.defaultAgentProfileId | Existing settings.agents.defaultAgentProfileId | Classic Runtime / Native JSON |
| Explorer | explorer.showHiddenFiles, explorer.showIgnoredFiles | settings.desktop.local.v1 | Classic Workbench Drift / Native JSON |

**Twelve** reviewed shared fields now have exactly one Rust owner and one
transactional scalar CAS contract. This is an incremental *frontend binding*
of existing Rust portable/runtime stores, **not** a claim that all 15 GPUI
Settings UI sections are interactive.

Other already Rust-owned categories (not copied to GPUI JSON):
- `configuration.portable.v1` includes substantially more portable Terminal,
  Editor, keyboard, general, AI dictation and agent UI settings used by Classic.
- Rust RuntimeStore already owns workspace directory, automation, agent status
  hooks, quota configuration, AI assistance, text actions and the Agent Profile
  catalog, subject to their existing validation and lifecycle.
- `workbenchViewPrefs.get/update` already exposes a reviewed Rust-managed
  *shared* project/workspace grouping, filtering and visibility model.

Keep distinct per-frontend window/pane geometry, Flutter menu state, Native
focus/selection/tab decorations and other settings with different semantics.
Classic's Workbench pane width, panel selection, sort modes not already shared
through RuntimeStore are **not** merged into Native JSON or vice versa.

## Migration and synchronization

- Classic Settings migrates its three local scalar values once with
  `desktopSettings.seedLocal`. Classic Workbench independently seeds its two
  Explorer visibility preferences from `showHiddenFiles` and `explorerMode`.
  Rust only fills absent values; a previously saved other-edition selection
  remains authoritative.
- Native GPUI reads the former `native-settings-v1.json` **only for migration**
  of Explorer flags and the previously selected default Agent Profile. Native
  Profile migration is one-shot, requires a valid Host catalog entry and
  writes a tombstone even on an absent or invalid old selection. This prevents
  a cleared default from reappearing on subsequent startup.
- Original Classic Drift and GPUI JSON files remain on disk for rollback; they
  are not second owners of these 12 settings after Host sync succeeds.
- Classic Settings issues a **single** `desktopSettings.compareAndSetBatch`
  for every affected field, with the expected old values; Rust validates and
  commits *all* fields in one SQLite `BEGIN IMMEDIATE` transaction or none.
  Runtime setting revision is incremented when runtime fields change, so
  existing Host-wide revision clients reject stale writes.
- Classic Workbench uses the same atomic CAS for Explorer flags; a rejected
  write reloads the current Host values rather than silently displaying a
  failed optimistic choice. Workbench-only visual preferences remain Drift.
- GPUI settings and Explorer toggles call `alera settings --no-start --json`
  through the existing authenticated CLI bridge *off* the UI thread and never
  start a Host or own PTYs. GPUI applies the Host snapshot to its own models
  and Explorer tree/search index after authoritative readback.
- GPUI refresh cadence is 3 s while Settings is visible, 15 s otherwise;
  Classic Settings and Workbench respond to Host change broadcasts.
  This is **eventual UI refresh**, not an unsafe concurrent JSON write.
- If Host or CLI is too old/unavailable, GPUI rejects writes and surfaces an
  error rather than creating another divergent settings file. Classic retains
  its old fallback until Host is upgraded. A failed migration never deletes
  prior preferences.

## Security boundaries

The shared snapshot is a narrow reviewed non-secret projection only. Agents
cannot change removal confirmation or autoarchive through `alera settings
set` (the safe CLI allowlist excludes them). Full configuration, API keys,
Agent credentials, executable paths, user secrets and Host control tokens are
not exposed by the shared settings CLI.

## Acceptance and deployment

Automated checks cover two separate Rust RuntimeStore handles sharing one
database, cross-column CAS, stale conflict rollback, profile foreign-key
validation, one-time migration and process-restart persistence. GPUI model
tests check all 12 fields and fail-closed snapshots; Classic repository tests
cover Host projection, no stale whole-document writes and Explorer CAS.

Full interactive two-running-app acceptance still requires the rebuilt Host,
Classic Flutter desktop and GPUI binaries on the same Windows user profile.
Manual checklist:
1. Open Classic + GPUI with the upgraded Rust Host; check the 12 shared values.
2. Toggle each setting in Classic, observe GPUI; then reverse, including
   hidden and ignored file options and default Agent Profile.
3. Change a shared value concurrently in both editions; the stale CAS must
   reject and the losing UI must refresh. Repeat with two fields in one edit.
4. Restart both applications, checking the 12 values, Explorer tree filtering
   and Agent Profile selection; verify old preference files remain unchanged.
5. Check Host-offline, old-CLI, invalid/stale Agent Profile and readback
   failure paths. Verify no UI path silently writes to Native JSON as fallback.
6. Verify different sidebar widths and tab/pane layout stay independent,
   as these are not semantically common Settings values.

No source-code commit alone upgrades an already-running Host. Merging to
Classic main, publishing builds, and pushing to origin are separate actions.

## Remaining overlapping preferences requiring dedicated identity migration

This phase **does not complete every possible future shared preference**.
Git favorites now have a dedicated shared Rust Core owner with exact
`[projectId, relativeGitRoot]` identity and Host-validated Native worktree
resolution, insert-only Classic/GPUI legacy migration and per-scope CAS.
See `shared-git-favorites-phase3.md`. The standalone worktree JSON is no
longer a production write target.

Native's preview recent-workspace/saved-project indexes also overlap Host
project/workspace inventory and must be reconciled by immutable Runtime
identities rather than blindly merging path-based caches. The remaining
noninteractive GPUI Settings catalog rows should bind to **existing Rust
portable/runtime owner methods when those features are implemented**; a
visual-only row is not evidence its setting is already connected.

Because the current workflow validates source and Rust store concurrency,
it is not a substitute for an actual two-visible-app Windows interaction
session with both rebuilt binaries and upgraded Host.
