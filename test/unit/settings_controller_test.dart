import 'package:alera/src/app/dependencies.dart';
import 'package:alera/src/design_system/feedback/alera_toast.dart';
import 'package:alera/src/features/keyboard/domain/keyboard_action.dart';
import 'package:alera/src/features/settings/application/settings_controller.dart';
import 'package:alera/src/features/settings/application/settings_repository.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:alera/src/features/settings/domain/editor_syntax_theme_catalog.dart';
import 'package:alera/src/features/settings/infra/drift_settings_repository.dart';
import 'package:alera/src/platform/runtime_host/protocol/terminal_host_protocol.dart';
import 'package:alera/src/shared/infra/storage/drift_database.dart';
import 'package:drift/native.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('SettingsController', () {
    test('round-trips flat and unknown agent status hook keys', () {
      final hooks = AgentStatusHookSettings.fromJson({
        'codex': true,
        'future-agent': false,
      });

      expect(hooks.isEnabled('codex'), isTrue);
      expect(hooks.isEnabled('future-agent'), isFalse);
      expect(hooks.toMap(), <String, dynamic>{
        'codex': true,
        'future-agent': false,
      });
    });

    test(
      'refreshes and reports a runtime settings revision conflict',
      () async {
        final repository = _ConflictSettingsRepository();
        final container = ProviderContainer(
          overrides: [settingsRepositoryProvider.overrideWithValue(repository)],
        );
        addTearDown(container.dispose);
        final controller = container.read(settingsControllerProvider.notifier);
        await controller.load();

        repository.current = AleraSettings.defaults.copyWith(
          general: AleraSettings.defaults.general.copyWith(
            workspaceDirectory: '/changed-elsewhere',
          ),
        );
        repository.conflictOnSave = true;
        final toast = AleraToast.stream.first;

        await expectLater(
          controller.setConfirmProjectRemoval(false),
          throwsA(
            isA<TerminalHostConflictException>().having(
              (error) => error.code,
              'code',
              'runtime_settings_revision_conflict',
            ),
          ),
        );

        expect(
          container.read(settingsControllerProvider).general.workspaceDirectory,
          '/changed-elsewhere',
        );
        expect((await toast).message, contains('not saved'));
      },
    );

    test('autosaves terminal updates', () async {
      final db = AleraDatabase(executor: NativeDatabase.memory());
      addTearDown(db.close);
      final repository = DriftSettingsRepository(db);
      final container = ProviderContainer(
        overrides: [settingsRepositoryProvider.overrideWithValue(repository)],
      );
      addTearDown(container.dispose);
      final controller = container.read(settingsControllerProvider.notifier);
      await controller.load();

      await controller.updateTerminal(
        (terminal) => terminal.copyWith(fontSize: 18),
      );
      final restored = await repository.load();

      expect(container.read(settingsControllerProvider).terminal.fontSize, 18);
      expect(restored.terminal.fontSize, 18);
    });

    test('resets terminal settings to defaults', () async {
      final db = AleraDatabase(executor: NativeDatabase.memory());
      addTearDown(db.close);
      final repository = DriftSettingsRepository(db);
      final container = ProviderContainer(
        overrides: [settingsRepositoryProvider.overrideWithValue(repository)],
      );
      addTearDown(container.dispose);
      final controller = container.read(settingsControllerProvider.notifier);
      await controller.load();

      await controller.updateTerminal(
        (terminal) => terminal.copyWith(fontFamily: 'Menlo', cursorShape: .bar),
      );
      await controller.resetTerminalSettings();

      expect(
        container.read(settingsControllerProvider).terminal.fontFamily,
        'JetBrains Mono',
      );
      expect(
        container.read(settingsControllerProvider).terminal.cursorShape,
        TerminalCursorShape.block,
      );
      expect((await repository.load()).terminal.fontFamily, 'JetBrains Mono');
    });

    test('pins and unpins agent quotas per host', () async {
      final db = AleraDatabase(executor: NativeDatabase.memory());
      addTearDown(db.close);
      final repository = DriftSettingsRepository(db);
      final container = ProviderContainer(
        overrides: [settingsRepositoryProvider.overrideWithValue(repository)],
      );
      addTearDown(container.dispose);
      final controller = container.read(settingsControllerProvider.notifier);
      await controller.load();

      await controller.setAgentQuotaPinned(
        hostId: 'local',
        pinKey: 'codex',
        pinned: false,
      );
      await controller.setAgentQuotaPinned(
        hostId: 'local',
        pinKey: 'codex',
        pinned: false,
      );
      await controller.setAgentQuotaPinned(
        hostId: 'local',
        pinKey: 'claude:leynierdev',
        pinned: false,
      );

      final unpinned = controller.state.agents.quotas
          .forHost('local')
          .unpinnedQuotaKeys;
      expect(unpinned, unorderedEquals(<String>['codex', 'claude:leynierdev']));
      expect(
        (await repository.load()).agents.quotas
            .forHost('local')
            .unpinnedQuotaKeys,
        unorderedEquals(<String>['codex', 'claude:leynierdev']),
      );
      expect(
        controller.state.agents.quotas.forHost('remote').unpinnedQuotaKeys,
        isEmpty,
      );

      await controller.setAgentQuotaPinned(
        hostId: 'local',
        pinKey: 'codex',
        pinned: true,
      );
      expect(
        controller.state.agents.quotas.forHost('local').unpinnedQuotaKeys,
        <String>['claude:leynierdev'],
      );
    });

    test('prunes orphaned claude pin keys when profiles change', () async {
      final db = AleraDatabase(executor: NativeDatabase.memory());
      addTearDown(db.close);
      final repository = DriftSettingsRepository(db);
      final container = ProviderContainer(
        overrides: [settingsRepositoryProvider.overrideWithValue(repository)],
      );
      addTearDown(container.dispose);
      final controller = container.read(settingsControllerProvider.notifier);
      await controller.load();

      await controller.setAgentQuotaPinned(
        hostId: 'local',
        pinKey: 'claude:old',
        pinned: false,
      );
      await controller.setAgentQuotaPinned(
        hostId: 'local',
        pinKey: 'claude:default',
        pinned: false,
      );
      await controller.setAgentQuotaPinned(
        hostId: 'local',
        pinKey: 'kimi',
        pinned: false,
      );
      await controller.setClaudeQuotaProfiles(
        hostId: 'local',
        profiles: const <ClaudeQuotaProfileSettings>[
          ClaudeQuotaProfileSettings(alias: 'ccdev', profile: 'dev'),
        ],
      );

      expect(
        controller.state.agents.quotas.forHost('local').unpinnedQuotaKeys,
        unorderedEquals(<String>['claude:default', 'kimi']),
      );
    });

    test('persists and resets editor settings', () async {
      final db = AleraDatabase(executor: NativeDatabase.memory());
      addTearDown(db.close);
      final repository = DriftSettingsRepository(db);
      final container = ProviderContainer(
        overrides: [settingsRepositoryProvider.overrideWithValue(repository)],
      );
      addTearDown(container.dispose);
      final controller = container.read(settingsControllerProvider.notifier);
      await controller.load();

      await controller.updateEditor(
        (editor) => editor.copyWith(
          tabSize: 2,
          themeName: EditorSyntaxThemeNames.monokai,
          quickOpenExcludedDirectories: <String>['generated'],
        ),
      );
      var restored = await repository.load();
      expect(restored.editor.tabSize, 2);
      expect(restored.editor.themeName, EditorSyntaxThemeNames.monokai);
      expect(restored.editor.quickOpenExcludedDirectories, <String>[
        'generated',
      ]);

      await controller.resetEditorSettings();
      restored = await repository.load();
      expect(restored.editor.tabSize, EditorSettings.defaults.tabSize);
      expect(restored.editor.themeName, EditorSettings.defaults.themeName);
      expect(
        restored.editor.quickOpenExcludedDirectories,
        EditorSettings.defaultQuickOpenExcludedDirectories,
      );
    });

    test('persists keyboard binding changes and reset', () async {
      final db = AleraDatabase(executor: NativeDatabase.memory());
      addTearDown(db.close);
      final repository = DriftSettingsRepository(db);
      final container = ProviderContainer(
        overrides: [settingsRepositoryProvider.overrideWithValue(repository)],
      );
      addTearDown(container.dispose);
      final controller = container.read(settingsControllerProvider.notifier);
      await controller.load();

      await controller.setActionBindings(.newTerminalTab, <String>[
        'Mod+Shift+T',
      ]);
      await controller.setTerminalShortcutPolicy(.terminalFirst);

      var restored = await repository.load();
      expect(
        restored.keyboard.overrides[KeyboardActionId.newTerminalTab],
        <String>['Mod+Shift+T'],
      );
      expect(
        restored.keyboard.terminalPolicy,
        TerminalShortcutPolicy.terminalFirst,
      );

      await controller.resetKeyboardShortcuts();
      restored = await repository.load();
      expect(restored.keyboard.overrides, isEmpty);
      // Reset preserves the chosen policy.
      expect(
        restored.keyboard.terminalPolicy,
        TerminalShortcutPolicy.terminalFirst,
      );
    });

    test(
      'persists destructive confirmation and agent status preferences',
      () async {
        final db = AleraDatabase(executor: NativeDatabase.memory());
        addTearDown(db.close);
        final repository = DriftSettingsRepository(db);
        final container = ProviderContainer(
          overrides: [settingsRepositoryProvider.overrideWithValue(repository)],
        );
        addTearDown(container.dispose);
        final controller = container.read(settingsControllerProvider.notifier);
        await controller.load();

        await controller.setConfirmProjectRemoval(false);
        await controller.setConfirmWorkspaceRemoval(false);
        await controller.setAgentStatusHookEnabled(.codex, true);
        await controller.setAgentStatusHookEnabled(.cursor, true);
        await controller.setAgentStatusHookEnabled(.agy, true);
        await controller.setAgentStatusHookEnabled(.opencode, true);
        await controller.setAgentStatusHookEnabled(.pi, true);
        await controller.setAgentStatusHookEnabled(.amp, true);
        await controller.setAgentStatusHookEnabled(.grok, true);
        await controller.setAgentStatusHookEnabled(.fx, true);
        await controller.setAgentStatusNotificationsEnabled(true);
        await controller.setKeepComputerAwakeWhileAgentsWork(true);
        await controller.setShowTabTitlesInSidebar(true);
        await controller.setKeepAliveEnabled(true);
        await controller.setShowTrayIcon(false);
        await controller.setShowDockBadge(false);
        await controller.setShowTrayBadge(false);
        await controller.setShowPullRequestStatusInSidebar(false);
        await controller.setPullRequestFailureNotificationsEnabled(true);

        final restored = await repository.load();
        expect(
          container
              .read(settingsControllerProvider)
              .general
              .confirmProjectRemoval,
          isFalse,
        );
        expect(
          container
              .read(settingsControllerProvider)
              .general
              .confirmWorkspaceRemoval,
          isFalse,
        );
        expect(restored.general.confirmProjectRemoval, isFalse);
        expect(restored.general.confirmWorkspaceRemoval, isFalse);
        expect(restored.agents.agentStatusHooks.isEnabled('codex'), isTrue);
        expect(restored.agents.agentStatusHooks.isEnabled('claude'), isFalse);
        expect(restored.agents.agentStatusHooks.isEnabled('copilot'), isFalse);
        expect(restored.agents.agentStatusHooks.isEnabled('cursor'), isTrue);
        expect(restored.agents.agentStatusHooks.isEnabled('agy'), isTrue);
        expect(restored.agents.agentStatusHooks.isEnabled('opencode'), isTrue);
        expect(restored.agents.agentStatusHooks.isEnabled('pi'), isTrue);
        expect(restored.agents.agentStatusHooks.isEnabled('amp'), isTrue);
        expect(restored.agents.agentStatusHooks.isEnabled('grok'), isTrue);
        expect(restored.agents.agentStatusHooks.isEnabled('fx'), isTrue);
        expect(restored.agents.agentStatusNotificationsEnabled, isTrue);
        expect(restored.agents.keepComputerAwakeWhileAgentsWork, isTrue);
        expect(restored.agents.showTabTitlesInSidebar, isTrue);
        expect(restored.general.keepAliveEnabled, isTrue);
        expect(restored.general.showTrayIcon, isFalse);
        expect(restored.general.showDockBadge, isFalse);
        expect(restored.general.showTrayBadge, isFalse);
        expect(restored.general.showPullRequestStatusInSidebar, isFalse);
        expect(restored.general.pullRequestFailureNotificationsEnabled, isTrue);
      },
    );

    test('persists the default agent profile', () async {
      final db = AleraDatabase(executor: NativeDatabase.memory());
      addTearDown(db.close);
      final repository = DriftSettingsRepository(db);
      final container = ProviderContainer(
        overrides: [settingsRepositoryProvider.overrideWithValue(repository)],
      );
      addTearDown(container.dispose);
      final controller = container.read(settingsControllerProvider.notifier);
      await controller.load();

      await controller.setDefaultAgentProfile(' profile-1 ');

      expect(
        container.read(settingsControllerProvider).agents.defaultAgentProfileId,
        'profile-1',
      );
      expect(
        (await repository.load()).agents.defaultAgentProfileId,
        'profile-1',
      );

      await controller.setDefaultAgentProfile('   ');
      expect((await repository.load()).agents.defaultAgentProfileId, isNull);
    });

    test('applyBindingChanges reassigns a chord atomically', () async {
      final db = AleraDatabase(executor: NativeDatabase.memory());
      addTearDown(db.close);
      final repository = DriftSettingsRepository(db);
      final container = ProviderContainer(
        overrides: [settingsRepositoryProvider.overrideWithValue(repository)],
      );
      addTearDown(container.dispose);
      final controller = container.read(settingsControllerProvider.notifier);
      await controller.load();

      await controller.applyBindingChanges(<KeyboardActionId, List<String>?>{
        KeyboardActionId.closeTab: <String>['Mod+T'],
        KeyboardActionId.newTerminalTab: <String>[],
      });

      final restored = await repository.load();
      expect(restored.keyboard.overrides[KeyboardActionId.closeTab], <String>[
        'Mod+T',
      ]);
      expect(restored.keyboard.isDisabled(.newTerminalTab), isTrue);
    });

    test(
      'markStarClicked persists once and becomes a no-op afterward',
      () async {
        final db = AleraDatabase(executor: NativeDatabase.memory());
        addTearDown(db.close);
        final repository = DriftSettingsRepository(db);
        final container = ProviderContainer(
          overrides: [settingsRepositoryProvider.overrideWithValue(repository)],
        );
        addTearDown(container.dispose);
        final controller = container.read(settingsControllerProvider.notifier);
        await controller.load();

        await controller.markStarClicked();
        final firstRestore = await repository.load();
        expect(firstRestore.general.starClicked, isTrue);

        await controller.markStarClicked();
        final secondRestore = await repository.load();
        expect(secondRestore.general.starClicked, isTrue);
      },
    );
  });
}

final class _ConflictSettingsRepository implements SettingsRepository {
  AleraSettings current = .defaults;
  bool conflictOnSave = false;

  @override
  Future<AleraSettings> load() async => current;

  @override
  Future<void> save(AleraSettings settings) async {
    if (conflictOnSave) {
      throw const TerminalHostConflictException(
        code: 'runtime_settings_revision_conflict',
        message: 'Runtime settings revision conflict.',
      );
    }
    current = settings;
  }
}
