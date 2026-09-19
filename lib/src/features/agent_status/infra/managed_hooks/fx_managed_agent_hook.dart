part of '../managed_agent_hook_installer.dart';

final _fxManagedAgentHookAdapter = _ManagedAgentHookAdapter(
  runtimeOnlyStatus: (service) => service._fxRuntimeOnlyStatus(),
);

extension _FxManagedAgentHook on ManagedAgentHookInstallService {
  ManagedAgentHookInstallStatus _fxRuntimeOnlyStatus() {
    return ManagedAgentHookInstallStatus(
      agentType: .fx,
      state: .notInstalled,
      configPath: p.join(_homeDirectory, '.fx'),
      managedHooksPresent: false,
      detail: 'fx reports status through its built-in local Herdr integration, so no user hooks are installed.',
    );
  }
}
