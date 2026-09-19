// coverage:ignore-file
// Codex user-config descriptors are intentionally inactive: Codex hooks are
// installed only in Alera-managed runtime homes.
part of '../managed_agent_hook_installer.dart';

final _codexManagedAgentHookAdapter = _ManagedAgentHookAdapter(
  runtimeOnlyStatus: (service) => service._codexRuntimeOnlyStatus(),
  jsonDescriptor: (service, scriptFileName, scriptPath) => service
      ._codexDescriptor(scriptFileName: scriptFileName, scriptPath: scriptPath),
);

extension _CodexManagedAgentHook on ManagedAgentHookInstallService {
  ManagedAgentHookInstallStatus _codexRuntimeOnlyStatus() {
    return ManagedAgentHookInstallStatus(
      agentType: .codex,
      state: .notInstalled,
      configPath: p.join(_homeDirectory, '.codex', 'hooks.json'),
      managedHooksPresent: false,
      detail: 'Codex hooks are installed only in Alera-managed runtime homes.',
    );
  }

  _AgentHookDescriptor _codexDescriptor({
    required String scriptFileName,
    required String scriptPath,
  }) {
    return _AgentHookDescriptor(
      agentType: .codex,
      configPath: p.join(_homeDirectory, '.codex', 'hooks.json'),
      configLabel: 'Codex hooks.json',
      scriptFileName: scriptFileName,
      scriptPath: scriptPath,
      eventEnvVar: 'ALERA_AGENT_HOOK_EVENT',
      configShape: .hooks,
      definitionShape: .nestedCommand,
      events: const <_ManagedHookEvent>[
        _ManagedHookEvent('SessionStart'),
        _ManagedHookEvent('UserPromptSubmit'),
        _ManagedHookEvent('PreToolUse'),
        _ManagedHookEvent('PostToolUse'),
        _ManagedHookEvent('PermissionRequest'),
        _ManagedHookEvent('Stop'),
        _ManagedHookEvent('Interrupt'),
        _ManagedHookEvent('SessionEnd'),
      ],
    );
  }
}
