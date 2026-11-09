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
      if (expected == value) continue;
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
  'general.confirmProjectRemoval',
  'general.confirmWorkspaceRemoval',
  'general.autoArchiveWorkspacesAfterDays',
  'agents.defaultAgentProfileId',
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
  'general.confirmProjectRemoval' => settings.general.confirmProjectRemoval,
  'general.confirmWorkspaceRemoval' => settings.general.confirmWorkspaceRemoval,
  'general.autoArchiveWorkspacesAfterDays' =>
    settings.general.autoArchiveWorkspacesAfterDays,
  'agents.defaultAgentProfileId' => settings.agents.defaultAgentProfileId,
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
    ),
    terminal: current.terminal.copyWith(
      fontSize: fontSize is num && fontSize >= 8 && fontSize <= 48
          ? fontSize.toDouble()
          : current.terminal.fontSize,
    ),
    editor: current.editor.copyWith(
      autosaveEnabled: values['editor.autosaveEnabled'] is bool
          ? values['editor.autosaveEnabled'] as bool
          : current.editor.autosaveEnabled,
      tabSize: tabSize is int && tabSize >= 1 && tabSize <= 16
          ? tabSize
          : current.editor.tabSize,
    ),
    agents: values.containsKey('agents.defaultAgentProfileId')
        ? current.agents.copyWith(
            defaultAgentProfileId: _optionalRuntimeString(
              values['agents.defaultAgentProfileId'],
            ),
          )
        : current.agents,
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
