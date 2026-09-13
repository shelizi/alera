import 'package:alera/src/features/ai_assist/domain/ai_assist_settings.dart';
import 'package:alera/src/features/ai_dictation/domain/ai_dictation_settings.dart';
import 'package:alera/src/features/keyboard/domain/keyboard_action.dart';
import 'package:alera/src/features/keyboard/domain/keyboard_shortcut_settings.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:alera/src/features/settings/domain/settings_ownership.dart';
import 'package:alera/src/features/text_actions/domain/text_actions_settings.dart';
import 'package:alera_configuration/alera_configuration.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('settings ownership maps', () {
    test(
      'runtime, portable, and local-only contracts cover every persisted field',
      () {
        final settings = _populatedSettings();
        final serialized = settings.toMap();
        final runtime = settings.toRuntimeOperationalMap();
        final portable = settings.toPortableConfigurationMap();
        final serializedPaths = _sectionFieldPaths(serialized);

        expect(serializedPaths, unorderedEquals(_classifiedFieldPaths));
        expect(runtime.keys, unorderedEquals(_runtimeOperationalKeys));
        expect(
          (runtime['aiTextGeneration']! as Map<String, Object?>).keys,
          unorderedEquals(_runtimeAiAssistKeys),
        );
        expect(portable.keys, unorderedEquals(desktopPortableFields.keys));
        for (final entry in desktopPortableFields.entries) {
          final section = portable[entry.key]! as Map;
          final source = serialized[entry.key];
          final sourceKeys = source is Map
              ? source.keys.map((key) => key.toString()).toSet()
              : <String>{};
          expect(
            section.keys,
            unorderedEquals(
              entry.value.where(sourceKeys.contains).toList(growable: false),
            ),
            reason: 'portable section ${entry.key} dropped or gained a key',
          );
        }

        expect(runtime['workspaceDirectory'], '/tmp/workspaces');
        expect(runtime['confirmProjectRemoval'], isFalse);
        expect(runtime['confirmWorkspaceRemoval'], isFalse);
        expect(runtime['autoArchiveWorkspacesAfterDays'], 7);
        expect(runtime['defaultAgentProfileId'], 'prof_1');
        expect(
          runtime['agentStatusHooks'],
          settings.agents.agentStatusHooks.toMap(),
        );
        expect(
          runtime['agentQuotas'],
          settings.agents.quotas.forHost('local').toMap(),
        );
        expect(runtime['textActions'], settings.textActions.toMap());
        expect(
          (runtime['aiTextGeneration']! as Map<String, Object?>)['agent'],
          'claude',
        );
        expect(
          (runtime['aiTextGeneration']! as Map<String, Object?>).containsKey(
            'discoveredModelsByAgent',
          ),
          isFalse,
        );
        expect(
          (runtime['aiTextGeneration']! as Map<String, Object?>).containsKey(
            'discoveredDefaultModelByAgent',
          ),
          isFalse,
        );

        expect((portable['general']! as Map)['workspaceDirectory'], isNull);
        expect((portable['general']! as Map)['language'], isNull);
        expect((portable['agents']! as Map)['quotas'], isNull);
        expect((portable['agents']! as Map)['agentStatusHooks'], isNull);
        expect(
          (portable['aiTextGeneration']! as Map)['discoveredModelsByAgent'],
          isNull,
        );
        expect((portable['aiDictation']! as Map)['localModelId'], isNull);
        expect((portable['editor']! as Map)['externalEditor'], isNull);
        expect((portable['terminal']! as Map)['scrollbackLines'], isNull);
        expect((portable['terminal']! as Map)['loginShell'], isNull);
        expect((portable['diagnostics']), isNull);
        expect((portable['general']! as Map)['showTrayIcon'], isFalse);
        expect((portable['keyboard']! as Map)['terminalPolicy'], isNotNull);
      },
    );

    test('a new serialized field must be classified before it can ship', () {
      final unexpected = _sectionFieldPaths(AleraSettings.defaults.toMap())
          .difference(_classifiedFieldPaths);
      expect(
        unexpected,
        isEmpty,
        reason:
            'classify new AleraSettings fields in settings_ownership_test.dart '
            'and docs/settings-ownership.md: $unexpected',
      );
    });
  });
}

const _runtimeOperationalKeys = <String>{
  'workspaceDirectory',
  'confirmProjectRemoval',
  'confirmWorkspaceRemoval',
  'autoArchiveWorkspacesAfterDays',
  'defaultAgentProfileId',
  'agentStatusHooks',
  'agentQuotas',
  'aiTextGeneration',
  'textActions',
};

const _runtimeAiAssistKeys = <String>{
  'enabled',
  'autoGenerateAgentTitles',
  'agent',
  'selectedModelByAgent',
  'selectedThinkingByModel',
  'selectedThinkingByOperation',
  'customCommand',
  'instructionsByOperation',
  'promptSettingsByOperation',
  'timeoutSeconds',
};

/// Every `section.field` path in `AleraSettings.toMap()`. Keep this in sync
/// with `docs/settings-ownership.md`.
const _classifiedFieldPaths = <String>{
  'general.language',
  'general.workspaceDirectory',
  'general.starClicked',
  'general.confirmProjectRemoval',
  'general.confirmWorkspaceRemoval',
  'general.keepAliveEnabled',
  'general.showTrayIcon',
  'general.showDockBadge',
  'general.showTrayBadge',
  'general.showPullRequestStatusInSidebar',
  'general.pullRequestFailureNotificationsEnabled',
  'general.autoArchiveWorkspacesAfterDays',
  'agents.agentStatusHooks',
  'agents.agentStatusNotificationsEnabled',
  'agents.agentStatusFinishedNotificationsEnabled',
  'agents.keepComputerAwakeWhileAgentsWork',
  'agents.showTabTitlesInSidebar',
  'agents.defaultAgentProfileId',
  'agents.quotas',
  'aiTextGeneration.enabled',
  'aiTextGeneration.autoGenerateAgentTitles',
  'aiTextGeneration.agent',
  'aiTextGeneration.selectedModelByAgent',
  'aiTextGeneration.selectedThinkingByModel',
  'aiTextGeneration.selectedThinkingByOperation',
  'aiTextGeneration.discoveredModelsByAgent',
  'aiTextGeneration.discoveredDefaultModelByAgent',
  'aiTextGeneration.customCommand',
  'aiTextGeneration.instructionsByOperation',
  'aiTextGeneration.promptSettingsByOperation',
  'aiTextGeneration.timeoutSeconds',
  'aiDictation.enabled',
  'aiDictation.transcriptionEngine',
  'aiDictation.rewriteMode',
  'aiDictation.providerPolicy',
  'aiDictation.language',
  'aiDictation.localModelId',
  'aiDictation.hostFallbackEnabled',
  'aiDictation.providerFallbackEnabled',
  'aiDictation.remoteBaseUrl',
  'aiDictation.remoteModel',
  'aiDictation.codexRealtimeModel',
  'aiDictation.remoteProvider',
  'aiDictation.timeoutSeconds',
  'aiDictation.remoteConsentVersion',
  'aiDictation.systemRecognitionConsentVersion',
  'textActions.actions',
  'editor.tabSize',
  'editor.themeName',
  'editor.autosaveEnabled',
  'editor.autosaveDelaySeconds',
  'editor.externalEditor',
  'editor.codeOpenTarget',
  'editor.zedExecutableMode',
  'editor.zedExecutablePath',
  'editor.externalEditorWorkspaceMode',
  'editor.autoOpenNewWorkspacesInZed',
  'diagnostics.logLevel',
  'diagnostics.crashReportingEnabled',
  'terminal.fontFamily',
  'terminal.fontSize',
  'terminal.fontWeight',
  'terminal.lineHeight',
  'terminal.paddingX',
  'terminal.paddingY',
  'terminal.cursorShape',
  'terminal.cursorBlink',
  'terminal.cursorOpacity',
  'terminal.themeName',
  'terminal.backgroundOpacity',
  'terminal.wordSeparators',
  'terminal.colorOverrides',
  'terminal.scrollbackLines',
  'terminal.tuiScrollSensitivity',
  'terminal.clipboardOnSelect',
  'terminal.allowOsc52Clipboard',
  'terminal.showComposerByDefault',
  'terminal.toolbarCorner',
  'terminal.hostEmptyShutdownDelaySeconds',
  'terminal.hostDetachedSessionShutdownDelaySeconds',
  'terminal.hostScrollbackBytes',
  'terminal.bufferBudgetMegabytes',
  'terminal.keepRuntimeOpenOnAppQuit',
  'terminal.loginShell',
  'terminal.confirmCloseRunningProcesses',
  'keyboard.overrides',
  'keyboard.terminalPolicy',
};

Set<String> _sectionFieldPaths(Map<String, Object?> settings) {
  final paths = <String>{};
  for (final section in settings.entries) {
    final value = section.value;
    if (value is Map) {
      for (final field in value.keys) {
        paths.add('${section.key}.$field');
      }
    } else {
      paths.add(section.key);
    }
  }
  return paths;
}

AleraSettings _populatedSettings() {
  return AleraSettings.defaults.copyWith(
    general: AleraSettings.defaults.general.copyWith(
      language: AppLanguage.english,
      workspaceDirectory: '/tmp/workspaces',
      starClicked: true,
      confirmProjectRemoval: false,
      confirmWorkspaceRemoval: false,
      keepAliveEnabled: true,
      showTrayIcon: false,
      showDockBadge: false,
      showTrayBadge: false,
      showPullRequestStatusInSidebar: false,
      pullRequestFailureNotificationsEnabled: true,
      autoArchiveWorkspacesAfterDays: 7,
    ),
    agents: AleraSettings.defaults.agents.copyWith(
      agentStatusHooks: const AgentStatusHookSettings(codex: true),
      agentStatusNotificationsEnabled: true,
      defaultAgentProfileId: 'prof_1',
      quotas: AgentQuotaSettings.defaults.withHost(
        'local',
        const AgentQuotaHostSettings(
          selectedClaudeProfile: 'work',
          unpinnedQuotaKeys: <String>['codex'],
        ),
      ),
    ),
    aiAssist: const AiAssistSettings(
      agent: AiAssistAgent.claude,
      discoveredModelsByAgent: <AiAssistAgent, List<AiAssistDiscoveredModel>>{
        AiAssistAgent.claude: <AiAssistDiscoveredModel>[
          AiAssistDiscoveredModel(id: 'opus', label: 'Opus'),
        ],
      },
      discoveredDefaultModelByAgent: <AiAssistAgent, String>{
        AiAssistAgent.claude: 'opus',
      },
    ),
    aiDictation: const AiDictationSettings(
      language: 'es',
      localModelId: 'whisper-local',
      remoteConsentVersion: 2,
      systemRecognitionConsentVersion: 1,
      codexRealtimeModel: 'gpt-realtime',
    ),
    textActions: const TextActionsSettings(
      actions: <TextAction>[
        TextAction(id: 'polish', name: 'Polish', prompt: 'Improve this.'),
      ],
    ),
    editor: AleraSettings.defaults.editor.copyWith(
      tabSize: 2,
      zedExecutablePath: '/usr/bin/zed',
    ),
    terminal: AleraSettings.defaults.terminal.copyWith(
      fontSize: 16,
      scrollbackLines: 1234,
      loginShell: true,
      wordSeparators: ' /',
    ),
    keyboard: const KeyboardShortcutSettings(
      overrides: <KeyboardActionId, List<String>>{
        KeyboardActionId.openSettings: <String>['Mod+Comma'],
      },
      terminalPolicy: TerminalShortcutPolicy.terminalFirst,
    ),
  );
}
