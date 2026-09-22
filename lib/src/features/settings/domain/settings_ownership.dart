import 'package:alera/src/features/ai_assist/domain/ai_assist_settings.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:alera_configuration/alera_configuration.dart';

/// Explicit tiers of settings ownership in Alera.
///
/// Settings fall into three distinct architectural categories:
/// 1. [localOnlyUi]: Settings that are local to this specific device/window UI
///    and never leave the machine. Stored in local Drift storage.
/// 2. [runtimeOperational]: Settings that configure and govern the local
///    terminal-host runtime daemon (`ServerActor` / `runtimeMetadata`).
/// 3. [portableConfiguration]: Preferences that are synchronized across devices
///    or to the cloud via the portable configuration sync document.
enum SettingsOwnershipTier {
  /// Preferences stored only in local SQLite that never leave this client.
  localOnlyUi('Local Device'),

  /// Operational settings that configure the terminal-host sidecar.
  runtimeOperational('Runtime Host'),

  /// Portable preferences that sync to cloud and paired devices.
  portableConfiguration('Cloud Synced');

  SettingsOwnershipTier(this.label);

  final String label;
}

/// Helper methods that partition and extract [AleraSettings] according to
/// the three settings ownership tiers.
extension AleraSettingsOwnershipExtension on AleraSettings {
  /// Extracts the projection of settings governed by the runtime operational tier.
  Map<String, Object?> toRuntimeOperationalMap() {
    return <String, Object?>{
      'workspaceDirectory': general.workspaceDirectory,
      'confirmProjectRemoval': general.confirmProjectRemoval,
      'confirmWorkspaceRemoval': general.confirmWorkspaceRemoval,
      'autoArchiveWorkspacesAfterDays': general.autoArchiveWorkspacesAfterDays,
      'defaultAgentProfileId': agents.defaultAgentProfileId,
      'agentStatusHooks': agents.agentStatusHooks.toMap(),
      'agentQuotas': agents.quotas.forHost('local').toMap(),
      'aiTextGeneration': runtimeAiAssistSettings(aiAssist),
      'textActions': textActions.toMap(),
    };
  }

  /// Extracts the portable settings projection using the schema from
  /// `packages/alera_configuration`.
  Map<String, Object?> toPortableConfigurationMap() {
    return portableDesktopSettings(toMap());
  }
}

/// Serializes AI Assist settings to the exact runtime representation expected
/// by the runtime host `runtimeSettings.update` endpoint.
Map<String, Object?> runtimeAiAssistSettings(AiAssistSettings settings) {
  return <String, Object?>{
    'enabled': settings.enabled,
    'autoGenerateAgentTitles': settings.autoGenerateAgentTitles,
    'agent': settings.agent.key,
    'selectedModelByAgent': <String, String>{
      for (final entry in settings.selectedModelByAgent.entries)
        entry.key: entry.value,
    },
    'selectedThinkingByModel': settings.selectedThinkingByModel,
    'selectedThinkingByOperation': <String, Map<String, String>>{
      for (final entry in settings.selectedThinkingByOperation.entries)
        entry.key.key: entry.value,
    },
    'customCommand': settings.customCommand,
    'instructionsByOperation': <String, String>{
      for (final entry in settings.instructionsByOperation.entries)
        entry.key.key: entry.value,
    },
    'promptSettingsByOperation': <String, Map<String, Object?>>{
      for (final entry in settings.promptSettingsByOperation.entries)
        entry.key.key: <String, Object?>{
          if (entry.value.agent != null) 'agent': entry.value.agent!.key,
          if (entry.value.model?.trim().isNotEmpty == true)
            'model': entry.value.model!.trim(),
        },
    },
    'timeoutSeconds': settings.timeoutSeconds,
  };
}
