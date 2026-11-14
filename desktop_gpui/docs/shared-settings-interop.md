# Alera Classic / GPUI — shared Rust settings (phase 1)

## Owner and contract

The **existing Rust RuntimeStore** is the one persistent owner for these six
non-secret fields. The Host exposes only `desktopSettings.get`,
`desktopSettings.seedLocal` and `desktopSettings.compareAndSet`.
All mutations validate exact path, type, bounds and old value under
`BEGIN IMMEDIATE`; no UI writes or replaces the whole settings document.

| Shared field | Rust storage | Old Classic storage |
|---|---|---|
| general.language | runtimeMetadata `settings.desktop.local.v1` | local Drift |
| general.autoGitFetchEnabled | same | local Drift |
| general.autoGitFetchIntervalMinutes | same | local Drift |
| terminal.fontSize | `configuration.portable.v1` | Host portable config |
| editor.autosaveEnabled | same | Host portable config |
| editor.tabSize | same | Host portable config |

- Existing Classic Drift values for the first three fields are imported
  **once** via `seedLocal`; only absent fields are filled. A previously saved
  GPUI value is never overwritten by delayed Classic migration.
- Classic loads authoritative Host values and stores a compatibility snapshot
  back in Drift. On user edits it performs exact-value CAS and writes only
  changed portable fields. Unrelated Classic changes no longer resend the
  three shared portable scalars and overwrite a newer GPUI value.
- GPUI invokes `alera settings --no-start --json get` / `set --path ...
  --expected ... --value ...` on its background executor. It never starts
  a new Host or reads tokens / SQLite directly.
- GPUI starts with a nonblocking refresh, polls every 3 s while Settings
  is open, and every 15 s otherwise. Classic reacts to
  `runtimeSettingsChanged` emitted after successful CAS.
- Language uses Classic's five `AppLanguage` wire identifiers, including
  `system`. Windows system locale uses Win32 GetUserDefaultLocaleName.
- A stale CAS rejects with `desktop_settings_revision_conflict`. When Host
  is offline or too old, GPUI refuses a divergent JSON fallback and shows
  a status/error. Classic gracefully retains its previous Drift behavior
  until the new Host/CLI is installed.

## Native-only settings and incomplete UI

GPUI continues to own Explorer hidden/ignored visibility and the Native-local
Agent Profile choice in `native-settings-v1.json`. They have not been
silently replaced, migrated or exposed via the six-field API. Most of the
Classic 15-category Settings catalog remains visual-only. Expanded settings
interoperability should use reviewed new field paths and the appropriate
RuntimeStore owner instead of writing directly to Flutter's Drift database.

Some dynamic Agent/profile fields need separate identity validation and
may be Host-owned rather than portable configuration. Passwords, agent
credentials, tokens, executable paths and full settings JSON are never
returned by the shared CLI snapshot.

## Deployment order and QA

1. Ship the Rust Core + Runtime Host + `alera` CLI with this new contract.
   Old `alera.exe` or an old running Host will reject GPUI shared setting
   operations; do not report them saved.
2. Ship Classic with the new repository and GPUI with the new Native bridge.
   Classic seeds missing local values after its existing portable config
   seed; GPUI preserves the Host result.
3. With both desktop apps open, change each of the six settings in Classic,
   verify Native refreshes; change in Native and verify Classic reacts.
4. Race two writes to the same value, verify one succeeds and the stale
   writer reloads. Verify unrelated values survive, restart both apps and
   inspect persisted values; verify offline/old Host error paths.
5. Validate System/English/Traditional Chinese/Simplified Chinese/Japanese
   and that language changes never restart a PTY or clear editor state.
6. For Flutter worktree tests initialize the two locked git submodules
   `third_party/xterm` and `third_party/dart_terminal` first.

Production launch, release, Main/Classic merge and remote push are
separate steps; committing this feature to the GPUI integration branch does
not deploy an updated Runtime Host. No current user preferences are deleted
or changed by a source-code-only merge.
