import 'package:alera/src/app/theme/alera_dark_theme.dart';
import 'package:alera/src/features/agent_quota/presentation/agent_quota_account_switch.dart';
import 'package:alera/src/features/settings/application/settings_controller.dart';
import 'package:alera/src/features/settings/application/settings_providers.dart';
import 'package:alera/src/features/settings/application/settings_repository.dart';
import 'package:alera/src/features/settings/application/runtime_settings_changes.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

class _Repository implements SettingsRepository {
  AleraSettings settings = AleraSettings.defaults;
  bool fail = false;
  @override
  Future<AleraSettings> load() async => settings;
  @override
  Future<void> save(AleraSettings value) async {
    if (fail) throw StateError('save failed');
    settings = value;
  }
}

void main() {
  for (final provider in [
    AgentQuotaProviderId.codex,
    AgentQuotaProviderId.claude,
  ]) {
    testWidgets(
      'quota selector persists $provider account without restarting a terminal',
      (tester) async {
        const host = AgentQuotaHostSettings(
          codexProfiles: [
            CodexQuotaProfileSettings(alias: 'Work', profile: '/codex-work'),
          ],
          claudeProfiles: [
            ClaudeQuotaProfileSettings(alias: 'Work', profile: 'work'),
          ],
        );
        final repository = _Repository();
        repository.settings = repository.settings.copyWith(
          agents: repository.settings.agents.copyWith(
            quotas: repository.settings.agents.quotas.withHost('remote', host),
          ),
        );
        final container = ProviderContainer(
          overrides: [
            settingsRepositoryProvider.overrideWithValue(repository),
            runtimeSettingsChangesProvider.overrideWith(
              (ref) => const Stream<void>.empty(),
            ),
          ],
        );
        addTearDown(container.dispose);
        await container.read(settingsControllerProvider.notifier).load();
        await tester.pumpWidget(
          UncontrolledProviderScope(
            container: container,
            child: MaterialApp(
              theme: buildAleraDarkTheme(),
              home: Scaffold(
                body: AgentQuotaAccountSwitch(
                  hostId: 'remote',
                  provider: provider,
                ),
              ),
            ),
          ),
        );
        await tester.tap(find.text('Default'));
        await tester.pumpAndSettle();
        await tester.tap(find.text('Work').last);
        await tester.pumpAndSettle();
        final selected = repository.settings.agents.quotas.forHost('remote');
        expect(
          provider == AgentQuotaProviderId.codex
              ? selected.selectedCodexProfile
              : selected.selectedClaudeProfile,
          provider == AgentQuotaProviderId.codex ? '/codex-work' : 'work',
        );
        expect(
          repository.settings.agents.quotas
              .forHost('local')
              .selectedCodexProfile,
          'default',
        );
        repository.fail = true;
        await tester.tap(find.text('Work').first);
        await tester.pumpAndSettle();
        await tester.tap(find.text('Default').last);
        await tester.pumpAndSettle();
        final current = container
            .read(settingsControllerProvider)
            .agents
            .quotas
            .forHost('remote');
        expect(
          provider == AgentQuotaProviderId.codex
              ? current.selectedCodexProfile
              : current.selectedClaudeProfile,
          provider == AgentQuotaProviderId.codex ? '/codex-work' : 'work',
        );
        expect(tester.takeException(), isNull);
      },
    );
  }
}
