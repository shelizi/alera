part of '../managed_agent_hook_installer.dart';

final _devinManagedAgentHookAdapter = _ManagedAgentHookAdapter(
  jsonDescriptor: (service, scriptFileName, scriptPath) => service
      ._devinDescriptor(scriptFileName: scriptFileName, scriptPath: scriptPath),
);

extension _DevinManagedAgentHook on ManagedAgentHookInstallService {
  _AgentHookDescriptor _devinDescriptor({
    required String scriptFileName,
    required String scriptPath,
  }) {
    final configRoot = switch (_platform) {
      ManagedAgentHookPlatform.windows =>
        _environment['APPDATA']?.trim().isNotEmpty == true
            ? _environment['APPDATA']!.trim()
            : p.join(_homeDirectory, 'AppData', 'Roaming'),
      ManagedAgentHookPlatform.posix => p.join(_homeDirectory, '.config'),
    };
    return _AgentHookDescriptor(
      agentType: .devin,
      configPath: p.join(configRoot, 'devin', 'config.json'),
      configLabel: 'Devin config.json',
      scriptFileName: scriptFileName,
      scriptPath: scriptPath,
      eventEnvVar: 'ALERA_DEVIN_EVENT',
      windowsExecutionStrategy: .gitBashToCmd,
      // The runtime sidecar installs Devin hooks against its shared
      // alera-runtime-agent-hook script; both script families count as managed
      // so the two installers cannot leave duplicate handlers behind.
      managedScriptFileNames: const <String>[
        'alera-devin-hook',
        'alera-runtime-agent-hook',
      ],
      configShape: .hooks,
      definitionShape: .nestedCommand,
      // Devin matchers are regexes. Omitting matcher is the documented way to
      // match every tool and avoids the invalid wildcard regex "*".
      events: const <_ManagedHookEvent>[
        _ManagedHookEvent('SessionStart'),
        _ManagedHookEvent('UserPromptSubmit'),
        _ManagedHookEvent('PreToolUse'),
        _ManagedHookEvent('PostToolUse'),
        _ManagedHookEvent('PermissionRequest'),
        _ManagedHookEvent('Stop'),
        _ManagedHookEvent('SessionEnd'),
      ],
    );
  }
}
