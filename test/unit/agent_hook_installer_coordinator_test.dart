import 'package:alera/src/app/providers.dart';
import 'package:alera/src/features/agent_status/application/agent_hook_reconciliation_service.dart';
import 'package:alera/src/features/agent_status/application/agent_status_providers.dart';
import 'package:alera/src/features/agent_status/infra/managed_agent_hook_installer.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('reconciles already-enabled hooks when coordinator starts', () async {
    final settings = _TestSettingsController(
      AleraSettings.defaults.copyWith(
        agents: AleraSettings.defaults.agents.copyWith(
          agentStatusHooks: const AgentStatusHookSettings(
            values: {'agy': true},
          ),
        ),
      ),
    );
    final reconciler = _RecordingHookReconciler();
    final container = ProviderContainer(
      overrides: [
        settingsControllerProvider.overrideWith(() => settings),
        agentHookReconciliationServiceProvider.overrideWithValue(reconciler),
      ],
    );
    addTearDown(container.dispose);

    container.read(agentHookInstallerCoordinatorProvider);
    await Future<void>.delayed(Duration.zero);

    expect(reconciler.settings, hasLength(1));
    expect(reconciler.settings.single.agy, isTrue);
  });
}

class _TestSettingsController(final AleraSettings _seed)
    extends SettingsController {
  @override
  AleraSettings build() => _seed;
}

class _RecordingHookReconciler implements AgentHookReconciler {
  final List<AgentStatusHookSettings> settings = <AgentStatusHookSettings>[];

  @override
  Future<List<ManagedAgentHookInstallStatus>> reconcile(
    AgentStatusHookSettings settings,
  ) async {
    this.settings.add(settings);
    return const <ManagedAgentHookInstallStatus>[];
  }
}
