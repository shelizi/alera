import 'package:alera/src/features/agent_profiles/domain/agent_descriptor_registry.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/agent_status/infra/claude_runtime_home_service.dart';
import 'package:alera/src/features/agent_status/infra/codex_runtime_home_service.dart';
import 'package:alera/src/features/agent_status/infra/managed_agent_hook_installer.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';

abstract interface class AgentHookReconciler {
  Future<List<ManagedAgentHookInstallStatus>> reconcile(
    AgentStatusHookSettings settings,
  );
}

class const AgentHookReconciliationService({
  required final ManagedAgentHookInstallService managedHooks,
  required final CodexRuntimeHomeService codexRuntimeHome,
  required final ClaudeRuntimeHomeService claudeRuntimeHome,
  final bool hostOwnsCodexHooks = false,
}) implements AgentHookReconciler {
  @override
  Future<List<ManagedAgentHookInstallStatus>> reconcile(
    AgentStatusHookSettings settings,
  ) async {
    final results = await managedHooks.reconcile(
      enabledAgentTypes: _enabledGlobalManagedAgentStatusHookTypes(settings),
      agentTypes: agentTypesWithGlobalManagedHooks,
    );
    if (hostOwnsCodexHooks) {
      // The Host reconciles the effective CODEX_HOME on startup, settings
      // changes and terminal launch. Classic must not replace its command or
      // trust indices with the legacy per-provider script. The same native
      // config can also be used by a different Host profile.
      if (isAgentSettingEnabled(settings.values, .codex)) {
        results.add(await codexRuntimeHome.status(hostManaged: true));
      }
    } else {
      results.add(
        isAgentSettingEnabled(settings.values, .codex)
            ? await codexRuntimeHome.install()
            : await codexRuntimeHome.remove(),
      );
    }
    results.add(
      isAgentSettingEnabled(settings.values, .claude)
          ? await claudeRuntimeHome.install()
          : await claudeRuntimeHome.remove(),
    );
    results.add(managedHooks.remove(.opencode));
    results.add(managedHooks.remove(.opencode2));
    results.add(managedHooks.remove(.pi));
    return results;
  }
}

List<AgentType> enabledAgentStatusHookTypes(AgentStatusHookSettings settings) {
  return agentTypesWithStatusHooks
      .where((agentType) => isAgentSettingEnabled(settings.values, agentType))
      .toList(growable: false);
}

List<AgentType> _enabledGlobalManagedAgentStatusHookTypes(
  AgentStatusHookSettings settings,
) {
  return agentTypesWithGlobalManagedHooks
      .where((agentType) => isAgentSettingEnabled(settings.values, agentType))
      .toList(growable: false);
}
