part of '../managed_agent_hook_installer.dart';

final _cursorManagedAgentHookAdapter = _ManagedAgentHookAdapter(
  jsonDescriptor: (service, scriptFileName, scriptPath) =>
      service._cursorDescriptor(
        scriptFileName: scriptFileName,
        scriptPath: scriptPath,
      ),
);

extension _CursorManagedAgentHook on ManagedAgentHookInstallService {
  _AgentHookDescriptor _cursorDescriptor({
    required String scriptFileName,
    required String scriptPath,
  }) {
    return _AgentHookDescriptor(
      agentType: .cursor,
      configPath: p.join(_homeDirectory, '.cursor', 'hooks.json'),
      configLabel: 'Cursor hooks.json',
      scriptFileName: scriptFileName,
      scriptPath: scriptPath,
      eventEnvVar: 'ALERA_CURSOR_EVENT',
      managedScriptFileNames: <String>[
        scriptFileName,
        'alera-cursor-hook.sh',
        'alera-cursor-hook.cmd',
        'alera-runtime-agent-hook.sh',
        'alera-runtime-agent-hook.cmd',
      ],
      configShape: .hooks,
      definitionShape: .topLevelCommand,
      rootConfigValuesOnInstall: const <String, Object?>{'version': 1},
      events: const <_ManagedHookEvent>[
        _ManagedHookEvent('beforeSubmitPrompt'),
        _ManagedHookEvent('stop'),
        _ManagedHookEvent('sessionEnd'),
        _ManagedHookEvent('preToolUse'),
        _ManagedHookEvent('postToolUse'),
        _ManagedHookEvent('postToolUseFailure'),
        _ManagedHookEvent('beforeShellExecution'),
        _ManagedHookEvent('afterShellExecution'),
        _ManagedHookEvent('beforeMCPExecution'),
        _ManagedHookEvent('afterMCPExecution'),
        _ManagedHookEvent('afterAgentResponse'),
      ],
    );
  }
}
