part of '../managed_agent_hook_installer.dart';

String? _devinPortableConfigPath(Map<String, String> environment) {
  final explicitRoot = environment['DEVIN_PORTABLE_ROOT']?.trim();
  if (explicitRoot != null && explicitRoot.isNotEmpty) {
    final config = _devinPortableConfigAtRoot(explicitRoot);
    if (config != null) {
      return config;
    }
  }
  final extensions = (environment['PATHEXT'] ?? '.COM;.EXE;.BAT;.CMD')
      .split(';')
      .where((extension) => extension.isNotEmpty);
  for (final directory in (environment['PATH'] ?? '').split(';')) {
    final path = directory.trim().replaceAll(RegExp(r'^"|"$'), '');
    if (path.isEmpty) {
      continue;
    }
    for (final extension in extensions) {
      final launcher = File(p.join(path, 'devin${extension.toLowerCase()}'));
      if (!launcher.existsSync()) {
        continue;
      }
      return _devinPortableConfigFromLauncher(launcher);
    }
  }
  return null;
}

String? _devinPortableConfigAtRoot(String root) {
  if (!File(p.join(root, 'bin', '_devin-env.cmd')).existsSync() ||
      !File(p.join(root, 'bin', 'devin.exe')).existsSync()) {
    return null;
  }
  return p.join(
    root,
    'data',
    'home',
    'AppData',
    'Roaming',
    'devin',
    'config.json',
  );
}

String? _devinPortableConfigFromLauncher(File launcher) {
  if (!const [
    '.cmd',
    '.bat',
  ].contains(p.extension(launcher.path).toLowerCase())) {
    return null;
  }
  try {
    if (launcher.lengthSync() > 65536) {
      return null;
    }
    var source = launcher.readAsStringSync();
    final shimTarget = RegExp(
      r'^@rem[ \t]+([^\r\n]+)',
      multiLine: true,
      caseSensitive: false,
    ).firstMatch(source)?.group(1)?.trim();
    if (shimTarget != null) {
      launcher = File(shimTarget);
      if (!launcher.existsSync() || launcher.lengthSync() > 65536) {
        return null;
      }
      source = launcher.readAsStringSync();
    }
    if (!source.toLowerCase().contains('_devin-env.cmd')) {
      return null;
    }
    return _devinPortableConfigAtRoot(p.dirname(p.dirname(launcher.path)));
  } on FileSystemException {
    return null;
  }
}

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
      configPath:
          (_platform == ManagedAgentHookPlatform.windows
              ? _devinPortableConfigPath(_environment)
              : null) ??
          p.join(configRoot, 'devin', 'config.json'),
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
