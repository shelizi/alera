import 'dart:convert';

import 'package:alera/src/features/settings/application/settings_repository.dart';
import 'package:alera/src/features/settings/domain/settings_ownership.dart';
import 'package:alera_configuration/alera_configuration.dart';
import 'package:alera/src/features/ai_assist/domain/ai_assist_settings.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:alera/src/features/keyboard/domain/keyboard_action.dart';
import 'package:alera/src/features/text_actions/domain/text_actions_settings.dart';
import 'package:alera/src/platform/runtime_host/protocol/terminal_host_protocol.dart';

class RuntimeSettingsRepository({
  required final RuntimeHostClient client,
  required final SettingsRepository legacyRepository,
  final Future<void> Function()? beforeAccess,
}) implements SettingsRepository {
  Map<String, Object?>? _lastLoadedPortable;
  Map<String, Object?>? _lastLoadedRuntime;
  bool _sharedDesktopAvailable = false;

  @override
  Future<AleraSettings> load() async {
    var legacy = await legacyRepository.load();
    _sharedDesktopAvailable = false;
    try {
      await beforeAccess?.call();
      if (await _supportsConfiguration()) {
        await client.runtimeRequest('configuration.settings.seed', {
          'settings': legacy.toPortableConfigurationMap(),
        });
        final portable = jsonMap(
          await client.runtimeRequest('configuration.settings.get'),
        );
        final keyboard = jsonMap(portable['keyboard']);
        keyboard['overrides'] = jsonMap(keyboard['overrides'])
          ..removeWhere(
            (key, _) => !KeyboardActionId.values.any((id) => id.name == key),
          );
        portable['keyboard'] = keyboard;
        legacy = AleraSettings.fromJson(
          applyDesktopSettings(
            legacy.toMap(),
            portable,
            defaults: AleraSettings.defaults.toMap(),
          ),
        );
      }
      // Import Classic local UI preferences once, then read the single
      // Rust-owned value. Late Classic migration must not replace a value
      // that GPUI has already saved.
      var sharedDesktop = <String, Object?>{};
      try {
        final imported = _asMap(
          await client.runtimeRequest('desktopSettings.seedLocal', {
            'values': {
              'general.language': legacy.general.language.name,
              'general.autoGitFetchEnabled': legacy.general.autoGitFetchEnabled,
              'general.autoGitFetchIntervalMinutes':
                  legacy.general.autoGitFetchIntervalMinutes,
              'editor.twoRowTabs': legacy.editor.twoRowTabs,
            },
          }),
        );
        sharedDesktop = _asMap(imported['values']);
        _sharedDesktopAvailable = true;
      } catch (_) {
        // A running older Host cannot synchronize shared preferences yet.
        // Keep Classic's Drift values rather than breaking Settings startup.
      }
      final payload = await client.runtimeRequest('runtimeSettings.get');
      final runtime = _asMap(payload);
      if (!runtime.containsKey('workspaceDirectory')) {
        final effective = _applySharedDesktopValues(legacy, sharedDesktop);
        _lastLoadedPortable = effective.toPortableConfigurationMap();
        _lastLoadedRuntime = effective.toRuntimeOperationalMap();
        return effective;
      }
      final legacyLocalQuotas = legacy.agents.quotas.forHost('local');
      final runtimeAiAssist = _asMap(runtime['aiTextGeneration']);
      final sharedAiAssist = runtimeAiAssist.isEmpty
          ? legacy.aiAssist
          : AiAssistSettings.fromJson(runtimeAiAssist).copyWith(
              discoveredModelsByAgent: legacy.aiAssist.discoveredModelsByAgent,
              discoveredDefaultModelByAgent:
                  legacy.aiAssist.discoveredDefaultModelByAgent,
            );
      final runtimeTextActions = _asMap(runtime['textActions']);
      final sharedTextActions = runtimeTextActions.isEmpty
          ? legacy.textActions
          : TextActionsSettings.fromJson(runtimeTextActions);
      final defaultAgentProfileId = runtime.containsKey('defaultAgentProfileId')
          ? _optionalRuntimeString(runtime['defaultAgentProfileId'])
          : legacy.agents.defaultAgentProfileId;
      final resolved = legacy.copyWith(
        general: legacy.general.copyWith(
          workspaceDirectory: runtime['workspaceDirectory'] as String?,
          confirmProjectRemoval:
              runtime['confirmProjectRemoval'] as bool? ??
              legacy.general.confirmProjectRemoval,
          confirmWorkspaceRemoval:
              runtime['confirmWorkspaceRemoval'] as bool? ??
              legacy.general.confirmWorkspaceRemoval,
          autoArchiveWorkspacesAfterDays:
              (runtime['autoArchiveWorkspacesAfterDays'] as num?)?.toInt() ??
              legacy.general.autoArchiveWorkspacesAfterDays,
        ),
        agents: legacy.agents.copyWith(
          defaultAgentProfileId: defaultAgentProfileId,
          agentStatusHooks: .fromJson(_asMap(runtime['agentStatusHooks'])),
          quotas: legacy.agents.quotas.withHost(
            'local',
            // The runtime host stores only the quota fields it needs to fetch
            // usage, so UI-only fields must survive the merge from the local
            // repository or every runtimeSettingsChanged event resets them.
            AgentQuotaHostSettings.fromJson(
              _asMap(runtime['agentQuotas']),
            ).copyWith(
              selectedClaudeProfile: legacyLocalQuotas.selectedClaudeProfile,
              selectedCodexProfile: legacyLocalQuotas.selectedCodexProfile,
              unpinnedQuotaKeys: legacyLocalQuotas.unpinnedQuotaKeys,
            ),
          ),
        ),
        aiAssist: sharedAiAssist,
        textActions: sharedTextActions,
      );
      final effective = _applySharedDesktopValues(resolved, sharedDesktop);
      _lastLoadedPortable = effective.toPortableConfigurationMap();
      _lastLoadedRuntime = effective.toRuntimeOperationalMap();
      await legacyRepository.save(effective);
      return effective;
    } catch (_) {
      _lastLoadedPortable = legacy.toPortableConfigurationMap();
      _lastLoadedRuntime = legacy.toRuntimeOperationalMap();
      return legacy;
    }
  }

  /// Write only user-edited fields to the Rust Host with field-scoped CAS.
  /// Existing installations without desktopSettings support keep legacy
  /// persistence until both app binaries can be upgraded together.
  Future<void> compareAndSetSharedDesktopChanges(
    AleraSettings previous,
    AleraSettings next,
  ) async {
    if (!_sharedDesktopAvailable) return;
    final changes = <Map<String, Object?>>[];
    for (final path in _sharedDesktopPaths) {
      final expected = _sharedDesktopValue(previous, path);
      final value = _sharedDesktopValue(next, path);
      // List-valued shared fields (Quick Open excluded names) must use
      // structural comparison, not Dart List identity.
      if (jsonEncode(expected) == jsonEncode(value)) continue;
      changes.add({'path': path, 'expectedValue': expected, 'value': value});
    }
    if (changes.isNotEmpty) {
      // All changed values (local, portable and runtime) commit atomically.
      await client.runtimeRequest('desktopSettings.compareAndSetBatch', {
        'changes': changes,
      });
    }
  }

  /// Authoritative MCP scalar reads deliberately bypass the legacy Drift
  /// fallback: a disconnected Host must fail closed rather than acknowledge
  /// a stale value from another Flutter window. Only reviewed setting paths
  /// are returned; never expose a whole configuration document.
  Future<Object> readAuthoritativeMcpSetting(String path) async {
    // Reject unknown fields before any Host RPC, not after a capability
    // negotiation failure. Never permit free-form portable field access.
    if (!const <String>{
      'general.autoGitFetchEnabled',
      'general.autoGitFetchIntervalMinutes',
      'terminal.fontSize',
      'editor.autosaveEnabled',
      'editor.tabSize',
      'general.confirmProjectRemoval',
      'general.confirmWorkspaceRemoval',
      'general.autoArchiveWorkspacesAfterDays',
    }.contains(path)) {
      throw ArgumentError.value(path, 'path', 'MCP setting is not exposed');
    }
    if (_sharedDesktopAvailable && _sharedDesktopPaths.contains(path)) {
      final snapshot = _asMap(
        await client.runtimeRequest('desktopSettings.get'),
      );
      final value = _asMap(snapshot['values'])[path];
      if (value == null) {
        throw StateError('mcp_setting_authoritative_value_unavailable');
      }
      return value;
    }
    Object? value;
    if (const <String>{
      'general.confirmProjectRemoval',
      'general.confirmWorkspaceRemoval',
      'general.autoArchiveWorkspacesAfterDays',
    }.contains(path)) {
      final runtime = jsonMap(
        await client.runtimeRequest('runtimeSettings.get'),
      );
      value = runtime[path.split('.').last];
    } else {
      if (!await _supportsConfiguration()) {
        throw StateError('mcp_setting_authoritative_source_unavailable');
      }
      final portable = jsonMap(
        await client.runtimeRequest('configuration.settings.get'),
      );
      final parts = path.split('.');
      if (parts.length != 2) {
        throw ArgumentError.value(path, 'path', 'MCP setting is not exposed');
      }
      value = jsonMap(portable[parts.first])[parts.last];
    }
    final valid = switch (path) {
      'general.autoGitFetchEnabled' ||
      'editor.autosaveEnabled' ||
      'general.confirmProjectRemoval' ||
      'general.confirmWorkspaceRemoval' => value is bool,
      'general.autoGitFetchIntervalMinutes' =>
        value is int && value >= 1 && value <= 60,
      'editor.tabSize' => value is int && value >= 1 && value <= 16,
      'general.autoArchiveWorkspacesAfterDays' =>
        value is int && value >= 0 && value <= 36500,
      'terminal.fontSize' =>
        value is num && value.isFinite && value >= 8 && value <= 48,
      _ => throw ArgumentError.value(
        path,
        'path',
        'MCP setting is not exposed',
      ),
    };
    if (!valid) {
      throw StateError('mcp_setting_authoritative_value_unavailable');
    }
    return value as Object;
  }

  @override
  Future<void> save(AleraSettings settings) async {
    await beforeAccess?.call();
    if (await _supportsConfiguration()) {
      // An unrelated Classic setting edit must never replay stale Terminal
      // size, Autosave or Tab Size over a newer GPUI CAS update.
      final current = settings.toPortableConfigurationMap();
      final patch = _changedPortableFields(
        _lastLoadedPortable,
        current,
        excludeShared: _sharedDesktopAvailable,
      );
      if (patch.isNotEmpty) {
        await client.runtimeRequest('configuration.settings.update', {
          'settings': patch,
          'supportedKeyboardActionIds': KeyboardActionId.values
              .map((id) => id.name)
              .toList(),
        });
      }
    }
    // Runtime Host accepts partial updates. Sending the old full snapshot
    // would resurrect a deleted default profile or undo a safety change
    // made from GPUI while Classic remained open.
    final runtimeNow = settings.toRuntimeOperationalMap();
    final changedRuntime = _changedRuntimeFields(
      _lastLoadedRuntime,
      runtimeNow,
      excludeShared: _sharedDesktopAvailable,
    );
    if (changedRuntime.isNotEmpty) {
      await client.runtimeRequest('runtimeSettings.update', changedRuntime);
    }
    await legacyRepository.save(settings);
    _lastLoadedPortable = settings.toPortableConfigurationMap();
    _lastLoadedRuntime = runtimeNow;
  }

  Future<bool> _supportsConfiguration() async {
    final capabilityClient = client;
    return capabilityClient is RuntimeHostCapabilityClient &&
        await (capabilityClient as RuntimeHostCapabilityClient)
            .supportsRuntimeCapability('configurationSyncV1');
  }
}

const _sharedDesktopPaths = <String>[
  'general.language',
  'general.autoGitFetchEnabled',
  'general.autoGitFetchIntervalMinutes',
  'terminal.fontSize',
  'editor.autosaveEnabled',
  'editor.tabSize',
  'editor.twoRowTabs',
  'general.confirmProjectRemoval',
  'general.confirmWorkspaceRemoval',
  'general.autoArchiveWorkspacesAfterDays',
  'agents.defaultAgentProfileId',
  'general.showTrayIcon',
  'general.showDockBadge',
  'general.showTrayBadge',
  'terminal.fontFamily',
  'terminal.lineHeight',
  'terminal.cursorShape',
  'terminal.cursorBlink',
  'terminal.themeName',
  'terminal.backgroundOpacity',
  'terminal.tuiScrollSensitivity',
  'terminal.clipboardOnSelect',
  'editor.themeName',
  'editor.autosaveDelaySeconds',
  'agents.agentStatusNotificationsEnabled',
  'agents.agentStatusFinishedNotificationsEnabled',
  'agents.showTabTitlesInSidebar',
  'editor.quickOpenExcludedDirectories',
];

Object? _sharedDesktopValue(
  AleraSettings settings,
  String path,
) => switch (path) {
  'general.language' => settings.general.language.name,
  'general.autoGitFetchEnabled' => settings.general.autoGitFetchEnabled,
  'general.autoGitFetchIntervalMinutes' =>
    settings.general.autoGitFetchIntervalMinutes,
  'terminal.fontSize' => settings.terminal.fontSize,
  'editor.autosaveEnabled' => settings.editor.autosaveEnabled,
  'editor.tabSize' => settings.editor.tabSize,
  'editor.twoRowTabs' => settings.editor.twoRowTabs,
  'general.confirmProjectRemoval' => settings.general.confirmProjectRemoval,
  'general.confirmWorkspaceRemoval' => settings.general.confirmWorkspaceRemoval,
  'general.autoArchiveWorkspacesAfterDays' =>
    settings.general.autoArchiveWorkspacesAfterDays,
  'agents.defaultAgentProfileId' => settings.agents.defaultAgentProfileId,
  'general.showTrayIcon' => settings.general.showTrayIcon,
  'general.showDockBadge' => settings.general.showDockBadge,
  'general.showTrayBadge' => settings.general.showTrayBadge,
  'terminal.fontFamily' => settings.terminal.fontFamily,
  'terminal.lineHeight' => settings.terminal.lineHeight,
  'terminal.cursorShape' => settings.terminal.cursorShape.name,
  'terminal.cursorBlink' => settings.terminal.cursorBlink,
  'terminal.themeName' => settings.terminal.themeName,
  'terminal.backgroundOpacity' => settings.terminal.backgroundOpacity,
  'terminal.tuiScrollSensitivity' => settings.terminal.tuiScrollSensitivity,
  'terminal.clipboardOnSelect' => settings.terminal.clipboardOnSelect,
  'editor.themeName' => settings.editor.themeName,
  'editor.autosaveDelaySeconds' => settings.editor.autosaveDelaySeconds,
  'agents.agentStatusNotificationsEnabled' =>
    settings.agents.agentStatusNotificationsEnabled,
  'agents.agentStatusFinishedNotificationsEnabled' =>
    settings.agents.agentStatusFinishedNotificationsEnabled,
  'agents.showTabTitlesInSidebar' => settings.agents.showTabTitlesInSidebar,
  'editor.quickOpenExcludedDirectories' =>
    settings.editor.quickOpenExcludedDirectories,
  _ => throw ArgumentError.value(path, 'path', 'Unsupported desktop setting'),
};

/// Host settings writes are also partial, so Classic never resends a stale
/// safety switch, default Agent Profile, quota or AI setting from another
/// window merely because a different preference was edited.
Map<String, Object?> _changedRuntimeFields(
  Map<String, Object?>? before,
  Map<String, Object?> current, {
  required bool excludeShared,
}) {
  const shared = <String>{
    'confirmProjectRemoval',
    'confirmWorkspaceRemoval',
    'autoArchiveWorkspacesAfterDays',
    'defaultAgentProfileId',
  };
  return {
    for (final entry in current.entries)
      if ((!excludeShared || !shared.contains(entry.key)) &&
          (before == null ||
              !before.containsKey(entry.key) ||
              jsonEncode(before[entry.key]) != jsonEncode(entry.value)))
        entry.key: entry.value,
  };
}

/// Preserve all unknown/future portable keys. A settings save writes only
/// fields changed in this Classic process, not GPUI's newer field values.
Map<String, Object?> _changedPortableFields(
  Map<String, Object?>? before,
  Map<String, Object?> current, {
  required bool excludeShared,
}) {
  final result = <String, Object?>{};
  for (final section in current.entries) {
    final earlier = _asMap(before?[section.key]);
    final changed = <String, Object?>{};
    for (final field in _asMap(section.value).entries) {
      final path = '${section.key}.${field.key}';
      if (excludeShared &&
          const {
            'terminal.fontSize',
            'editor.autosaveEnabled',
            'editor.tabSize',
            'general.confirmProjectRemoval',
            'general.confirmWorkspaceRemoval',
            'general.autoArchiveWorkspacesAfterDays',
            'agents.defaultAgentProfileId',
            'general.showTrayIcon',
            'general.showDockBadge',
            'general.showTrayBadge',
            'terminal.fontFamily',
            'terminal.lineHeight',
            'terminal.cursorShape',
            'terminal.cursorBlink',
            'terminal.themeName',
            'terminal.backgroundOpacity',
            'terminal.tuiScrollSensitivity',
            'terminal.clipboardOnSelect',
            'editor.themeName',
            'editor.autosaveDelaySeconds',
            'agents.agentStatusNotificationsEnabled',
            'agents.agentStatusFinishedNotificationsEnabled',
            'agents.showTabTitlesInSidebar',
            'editor.quickOpenExcludedDirectories',
          }.contains(path)) {
        continue;
      }
      if (before == null ||
          !earlier.containsKey(field.key) ||
          jsonEncode(earlier[field.key]) != jsonEncode(field.value)) {
        changed[field.key] = field.value;
      }
    }
    if (changed.isNotEmpty) result[section.key] = changed;
  }
  return result;
}

/// Apply the Host's reviewed non-secret settings projection to the existing
/// Classic model. Unshared local Drift preferences stay intact.
AleraSettings _applySharedDesktopValues(
  AleraSettings current,
  Map<String, Object?> values,
) {
  final rawLanguage = values['general.language'];
  final language = rawLanguage is String
      ? AppLanguage.values.firstWhere(
          (item) => item.name == rawLanguage,
          orElse: () => current.general.language,
        )
      : current.general.language;
  final fetchInterval = values['general.autoGitFetchIntervalMinutes'];
  final archiveDays = values['general.autoArchiveWorkspacesAfterDays'];
  final fontSize = values['terminal.fontSize'];
  final tabSize = values['editor.tabSize'];
  final lineHeight = values['terminal.lineHeight'];
  final backgroundOpacity = values['terminal.backgroundOpacity'];
  final scrollSensitivity = values['terminal.tuiScrollSensitivity'];
  final autosaveDelay = values['editor.autosaveDelaySeconds'];
  final excludedDirs = values['editor.quickOpenExcludedDirectories'];
  final cursorShape = values['terminal.cursorShape'];
  return current.copyWith(
    general: current.general.copyWith(
      language: language,
      autoGitFetchEnabled: values['general.autoGitFetchEnabled'] is bool
          ? values['general.autoGitFetchEnabled'] as bool
          : current.general.autoGitFetchEnabled,
      autoGitFetchIntervalMinutes:
          fetchInterval is int && fetchInterval >= 1 && fetchInterval <= 60
          ? fetchInterval
          : current.general.autoGitFetchIntervalMinutes,
      confirmProjectRemoval: values['general.confirmProjectRemoval'] is bool
          ? values['general.confirmProjectRemoval'] as bool
          : current.general.confirmProjectRemoval,
      confirmWorkspaceRemoval: values['general.confirmWorkspaceRemoval'] is bool
          ? values['general.confirmWorkspaceRemoval'] as bool
          : current.general.confirmWorkspaceRemoval,
      autoArchiveWorkspacesAfterDays:
          archiveDays is int && archiveDays >= 0 && archiveDays <= 36500
          ? archiveDays
          : current.general.autoArchiveWorkspacesAfterDays,
      showTrayIcon: values['general.showTrayIcon'] is bool
          ? values['general.showTrayIcon'] as bool
          : current.general.showTrayIcon,
      showDockBadge: values['general.showDockBadge'] is bool
          ? values['general.showDockBadge'] as bool
          : current.general.showDockBadge,
      showTrayBadge: values['general.showTrayBadge'] is bool
          ? values['general.showTrayBadge'] as bool
          : current.general.showTrayBadge,
    ),
    terminal: current.terminal.copyWith(
      fontSize: fontSize is num && fontSize >= 8 && fontSize <= 48
          ? fontSize.toDouble()
          : current.terminal.fontSize,
      fontFamily: values['terminal.fontFamily'] is String
          ? values['terminal.fontFamily'] as String
          : current.terminal.fontFamily,
      lineHeight: lineHeight is num && lineHeight >= 0.8 && lineHeight <= 2.4
          ? lineHeight.toDouble()
          : current.terminal.lineHeight,
      cursorShape: cursorShape is String
          ? TerminalCursorShape.values.firstWhere(
              (item) => item.name == cursorShape,
              orElse: () => current.terminal.cursorShape,
            )
          : current.terminal.cursorShape,
      cursorBlink: values['terminal.cursorBlink'] is bool
          ? values['terminal.cursorBlink'] as bool
          : current.terminal.cursorBlink,
      themeName: values['terminal.themeName'] is String
          ? values['terminal.themeName'] as String
          : current.terminal.themeName,
      backgroundOpacity:
          backgroundOpacity is num &&
              backgroundOpacity >= 0 &&
              backgroundOpacity <= 1
          ? backgroundOpacity.toDouble()
          : current.terminal.backgroundOpacity,
      tuiScrollSensitivity:
          scrollSensitivity is int &&
              scrollSensitivity >= 1 &&
              scrollSensitivity <= 10
          ? scrollSensitivity
          : current.terminal.tuiScrollSensitivity,
      clipboardOnSelect: values['terminal.clipboardOnSelect'] is bool
          ? values['terminal.clipboardOnSelect'] as bool
          : current.terminal.clipboardOnSelect,
    ),
    editor: current.editor.copyWith(
      twoRowTabs: values['editor.twoRowTabs'] is bool
          ? values['editor.twoRowTabs'] as bool
          : current.editor.twoRowTabs,
      autosaveEnabled: values['editor.autosaveEnabled'] is bool
          ? values['editor.autosaveEnabled'] as bool
          : current.editor.autosaveEnabled,
      tabSize: tabSize is int && tabSize >= 1 && tabSize <= 16
          ? tabSize
          : current.editor.tabSize,
      themeName: values['editor.themeName'] is String
          ? values['editor.themeName'] as String
          : current.editor.themeName,
      autosaveDelaySeconds:
          autosaveDelay is int && autosaveDelay >= 1 && autosaveDelay <= 60
          ? autosaveDelay
          : current.editor.autosaveDelaySeconds,
      quickOpenExcludedDirectories:
          excludedDirs is List && excludedDirs.every((item) => item is String)
          ? List<String>.from(excludedDirs)
          : current.editor.quickOpenExcludedDirectories,
    ),
    agents: current.agents
        .copyWith(
          agentStatusNotificationsEnabled:
              values['agents.agentStatusNotificationsEnabled'] is bool
              ? values['agents.agentStatusNotificationsEnabled'] as bool
              : current.agents.agentStatusNotificationsEnabled,
          agentStatusFinishedNotificationsEnabled:
              values['agents.agentStatusFinishedNotificationsEnabled'] is bool
              ? values['agents.agentStatusFinishedNotificationsEnabled'] as bool
              : current.agents.agentStatusFinishedNotificationsEnabled,
          showTabTitlesInSidebar:
              values['agents.showTabTitlesInSidebar'] is bool
              ? values['agents.showTabTitlesInSidebar'] as bool
              : current.agents.showTabTitlesInSidebar,
        )
        .copyWith(
          defaultAgentProfileId:
              values.containsKey('agents.defaultAgentProfileId')
              ? _optionalRuntimeString(values['agents.defaultAgentProfileId'])
              : current.agents.defaultAgentProfileId,
        ),
  );
}

Map<String, Object?> _asMap(Object? value) {
  if (value is Map<String, Object?>) {
    return value;
  }
  if (value is Map) {
    return Map<String, Object?>.from(value);
  }
  return const <String, Object?>{};
}

String? _optionalRuntimeString(Object? value) {
  if (value is String) {
    final trimmed = value.trim();
    if (trimmed.isNotEmpty) {
      return trimmed;
    }
  }
  return null;
}
