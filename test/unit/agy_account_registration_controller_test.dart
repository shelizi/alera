import 'package:alera/src/features/settings/application/runtime_settings_changes.dart';
import 'package:alera/src/features/settings/application/settings_controller.dart';
import 'package:alera/src/features/settings/application/settings_providers.dart';
import 'package:alera/src/features/settings/application/settings_repository.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

class _Repository implements SettingsRepository {
  AleraSettings settings = AleraSettings.defaults;

  @override
  Future<AleraSettings> load() async => settings;

  @override
  Future<void> save(AleraSettings value) async {
    settings = value;
  }
}

void main() {
  test(
    'Google registration edits preserve others and clean stale history groups',
    () async {
      final repository = _Repository();
      const initial = AgentQuotaHostSettings(
        agyProfiles: [AgyQuotaProfileSettings(alias: 'Old', accountId: 'old')],
        historySync: AgentHistorySyncSettings(
          mode: AgentHistorySyncMode.groups,
          accountGroups: {
            'agy:old': 'team',
            'agy:default': 'team',
            'codex:default': 'other',
          },
        ),
      );
      repository.settings = repository.settings.copyWith(
        agents: repository.settings.agents.copyWith(
          quotas: repository.settings.agents.quotas.withHost('remote', initial),
        ),
      );
      final container = ProviderContainer(
        overrides: [
          settingsRepositoryProvider.overrideWithValue(repository),
          runtimeSettingsChangesProvider.overrideWith(
            (ref) => const Stream<int>.empty(),
          ),
        ],
      );
      addTearDown(container.dispose);
      final controller = container.read(settingsControllerProvider.notifier);
      await controller.load();
      await controller.setAgyQuotaProfiles(
        hostId: 'remote',
        profiles: const [
          AgyQuotaProfileSettings(alias: ' Research ', accountId: 'research'),
        ],
      );
      await controller.setSelectedAgyAccount(
        hostId: 'remote',
        accountId: 'research',
      );
      final saved = repository.settings.agents.quotas.forHost('remote');
      expect(saved.agyProfiles.single.alias, 'Research');
      expect(saved.agyProfiles.single.accountId, 'research');
      expect(saved.selectedAgyAccountId, 'research');
      expect(saved.historySync.accountGroups.containsKey('agy:old'), isFalse);
      expect(saved.historySync.accountGroups['agy:default'], 'team');
      expect(saved.historySync.accountGroups['codex:default'], 'other');
      expect(saved.historySync.mode, AgentHistorySyncMode.groups);
      await expectLater(
        controller.setAgyQuotaProfiles(
          hostId: 'remote',
          profiles: const [
            AgyQuotaProfileSettings(alias: 'Dup', accountId: 'one'),
            AgyQuotaProfileSettings(alias: 'Dup', accountId: 'two'),
          ],
        ),
        throwsArgumentError,
      );
      expect(
        repository.settings.agents.quotas
            .forHost('remote')
            .agyProfiles
            .single
            .accountId,
        'research',
      );
      await controller.setAgyQuotaProfiles(
        hostId: 'remote',
        profiles: const [],
      );
      expect(
        repository.settings.agents.quotas
            .forHost('remote')
            .selectedAgyAccountId,
        'default',
      );
    },
  );
}
