part of '../managed_agent_hook_installer.dart';

final _cursorManagedAgentHookAdapter = _ManagedAgentHookAdapter(
  runtimeOnlyStatus: (service) => service._cursorRuntimeOnlyStatus(),
);

extension _CursorManagedAgentHook on ManagedAgentHookInstallService {
  ManagedAgentHookInstallStatus _cursorRuntimeOnlyStatus() {
    return ManagedAgentHookInstallStatus(
      agentType: .cursor,
      state: .notInstalled,
      configPath: p.join(_homeDirectory, '.cursor', 'hooks.json'),
      managedHooksPresent: false,
      detail: 'Cursor hooks are installed as a per-session plugin, never in this file.',
    );
  }
}
