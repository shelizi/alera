# Settings Ownership

Every persisted settings field belongs to one or more of three ownership tiers. The tier decides where the value is stored, which channel delivers it, and which mixin on `SettingsController` mutates it.

## Tiers

| Tier | Storage and delivery | Codified in |
| --- | --- | --- |
| Local-only UI prefs | `DriftSettingsRepository` only; never leaves the machine | `settings_controller_local_ui.dart` |
| Runtime operational settings | pushed to the terminal-host sidecar through `runtimeSettings.update` or the `configure` request (`terminalHostConfigFor`) | `settings_controller_runtime_operational.dart`, `AleraSettings.toRuntimeOperationalMap` |
| Portable-cloud configuration | synced through the `configuration.settings.*` document; field allowlist is `desktopPortableFields` in `packages/alera_configuration` | `settings_controller_portable.dart`, `AleraSettings.toPortableConfigurationMap` |

A field can sit in more than one tier: the `runtimeSettings.update` payload and the portable allowlist overlap (for example `confirmProjectRemoval`, `autoArchiveWorkspacesAfterDays`, `defaultAgentProfileId`, and the AI-assist subset are both runtime operational and portable).

`SettingsOwnershipTier` in `lib/src/features/settings/domain/settings_ownership.dart` names the three tiers for doc references.

## Field-level ownership

### `general`

| Field | Tiers |
| --- | --- |
| `language` | local-only |
| `workspaceDirectory` | runtime (`runtimeSettings.update`) |
| `starClicked` | local-only |
| `confirmProjectRemoval` | runtime + portable |
| `confirmWorkspaceRemoval` | runtime + portable |
| `keepAliveEnabled` | local-only |
| `showTrayIcon` | portable |
| `showDockBadge` | portable |
| `showTrayBadge` | portable |
| `showPullRequestStatusInSidebar` | local-only |
| `pullRequestFailureNotificationsEnabled` | local-only |
| `autoArchiveWorkspacesAfterDays` | runtime + portable |

### `agents`

| Field | Tiers |
| --- | --- |
| `agentStatusHooks` | runtime (`runtimeSettings.update`) |
| `agentStatusNotificationsEnabled` | portable |
| `agentStatusFinishedNotificationsEnabled` | portable |
| `keepComputerAwakeWhileAgentsWork` | local-only |
| `showTabTitlesInSidebar` | portable |
| `defaultAgentProfileId` | runtime + portable |
| `gitBashExecutablePath` | local-only (per-device executable override) |
| `quotas` (per-host `enabledProviders`, `providerDefaultsVersion`, `claudeDefault*`, `claudeProfiles`, `environment`) | runtime (`agentQuotas` for host `local`) |
| `quotas.selectedClaudeProfile`, `quotas.unpinnedQuotaKeys` | local-only (merged back from local storage on load) |

### `aiTextGeneration` (`AiAssistSettings`)

| Field | Tiers |
| --- | --- |
| `enabled`, `autoGenerateAgentTitles`, `agent`, `selectedModelByAgent`, `selectedThinkingByModel`, `selectedThinkingByOperation`, `customCommand`, `instructionsByOperation`, `promptSettingsByOperation`, `timeoutSeconds` | runtime + portable |
| `discoveredModelsByAgent`, `discoveredDefaultModelByAgent` | local-only (stripped from the runtime payload and merged back on load). `defaultThinkingLevel` is a property of each discovered model, not a top-level `AiAssistSettings` field. |

### `aiDictation`

| Field | Tiers |
| --- | --- |
| `enabled`, `transcriptionEngine`, `rewriteMode`, `providerPolicy`, `language`, `hostFallbackEnabled`, `providerFallbackEnabled`, `remoteBaseUrl`, `remoteModel`, `codexRealtimeModel`, `remoteProvider`, `timeoutSeconds` | portable |
| `localModelId`, `remoteConsentVersion`, `systemRecognitionConsentVersion` | local-only |

### `textActions`

| Field | Tiers |
| --- | --- |
| entire section | runtime (`runtimeSettings.update`); catalog items also sync through the shared portable document |

### `editor`

| Field | Tiers |
| --- | --- |
| `tabSize`, `themeName`, `autosaveEnabled`, `autosaveDelaySeconds`, `quickOpenExcludedDirectories` | portable |
| `externalEditor`, `codeOpenTarget`, `externalEditorExecutablePaths`, `externalEditorWorkspaceMode`, `autoOpenNewWorkspacesExternally`, `languageIntelligence` | local-only |

### `diagnostics`

| Field | Tiers |
| --- | --- |
| `logLevel`, `crashReportingEnabled` | local-only |

### `terminal`

| Field | Tiers |
| --- | --- |
| `fontFamily`, `fontSize`, `fontWeight`, `lineHeight`, `paddingX`, `paddingY`, `cursorShape`, `cursorBlink`, `cursorOpacity`, `themeName`, `backgroundOpacity`, `wordSeparators`, `colorOverrides`, `tuiScrollSensitivity`, `clipboardOnSelect`, `allowOsc52Clipboard`, `showComposerByDefault`, `toolbarCorner` | portable |
| `hostEmptyShutdownDelaySeconds`, `hostDetachedSessionShutdownDelaySeconds`, `hostScrollbackBytes`, `loginShell` (as `resolvedLoginShell`) | runtime (`configure`) |
| `bufferBudgetMegabytes`, `keepRuntimeOpenOnAppQuit` | runtime (consumed by the workbench buffer owner and the runtime-host quit gate) |
| `scrollbackLines`, `powerShell7ExecutablePath`, `confirmCloseRunningProcesses` | local-only (`scrollbackLines` also feeds the derived `restoreSnapshotBytes` sent in `configure`; executable overrides are per-device) |

### `keyboard`

| Field | Tiers |
| --- | --- |
| `overrides`, `terminalPolicy` | portable |

## File map

Domain (`lib/src/features/settings/domain/`):

- `alera_settings.dart` - `AleraSettings` aggregate, `AppLanguage`, the mixed `GeneralSettings`/`AgentSettings` sections, and the legacy `MappingHook`s.
- `agent_status_hook_settings.dart` - `AgentStatusHookSettings` (runtime operational).
- `agent_quota_settings.dart` - quota provider/profile/environment models (runtime operational payload, with the two local-only UI fields noted in the file header).
- `alera_terminal_settings.dart` - `TerminalSettings` (portable appearance fields plus the runtime `configure` knobs, noted in the class doc).
- `diagnostics_settings.dart` - `DiagnosticsSettings` (local-only).
- `editor_settings.dart` - `EditorSettings` (portable core plus local-only external editor fields, noted in the class doc).
- `settings_ownership.dart` - `SettingsOwnershipTier` and the `toRuntimeOperationalMap` / `toPortableConfigurationMap` projections consumed by `RuntimeSettingsRepository`.

Application (`lib/src/features/settings/application/`):

- `settings_controller.dart` - the facade: load, mutation serialization, persistence.
- `settings_controller_local_ui.dart` - `_SettingsControllerLocalUiSettings`.
- `settings_controller_runtime_operational.dart` - `_SettingsControllerRuntimeOperationalSettings`.
- `settings_controller_portable.dart` - `_SettingsControllerPortableSettings`.

The application domain does not depend on settings presentation: `settings/{application,domain,infra}` and `workbench/{application,domain}` have no imports into `settings/presentation`, and the runtime architecture guard enforces the direction.
