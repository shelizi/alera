import 'package:alera/src/features/settings/application/settings_repository.dart';
import 'package:alera/src/features/ai_assist/domain/ai_assist_settings.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:alera/src/features/settings/infra/runtime_settings_repository.dart';
import 'package:alera/src/features/text_actions/domain/text_actions_settings.dart';
import 'package:alera/src/platform/runtime_host/protocol/terminal_host_protocol.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'MCP authoritative reads use Host values and reject stale fallbacks',
    () async {
      final legacy = _MemorySettingsRepository();
      final host = _RecordingRuntimeHostClient()..configurationSupported = true;
      host.responses['configuration.settings.get'] = <String, Object?>{
        'editor': <String, Object?>{'tabSize': 5},
      };
      host.responses['runtimeSettings.get'] = <String, Object?>{
        'confirmWorkspaceRemoval': true,
      };
      final repository = RuntimeSettingsRepository(
        client: host,
        legacyRepository: legacy,
      );
      expect(legacy.settings.editor.tabSize, 4);
      expect(await repository.readAuthoritativeMcpSetting('editor.tabSize'), 5);
      expect(
        await repository.readAuthoritativeMcpSetting(
          'general.confirmWorkspaceRemoval',
        ),
        true,
      );
      expect(legacy.settings.editor.tabSize, 4);

      host.responses['configuration.settings.get'] = <String, Object?>{
        'editor': <String, Object?>{},
      };
      await expectLater(
        repository.readAuthoritativeMcpSetting('editor.tabSize'),
        throwsStateError,
      );
      host.configurationSupported = false;
      await expectLater(
        repository.readAuthoritativeMcpSetting('editor.tabSize'),
        throwsStateError,
      );
      await expectLater(
        repository.readAuthoritativeMcpSetting('agents.quotas'),
        throwsArgumentError,
      );
    },
  );

  test(
    'portable runtime values refresh the cache without exporting local paths',
    () async {
      final legacy = _MemorySettingsRepository();
      final client = _RecordingRuntimeHostClient()
        ..configurationSupported = true;
      client.responses['configuration.settings.get'] = {
        'terminal': {'fontSize': 19.0},
        'general': {'workspaceDirectory': '/remote/path'},
      };
      client.responses['runtimeSettings.get'] = {
        'workspaceDirectory': '/local/path',
      };
      final repository = RuntimeSettingsRepository(
        client: client,
        legacyRepository: legacy,
      );
      final result = await repository.load();
      expect(result.terminal.fontSize, 19);
      expect(result.general.workspaceDirectory, '/local/path');
      expect(legacy.settings.terminal.fontSize, 19);
      final seed =
          client.payloads['configuration.settings.seed']!.single['settings']!
              as Map;
      expect(seed['general'], isNot(contains('workspaceDirectory')));
      expect(seed['agents'], isNot(contains('quotas')));
      await repository.save(result);
      expect(
        client.payloads['configuration.settings.update'],
        isNull,
        reason: 'No-op Classic saves must not replay stale portable settings over GPUI',
      );
    },
  );
  test(
    'a failed runtime save leaves the compatibility cache unchanged',
    () async {
      final legacy = _MemorySettingsRepository();
      final client = _RecordingRuntimeHostClient()
        ..configurationSupported = true
        ..failConfigurationUpdate = true;
      final repository = RuntimeSettingsRepository(
        client: client,
        legacyRepository: legacy,
      );
      final original = legacy.settings;
      await expectLater(
        repository.save(
          original.copyWith(terminal: original.terminal.copyWith(fontSize: 21)),
        ),
        throwsStateError,
      );
      expect(legacy.settings, original);
      expect(client.payloads, isNot(contains('runtimeSettings.update')));
    },
  );
  test(
    'RuntimeSettingsRepository sends structured portable settings',
    () async {
      final client = _RecordingRuntimeHostClient();
      final repository = RuntimeSettingsRepository(
        client: client,
        legacyRepository: _MemorySettingsRepository(),
      );

      await repository.save(
        AleraSettings.defaults.copyWith(
          aiAssist: const AiAssistSettings(
            selectedThinkingByOperation:
                <AiAssistOperation, Map<String, String>>{
                  AiAssistOperation.commitMessage: <String, String>{
                    'gpt-5.5': 'high',
                  },
                },
          ),
        ),
      );

      final payload = client.payloads['runtimeSettings.update']!.single;
      expect(payload['agentStatusHooks'], isA<Map<String, Object?>>());
      expect(payload['agentQuotas'], isA<Map<String, Object?>>());
      expect(payload['defaultAgentProfileId'], isNull);
      expect(payload['aiTextGeneration'], isA<Map<String, Object?>>());
      final aiAssist = payload['aiTextGeneration']! as Map<String, Object?>;
      expect(aiAssist['agent'], 'codex');
      expect(
        aiAssist['promptSettingsByOperation'],
        isA<Map<String, Object?>>(),
      );
      expect(aiAssist['selectedThinkingByOperation'], <String, Object?>{
        'commitMessage': <String, String>{'gpt-5.5': 'high'},
      });
      expect(aiAssist, isNot(contains('discoveredModelsByAgent')));
      expect(
        (payload['agentQuotas']! as Map<String, Object?>)['enabledProviders'],
        <String>[
          'claude',
          'codex',
          'kimi',
          'grok',
          'cursor',
          'agy',
          'minimax',
          'zai',
          'devin',
          'opencode',
        ],
      );
    },
  );

  test(
    'load preserves UI-only quota fields the runtime does not store',
    () async {
      final legacyRepository = _MemorySettingsRepository();
      legacyRepository.settings = AleraSettings.defaults.copyWith(
        agents: AleraSettings.defaults.agents.copyWith(
          quotas: AgentQuotaSettings.defaults.withHost(
            'local',
            const AgentQuotaHostSettings(
              selectedClaudeProfile: 'leynierdev',
              selectedCodexProfile: '/codex-work',
              unpinnedQuotaKeys: <String>['codex', 'claude:leynierdev'],
            ),
          ),
        ),
      );
      final client = _RecordingRuntimeHostClient();
      client.responses['runtimeSettings.get'] = <String, Object?>{
        'workspaceDirectory': '/tmp/workspaces',
        'defaultAgentProfileId': 'prof_2',
        'agentQuotas': <String, Object?>{
          'enabledProviders': <String>['claude', 'codex'],
          'claudeDefaultEnabled': false,
        },
      };
      final repository = RuntimeSettingsRepository(
        client: client,
        legacyRepository: legacyRepository,
      );

      final loaded = await repository.load();
      final local = loaded.agents.quotas.forHost('local');

      expect(loaded.agents.defaultAgentProfileId, 'prof_2');
      expect(local.enabledProviders, <AgentQuotaProviderId>[
        AgentQuotaProviderId.claude,
        AgentQuotaProviderId.codex,
      ]);
      expect(local.claudeDefaultEnabled, isFalse);
      expect(local.claudeDefaultShowInUsage, isTrue);
      expect(local.selectedClaudeProfile, 'leynierdev');
      expect(local.selectedCodexProfile, '/codex-work');
      expect(local.unpinnedQuotaKeys, <String>['codex', 'claude:leynierdev']);
    },
  );

  test(
    'auto-archive days round-trip through runtime settings payloads',
    () async {
      final client = _RecordingRuntimeHostClient();
      final repository = RuntimeSettingsRepository(
        client: client,
        legacyRepository: _MemorySettingsRepository(),
      );

      await repository.save(
        AleraSettings.defaults.copyWith(
          general: AleraSettings.defaults.general.copyWith(
            autoArchiveWorkspacesAfterDays: 14,
          ),
        ),
      );
      expect(
        client
            .payloads['runtimeSettings.update']!
            .single['autoArchiveWorkspacesAfterDays'],
        14,
      );

      client.responses['runtimeSettings.get'] = <String, Object?>{
        'workspaceDirectory': '/tmp/workspaces',
        'autoArchiveWorkspacesAfterDays': 0,
      };
      final loaded = await repository.load();
      expect(loaded.general.autoArchiveWorkspacesAfterDays, 0);

      // Older hosts that never heard of the key keep the default.
      client.responses['runtimeSettings.get'] = <String, Object?>{};
      final legacy = _MemorySettingsRepository();
      legacy.settings = AleraSettings.defaults.copyWith(
        general: AleraSettings.defaults.general.copyWith(
          autoArchiveWorkspacesAfterDays: 45,
        ),
      );
      final legacyRepository = RuntimeSettingsRepository(
        client: client,
        legacyRepository: legacy,
      );
      final loadedLegacy = await legacyRepository.load();
      expect(loadedLegacy.general.autoArchiveWorkspacesAfterDays, 45);
    },
  );

  test('save sends the selected default agent profile', () async {
    final client = _RecordingRuntimeHostClient();
    final repository = RuntimeSettingsRepository(
      client: client,
      legacyRepository: _MemorySettingsRepository(),
    );
    final settings = AleraSettings.defaults.copyWith(
      agents: AleraSettings.defaults.agents.copyWith(
        defaultAgentProfileId: 'prof_2',
      ),
    );

    await repository.save(settings);

    expect(
      client
          .payloads['runtimeSettings.update']!
          .single['defaultAgentProfileId'],
      'prof_2',
    );
  });

  test(
    'load combines shared AI Assist execution settings with local discovery',
    () async {
      final legacyRepository = _MemorySettingsRepository();
      legacyRepository.settings = AleraSettings.defaults.copyWith(
        aiAssist: const AiAssistSettings(
          discoveredModelsByAgent: <String, List<AiAssistDiscoveredModel>>{
            'codex': <AiAssistDiscoveredModel>[
              AiAssistDiscoveredModel(id: 'local-model', label: 'Local Model'),
            ],
          },
        ),
      );
      final client = _RecordingRuntimeHostClient();
      client.responses['runtimeSettings.get'] = <String, Object?>{
        'workspaceDirectory': '/tmp/workspaces',
        'aiTextGeneration': <String, Object?>{
          'enabled': true,
          'agent': 'claude',
          'selectedModelByAgent': <String, String>{'claude': 'opus'},
          'selectedThinkingByOperation': <String, Object?>{
            'workspaceIdentity': <String, String>{'opus': 'high'},
          },
          'instructionsByOperation': <String, String>{
            'workspaceIdentity': 'Use feature branches.',
          },
          'promptSettingsByOperation': <String, Object?>{
            'workspaceIdentity': <String, Object?>{
              'agent': 'claude',
              'model': 'opus',
            },
          },
          'timeoutSeconds': 180,
        },
      };
      final repository = RuntimeSettingsRepository(
        client: client,
        legacyRepository: legacyRepository,
      );

      final loaded = await repository.load();

      expect(loaded.aiAssist.agent, 'claude');
      expect(
        loaded.aiAssist.instructionsFor(.workspaceIdentity),
        'Use feature branches.',
      );
      expect(
        loaded.aiAssist.agentTypeFor(.workspaceIdentity),
        AgentType.claude,
      );
      expect(loaded.aiAssist.modelForOperation(.workspaceIdentity), 'opus');
      expect(
        loaded.aiAssist.thinkingForOperation(.workspaceIdentity, 'opus'),
        'high',
      );
      expect(
        loaded.aiAssist.discoveredModelsForType(AgentType.codex).single.id,
        'local-model',
      );
    },
  );

  test(
    'saves and loads text actions while older hosts fall back locally',
    () async {
      final legacyRepository = _MemorySettingsRepository();
      final action = const TextAction(
        id: 'polish',
        name: 'Polish',
        prompt: 'Improve the selected text.',
      );
      final client = _RecordingRuntimeHostClient();
      final repository = RuntimeSettingsRepository(
        client: client,
        legacyRepository: legacyRepository,
      );

      await repository.save(
        AleraSettings.defaults.copyWith(
          textActions: TextActionsSettings(actions: <TextAction>[action]),
        ),
      );
      final payload = client.payloads['runtimeSettings.update']!.single;
      expect(payload['textActions'], <String, Object?>{
        'actions': <Map<String, Object?>>[
          <String, Object?>{
            'id': 'polish',
            'name': 'Polish',
            'prompt': 'Improve the selected text.',
            'enabled': true,
            'agentOverride': null,
            'modelOverride': null,
            'reasoningByModel': <String, String>{},
          },
        ],
      });

      legacyRepository.settings = AleraSettings.defaults.copyWith(
        textActions: TextActionsSettings(actions: <TextAction>[action]),
      );
      client.responses['runtimeSettings.get'] = <String, Object?>{
        'workspaceDirectory': '/tmp/workspaces',
      };
      final loadedFromOldHost = await repository.load();
      expect(loadedFromOldHost.textActions.actions.single.id, 'polish');

      client.responses['runtimeSettings.get'] = <String, Object?>{
        'workspaceDirectory': '/tmp/workspaces',
        'textActions': <String, Object?>{'actions': <Object?>[]},
      };
      final loadedFromNewHost = await repository.load();
      expect(loadedFromNewHost.textActions.actions, isEmpty);
    },
  );
  test('restoring absent portable settings uses defaults and preserves local fields', () async {
    final client = _RecordingRuntimeHostClient()..configurationSupported = true;
    final legacy = _MemorySettingsRepository();
    legacy.settings = AleraSettings.defaults.copyWith(
      terminal: AleraSettings.defaults.terminal.copyWith(
        fontSize: 20,
        scrollbackLines: 1234,
      ),
      general: AleraSettings.defaults.general.copyWith(
        workspaceDirectory: '/local',
      ),
    );
    client.responses['configuration.settings.get'] = <String, Object?>{
      'terminal': <String, Object?>{},
    };
    client.responses['runtimeSettings.get'] = <String, Object?>{
      'workspaceDirectory': '/local',
    };
    final actual = await RuntimeSettingsRepository(
      client: client,
      legacyRepository: legacy,
    ).load();
    expect(actual.terminal.fontSize, AleraSettings.defaults.terminal.fontSize);
    expect(actual.terminal.scrollbackLines, 1234);
    expect(actual.general.workspaceDirectory, '/local');
  });
  test(
    'unknown shortcuts do not prevent known portable preferences loading',
    () async {
      final client = _RecordingRuntimeHostClient()
        ..configurationSupported = true;
      final legacy = _MemorySettingsRepository();
      client.responses['configuration.settings.get'] = {
        'terminal': {'fontSize': 20},
        'keyboard': {
          'overrides': {
            'openSettings': ['Mod+Comma'],
            'futureAction': ['Mod+F12'],
          },
        },
      };
      client.responses['runtimeSettings.get'] = {'workspaceDirectory': null};
      final repository = RuntimeSettingsRepository(
        client: client,
        legacyRepository: legacy,
      );
      final actual = await repository.load();
      expect(actual.terminal.fontSize, 20);
      expect(actual.keyboard.overrides.keys.map((id) => id.name), [
        'openSettings',
      ]);
      await repository.save(
        actual.copyWith(terminal: actual.terminal.copyWith(fontSize: 21)),
      );
      final saved = client.payloads['configuration.settings.update']!.single;
      expect(saved['supportedKeyboardActionIds'], contains('openSettings'));
      expect(
        saved['supportedKeyboardActionIds'],
        isNot(contains('futureAction')),
      );
    },
  );
  test('Classic reads and CAS-writes GPUI portable theme, terminal and Agent preferences without stale section overwrite', () async {
    final legacy = _MemorySettingsRepository();
    final host = _RecordingRuntimeHostClient()..configurationSupported = true;
    host.responses['desktopSettings.seedLocal'] = {
      'values': {
        'terminal.fontFamily': 'Cascadia Code',
        'terminal.lineHeight': 1.6,
        'terminal.cursorShape': 'bar',
        'terminal.cursorBlink': true,
        'terminal.themeName': 'Dracula',
        'terminal.backgroundOpacity': 0.8,
        'terminal.tuiScrollSensitivity': 2,
        'terminal.clipboardOnSelect': true,
        'editor.themeName': 'Tokyo Night Dark',
        'editor.autosaveDelaySeconds': 7,
        'editor.quickOpenExcludedDirectories': <String>[
          'node_modules',
          'generated',
        ],
        'general.showTrayIcon': false,
        'general.showDockBadge': false,
        'general.showTrayBadge': true,
        'agents.agentStatusNotificationsEnabled': true,
        'agents.agentStatusFinishedNotificationsEnabled': true,
        'agents.showTabTitlesInSidebar': true,
      },
    };
    host.responses['runtimeSettings.get'] = {
      'workspaceDirectory': '/shared-workspaces',
    };
    final repo = RuntimeSettingsRepository(
      client: host,
      legacyRepository: legacy,
    );
    final loaded = await repo.load();
    expect(loaded.terminal.fontFamily, 'Cascadia Code');
    expect(loaded.terminal.lineHeight, 1.6);
    expect(loaded.terminal.cursorShape, TerminalCursorShape.bar);
    expect(loaded.terminal.backgroundOpacity, 0.8);
    expect(loaded.terminal.clipboardOnSelect, isTrue);
    expect(loaded.editor.themeName, 'Tokyo Night Dark');
    expect(loaded.editor.autosaveDelaySeconds, 7);
    expect(loaded.editor.quickOpenExcludedDirectories, [
      'node_modules',
      'generated',
    ]);
    expect(loaded.general.showTrayIcon, isFalse);
    expect(loaded.agents.showTabTitlesInSidebar, isTrue);
    await repo.compareAndSetSharedDesktopChanges(
      loaded,
      loaded.copyWith(
        editor: loaded.editor.copyWith(
          quickOpenExcludedDirectories: [
            ...loaded.editor.quickOpenExcludedDirectories,
          ],
        ),
      ),
    );
    expect(
      host.payloads['desktopSettings.compareAndSetBatch'],
      isNull,
      reason:
          'Structurally identical List values must not generate spurious CAS',
    );
    final changed = loaded.copyWith(
      terminal: loaded.terminal.copyWith(lineHeight: 2.0),
      editor: loaded.editor.copyWith(
        autosaveDelaySeconds: 10,
        quickOpenExcludedDirectories: ['node_modules', 'vendor'],
      ),
    );
    await repo.compareAndSetSharedDesktopChanges(loaded, changed);
    expect(host.payloads['desktopSettings.compareAndSetBatch'], [
      {
        'changes': [
          {'path': 'terminal.lineHeight', 'expectedValue': 1.6, 'value': 2.0},
          {
            'path': 'editor.autosaveDelaySeconds',
            'expectedValue': 7,
            'value': 10,
          },
          {
            'path': 'editor.quickOpenExcludedDirectories',
            'expectedValue': ['node_modules', 'generated'],
            'value': ['node_modules', 'vendor'],
          },
        ],
      },
    ]);
    await repo.save(changed);
    expect(
      host.payloads['configuration.settings.update'],
      isNull,
      reason: 'Classic must not replay whole portable sections after GPUI CAS writes',
    );
    expect(host.payloads['runtimeSettings.update'], isNull);
  });

  test('Classic imports Drift only once and uses Rust CAS without rewriting GPUI portable values', () async {
    final legacy = _MemorySettingsRepository();
    legacy.settings = AleraSettings.defaults.copyWith(
      general: AleraSettings.defaults.general.copyWith(
        language: AppLanguage.traditionalChinese,
        autoGitFetchEnabled: false,
        autoGitFetchIntervalMinutes: 12,
      ),
    );
    final host = _RecordingRuntimeHostClient()..configurationSupported = true;
    host.responses['desktopSettings.seedLocal'] = {
      'values': {
        'general.language': 'japanese',
        'general.autoGitFetchEnabled': true,
        'general.autoGitFetchIntervalMinutes': 8,
        'terminal.fontSize': 20.0,
        'editor.autosaveEnabled': true,
        'editor.tabSize': 2,
        'general.confirmProjectRemoval': false,
        'general.confirmWorkspaceRemoval': true,
        'general.autoArchiveWorkspacesAfterDays': 7,
        'agents.defaultAgentProfileId': 'prof_alpha',
      },
    };
    host.responses['runtimeSettings.get'] = {
      'workspaceDirectory': '/native-and-classic',
    };
    final repo = RuntimeSettingsRepository(
      client: host,
      legacyRepository: legacy,
    );
    final loaded = await repo.load();
    expect(loaded.general.language, AppLanguage.japanese);
    expect(loaded.general.autoGitFetchEnabled, true);
    expect(loaded.general.autoGitFetchIntervalMinutes, 8);
    expect(loaded.terminal.fontSize, 20);
    expect(loaded.editor.autosaveEnabled, true);
    expect(loaded.editor.tabSize, 2);
    expect(loaded.general.confirmProjectRemoval, false);
    expect(loaded.general.autoArchiveWorkspacesAfterDays, 7);
    expect(loaded.agents.defaultAgentProfileId, 'prof_alpha');
    final seeded =
        host.payloads['desktopSettings.seedLocal']!.single['values']! as Map;
    expect(seeded['general.language'], 'traditionalChinese');

    final edited = loaded.copyWith(
      general: loaded.general.copyWith(
        language: AppLanguage.english,
        confirmProjectRemoval: true,
      ),
      terminal: loaded.terminal.copyWith(fontSize: 22),
      agents: loaded.agents.copyWith(defaultAgentProfileId: null),
    );
    await repo.compareAndSetSharedDesktopChanges(loaded, edited);
    expect(host.payloads['desktopSettings.compareAndSetBatch'], [
      {
        'changes': [
          {
            'path': 'general.language',
            'expectedValue': 'japanese',
            'value': 'english',
          },
          {'path': 'terminal.fontSize', 'expectedValue': 20.0, 'value': 22.0},
          {
            'path': 'general.confirmProjectRemoval',
            'expectedValue': false,
            'value': true,
          },
          {
            'path': 'agents.defaultAgentProfileId',
            'expectedValue': 'prof_alpha',
            'value': null,
          },
        ],
      },
    ]);
    await repo.save(edited);
    expect(
      host.payloads['configuration.settings.update'],
      isNull,
      reason:
          'All shared portable fields are CAS-owned by the Rust Runtime Host',
    );
    expect(
      host.payloads['runtimeSettings.update'],
      isNull,
      reason: 'Shared safety and profile settings are already CAS-owned by the Rust Runtime Host',
    );
  });
}

final class _RecordingRuntimeHostClient
    implements RuntimeHostClient, RuntimeHostCapabilityClient {
  final payloads = <String, List<Map<String, Object?>>>{};
  final responses = <String, Object?>{};
  bool configurationSupported = false;
  bool failConfigurationUpdate = false;

  @override
  Future<bool> supportsRuntimeCapability(String capability) async =>
      configurationSupported && capability == 'configurationSyncV1';

  @override
  Stream<RuntimeHostEvent> get runtimeEvents => const Stream.empty();

  @override
  Future<Object?> runtimeRequest(
    String type, [
    Map<String, Object?> payload = const <String, Object?>{},
    Duration? timeout,
  ]) async {
    payloads.putIfAbsent(type, () => <Map<String, Object?>>[]).add(payload);
    if (type == 'desktopSettings.seedLocal' && !responses.containsKey(type)) {
      throw StateError('Older Host: shared desktop settings not supported');
    }
    if (type == 'configuration.settings.update' && failConfigurationUpdate) {
      throw StateError('Runtime unavailable');
    }
    return responses[type];
  }
}

final class _MemorySettingsRepository implements SettingsRepository {
  AleraSettings settings = .defaults;

  @override
  Future<AleraSettings> load() async => settings;

  @override
  Future<void> save(AleraSettings settings) async {
    this.settings = settings;
  }
}
