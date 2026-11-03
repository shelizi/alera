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
  @override
  Future<AleraSettings> load() async {
    var legacy = await legacyRepository.load();
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
      final payload = await client.runtimeRequest('runtimeSettings.get');
      final runtime = _asMap(payload);
      if (!runtime.containsKey('workspaceDirectory')) {
        return legacy;
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
      await legacyRepository.save(resolved);
      return resolved;
    } catch (_) {
      return legacy;
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
      await client.runtimeRequest('configuration.settings.update', {
        'settings': settings.toPortableConfigurationMap(),
        'supportedKeyboardActionIds': KeyboardActionId.values
            .map((id) => id.name)
            .toList(),
      });
    }
    await client.runtimeRequest(
      'runtimeSettings.update',
      settings.toRuntimeOperationalMap(),
    );
    await legacyRepository.save(settings);
  }

  Future<bool> _supportsConfiguration() async {
    final capabilityClient = client;
    return capabilityClient is RuntimeHostCapabilityClient &&
        await (capabilityClient as RuntimeHostCapabilityClient)
            .supportsRuntimeCapability('configurationSyncV1');
  }
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
