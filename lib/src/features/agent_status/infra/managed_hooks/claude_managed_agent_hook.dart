// coverage:ignore-file
// Claude user-config descriptors are intentionally inactive: Claude hooks are
// installed only in Alera-managed runtime homes.
part of '../managed_agent_hook_installer.dart';

final _claudeManagedAgentHookAdapter = _ManagedAgentHookAdapter(
  runtimeOnlyStatus: (service) => service._claudeRuntimeOnlyStatus(),
  jsonDescriptor: (service, scriptFileName, scriptPath) =>
      service._claudeDescriptor(
        scriptFileName: scriptFileName,
        scriptPath: scriptPath,
      ),
);

extension _ClaudeManagedAgentHook on ManagedAgentHookInstallService {
  ManagedAgentHookInstallStatus _claudeRuntimeOnlyStatus() {
    return ManagedAgentHookInstallStatus(
      agentType: .claude,
      state: .notInstalled,
      configPath: p.join(_homeDirectory, '.claude', 'settings.json'),
      managedHooksPresent: false,
      detail: 'Claude Code hooks are installed only in Alera-managed runtime homes.',
    );
  }

  _AgentHookDescriptor _claudeDescriptor({
    required String scriptFileName,
    required String scriptPath,
  }) {
    return _AgentHookDescriptor(
      agentType: .claude,
      configPath: p.join(_homeDirectory, '.claude', 'settings.json'),
      configLabel: 'Claude settings.json',
      scriptFileName: scriptFileName,
      scriptPath: scriptPath,
      eventEnvVar: 'ALERA_AGENT_HOOK_EVENT',
      configShape: .hooks,
      definitionShape: .nestedCommand,
      events: const <_ManagedHookEvent>[
        _ManagedHookEvent('UserPromptSubmit'),
        _ManagedHookEvent('Stop'),
        _ManagedHookEvent('StopFailure'),
        _ManagedHookEvent('PreToolUse', matcher: '*'),
        _ManagedHookEvent('PostToolUse', matcher: '*'),
        _ManagedHookEvent('PostToolUseFailure', matcher: '*'),
        _ManagedHookEvent('PermissionRequest', matcher: '*'),
      ],
    );
  }
}
