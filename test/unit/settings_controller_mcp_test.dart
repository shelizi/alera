import 'package:alera/src/app/dependencies.dart';
import 'package:alera/src/features/settings/application/settings_controller.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:alera/src/features/settings/infra/drift_settings_repository.dart';
import 'package:alera/src/shared/infra/storage/drift_database.dart';
import 'package:drift/native.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('MCP settings only project reviewed safe scalar values', () {
    final settings = AleraSettings.defaults;
    expect(mcpSettingValue(settings, 'general.autoGitFetchEnabled'), true);
    expect(mcpSettingValue(settings, 'general.autoGitFetchIntervalMinutes'), 5);
    expect(mcpSettingValue(settings, 'terminal.fontSize'), 13);
    expect(mcpSettingValue(settings, 'editor.tabSize'), 4);
    expect(mcpWritableSettingValue('terminal.fontSize', 16), isTrue);
    expect(mcpWritableSettingValue('terminal.fontSize', 49), isFalse);
    expect(mcpWritableSettingValue('terminal.fontSize', double.nan), isFalse);
    expect(mcpWritableSettingValue('editor.tabSize', 0), isFalse);
    expect(
      mcpWritableSettingValue('general.confirmProjectRemoval', false),
      isFalse,
    );
    expect(mcpWritableSettingValue('agents.agentStatusHooks', {}), isFalse);
    expect(
      () => mcpSettingValue(settings, 'agents.quotas'),
      throwsArgumentError,
    );
  });

  test(
    'MCP fresh read reloads persisted value changed by another window',
    () async {
      final db = AleraDatabase(executor: NativeDatabase.memory());
      addTearDown(db.close);
      final repository = DriftSettingsRepository(db);
      final first = ProviderContainer(
        overrides: [settingsRepositoryProvider.overrideWithValue(repository)],
      );
      final second = ProviderContainer(
        overrides: [settingsRepositoryProvider.overrideWithValue(repository)],
      );
      addTearDown(first.dispose);
      addTearDown(second.dispose);

      final controllerA = first.read(settingsControllerProvider.notifier);
      final controllerB = second.read(settingsControllerProvider.notifier);
      await controllerA.load();
      await controllerB.load();
      expect(controllerB.readMcpSetting('editor.tabSize'), 4);

      expect(
        await controllerA.updateMcpSetting(
          path: 'editor.tabSize',
          expectedValue: 4,
          value: 5,
        ),
        5,
      );
      // Independent controller B has no in-memory event from A: this is
      // exactly the two-process E2E race. Reading its cached state is stale.
      expect(controllerB.readMcpSetting('editor.tabSize'), 4);
      expect(await controllerB.readMcpSettingFresh('editor.tabSize'), 5);
      await expectLater(
        controllerB.readMcpSettingFresh('agents.quotas'),
        throwsArgumentError,
      );
      // Repeated reads remain authoritative and the value stays persisted.
      expect(await controllerB.readMcpSettingFresh('editor.tabSize'), 5);
      expect((await repository.load()).editor.tabSize, 5);
    },
  );

  test('MCP settings CAS persists in real settings owner and denies stale overwrite', () async {
    final db = AleraDatabase(executor: NativeDatabase.memory());
    addTearDown(db.close);
    final repository = DriftSettingsRepository(db);
    final container = ProviderContainer(
      overrides: [settingsRepositoryProvider.overrideWithValue(repository)],
    );
    addTearDown(container.dispose);
    final controller = container.read(settingsControllerProvider.notifier);
    await controller.load();

    expect(
      await controller.updateMcpSetting(
        path: 'terminal.fontSize',
        expectedValue: 13,
        value: 16,
      ),
      16,
    );
    expect((await repository.load()).terminal.fontSize, 16);
    await expectLater(
      controller.updateMcpSetting(
        path: 'terminal.fontSize',
        expectedValue: 13,
        value: 18,
      ),
      throwsA(isA<StateError>()),
    );
    expect((await repository.load()).terminal.fontSize, 16);

    expect(
      await controller.updateMcpSetting(
        path: 'general.autoGitFetchEnabled',
        expectedValue: true,
        value: false,
      ),
      false,
    );
    expect((await repository.load()).general.autoGitFetchEnabled, false);
    expect(
      await controller.updateMcpSetting(
        path: 'general.autoGitFetchIntervalMinutes',
        expectedValue: 5,
        value: 12,
      ),
      12,
    );
    expect((await repository.load()).general.autoGitFetchIntervalMinutes, 12);
    expect(
      await controller.updateMcpSetting(
        path: 'editor.tabSize',
        expectedValue: 4,
        value: 2,
      ),
      2,
    );
    expect((await repository.load()).editor.tabSize, 2);
    await expectLater(
      controller.updateMcpSetting(
        path: 'general.confirmWorkspaceRemoval',
        expectedValue: true,
        value: false,
      ),
      throwsArgumentError,
    );
    await expectLater(
      controller.updateMcpSetting(
        path: 'terminal.fontSize',
        expectedValue: 16,
        value: 100,
      ),
      throwsArgumentError,
    );
    expect((await repository.load()).terminal.fontSize, 16);
  });
}
