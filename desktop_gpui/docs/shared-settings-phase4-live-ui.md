# Alera GPUI Settings: shared Rust Core controls (phase 4)

## Live binding, not two preference stores

The GPUI Settings dialog uses the Classic navigation and row catalog, but
**connected rows now read the actual Rust Runtime Host snapshot and persist
with authenticated, per-field CAS**. A row is not independently writable
from Native JSON or Flutter Drift. A stale edit fails and reloads Host truth.

Rust's reviewed `SHARED_DESKTOP_PATHS` contract now has **30 fields**:
the original 12, 16 previously Classic-portable general/Terminal/Editor/
Agent preferences, the Quick Open excluded-directory list, and the
Classic/Native local `editor.twoRowTabs` toggle.

The additional sixteen fields:
- `general.showTrayIcon`, `showDockBadge`, `showTrayBadge`
- `terminal.fontFamily`, `lineHeight`, `cursorShape`, `cursorBlink`,
  `themeName`, `backgroundOpacity`, `tuiScrollSensitivity`,
  `clipboardOnSelect`
- `editor.themeName`, `autosaveDelaySeconds`
- `agents.agentStatusNotificationsEnabled`,
  `agentStatusFinishedNotificationsEnabled`, `showTabTitlesInSidebar`

Classic already persists these fields in
`configuration.portable.v1.desktop.settings`. Classic and GPUI now both
submit only edited fields to `desktopSettings.compareAndSetBatch` (or
`compareAndSet`) with expected prior values. The Host updates the **same**
SQLite document, rejecting stale writes atomically. Classic no longer
replays those sixteen fields through an unrelated broader
`configuration.settings.update` patch. Native JSON retains only migration
history for earlier shared values.

## Actual connected UI

- **Application**: Git auto-fetch (on/off, interval), tray icon, tray
  badge and Dock/taskbar badge values. Language selector is the existing
  real, Host-connected control at left. Destructive deletion confirmations
  and autoarchive are displayed as Rust Host values **read-only**; they
  remain editable in Classic because an Agent-executable CLI must not gain
  permission to disable safety gates.
- **Terminal**: font family and font size, line height, cursor shape and
  blinking, theme preset, background opacity, TUI scroll sensitivity and
  copy-on-select. The actual Native terminal text now uses the shared font
  family and line height, rather than fixed JetBrains Mono / 1.3.
  Host PTY row estimation and Windows IME candidate anchoring use the same
  configurable line height; released line selections use the shared
  copy-on-select switch with the existing current-Host-frame guard.
- **Editor**: syntax theme name, tab size, autosave enabled and 1–60 s
  debounce. Native file-backed panes use the Host-owned delay rather than
  a fixed 3-second threshold, and both panes use the same loaded value.
  Native's general housekeeping runs at 1.5 s cadence, so the earliest
  actual save may occur on the next tick, not exactly at 1.0 second.
- **Agents**: sidebar titles and desktop notification flags are bound to
  the portable Rust settings; existing Host-running-Agent panel remains
  active. Agent Profiles retain Host-backed default selection/readback.
- The remaining catalog sections are visible for parity, **but no longer
  draw fake switches, selects, inputs or clickable-looking actions**.
  They show an explicit unavailable label until their workflows are
  implemented. This does *not* claim the full Classic Settings UX is done.

## What is genuinely Native-effective today

- Git auto-fetch enabled/interval, Terminal font family/size and line
  height, terminal copy-on-select, Editor tab size/autosave/debounce, Explorer visibility, language
  selection, default Agent Profile, and Git branch favorites.
- Other portable appearance/notification controls share authoritative
  values with Classic. Native renderers and OS integrations that have not
  yet implemented a specific effect (e.g. cursor blinking, Classic tray
  badge, syntax theme color map, local speech-to-text, AI assistance,
  remote-host/device/account workflows) **must not be represented as fully
  implemented Native behavior**. Their values still synchronize with
  Classic and survive restarts.

## Validation and remaining acceptance

- Core contract checks strict type/range validation and excludes unsafe
  settings from CLI write allowlist.
- Classic Settings repository tests verify Native changes to themes,
  Terminal and Editor settings plus notification flags are read back
  authoritatively and cannot be overwritten by stale whole-section saves.
- Two independent Rust RuntimeStore clients validate cross-frontend
  portable writes, atomic batch rejection, stale CAS, Classic configuration
  readback and persisted reopen.
- GPUI Native snapshot test validates all 28 fields before applying any
  value; wrong schema or unsupported fields fail closed.
- Settings catalog mapping test checks every connected row has an approved
  Rust Core writable path.
- GPUI desktop binary compiles and Classic repository regression tests
  continue to pass with the expanded snapshot.
- A real simultaneous Classic+GPUI Windows GUI session with the rebuilt
  Host/CLI is still required for exact visual/readback/IME parity; source
  compilation cannot substitute for an interactive two-window test.

No deployment, main-branch merge or release is implied by this source commit.
