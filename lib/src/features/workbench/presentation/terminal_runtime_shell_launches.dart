part of 'terminal_runtime.dart';

Map<String, String> _terminalPlatformEnvironment() {
  final environment = <String, String>{...ghosttyTerminalPlatformEnvironment()};
  environment.remove('NO_COLOR');
  return environment;
}

List<GhosttyTerminalShellLaunch> _terminalShellLaunches({
  String? powerShell7ExecutablePath,
}) {
  final platformEnvironment = _terminalPlatformEnvironment();
  final ghosttyLaunches = <GhosttyTerminalShellLaunch>[
    ...ghosttyTerminalShellLaunches(
      profile: .userShell,
      platformEnvironment: platformEnvironment,
    ),
    ..._resolvedLoginShellFallbackLaunches(platformEnvironment),
    ...ghosttyTerminalShellLaunches(
      profile: .cleanZsh,
      platformEnvironment: platformEnvironment,
    ),
    ...ghosttyTerminalShellLaunches(
      profile: .auto,
      platformEnvironment: platformEnvironment,
    ),
  ];
  final launches = <GhosttyTerminalShellLaunch>[
    if (_isWindowsDesktopTerminalTarget)
      ..._windowsTerminalShellLaunches(
        platformEnvironment,
        powerShell7ExecutablePath: powerShell7ExecutablePath,
      ),
    ...ghosttyLaunches,
    if (!_isWindowsDesktopTerminalTarget && ghosttyLaunches.isEmpty)
      ..._posixDesktopFallbackShellLaunches(platformEnvironment),
  ];
  final seen = <String>{};
  return launches
      .where((launch) {
        final key = '${launch.shell}\u0000${launch.arguments.join('\u0000')}';
        return seen.add(key);
      })
      .toList(growable: false);
}

List<GhosttyTerminalShellLaunch> _posixDesktopFallbackShellLaunches(
  Map<String, String> platformEnvironment,
) {
  return switch (defaultTargetPlatform) {
    TargetPlatform.linux ||
    TargetPlatform.macOS => <GhosttyTerminalShellLaunch>[
      GhosttyTerminalShellLaunch(
        label: 'sh',
        shell: '/bin/sh',
        arguments: const <String>['-i'],
        environment: ghosttyTerminalShellEnvironment(
          platformEnvironment: platformEnvironment,
          overrides: const <String, String>{'TERM': 'xterm-256color'},
        ),
      ),
    ],
    _ => const <GhosttyTerminalShellLaunch>[],
  };
}

/// A real login-capable shell to use when `$SHELL` is absent from the app
/// environment (a Finder/Dock launch can start the app without it). Without
/// this, the first surviving candidate is a deliberately rc-skipping profile
/// (`zsh -f`, `bash --noprofile --norc`) that never sources the user's profile,
/// so the terminal loses the aliases, functions, and PATH those files build.
/// The `userShell` profile already covers the case where `$SHELL` is set, so
/// this only fills the gap and is a no-op otherwise.
List<GhosttyTerminalShellLaunch> _resolvedLoginShellFallbackLaunches(
  Map<String, String> platformEnvironment, {
  bool Function(String path) fileExists = _fileExists,
}) {
  if (_isWindowsDesktopTerminalTarget) {
    return const <GhosttyTerminalShellLaunch>[];
  }
  final envShell = platformEnvironment['SHELL'];
  if (envShell != null && envShell.isNotEmpty) {
    return const <GhosttyTerminalShellLaunch>[];
  }
  final shell = _probeDefaultLoginShell(fileExists);
  if (shell == null) {
    return const <GhosttyTerminalShellLaunch>[];
  }
  return <GhosttyTerminalShellLaunch>[
    GhosttyTerminalShellLaunch(
      label: 'login shell',
      shell: shell,
      arguments: const <String>['-i'],
      environment: ghosttyTerminalShellEnvironment(
        platformEnvironment: platformEnvironment,
        overrides: const <String, String>{'TERM': 'xterm-256color'},
      ),
    ),
  ];
}

/// Well-known interactive shells, preferring `zsh` (the modern macOS default and
/// the host's own hydration fallback) before `bash`, then POSIX `sh`.
String? _probeDefaultLoginShell(bool Function(String path) fileExists) {
  const candidates = <String>[
    '/bin/zsh',
    '/usr/bin/zsh',
    '/bin/bash',
    '/usr/bin/bash',
    '/bin/sh',
  ];
  for (final candidate in candidates) {
    if (fileExists(candidate)) {
      return candidate;
    }
  }
  return null;
}

List<GhosttyTerminalShellLaunch> _windowsTerminalShellLaunches(
  Map<String, String> platformEnvironment, {
  String? powerShell7ExecutablePath,
  bool Function(String path) fileExists = _fileExists,
}) {
  final shellEnvironment = ghosttyTerminalShellEnvironment(
    platformEnvironment: platformEnvironment,
    overrides: const <String, String>{'TERM': 'xterm-256color'},
  );
  final launches = <GhosttyTerminalShellLaunch>[];
  final pwsh = _resolveWindowsPowerShell7(
    platformEnvironment,
    configuredPath: powerShell7ExecutablePath,
    fileExists: fileExists,
  );
  if (pwsh != null) {
    launches.add(
      GhosttyTerminalShellLaunch(
        label: 'PowerShell 7',
        shell: pwsh,
        environment: shellEnvironment,
      ),
    );
  }
  final powershell = _resolveWindowsPowerShell(platformEnvironment);
  if (powershell != null) {
    launches.add(
      GhosttyTerminalShellLaunch(
        label: 'Windows PowerShell',
        shell: powershell,
        environment: shellEnvironment,
      ),
    );
  }
  launches.add(
    GhosttyTerminalShellLaunch(
      label: 'cmd.exe',
      shell: _resolveWindowsCommandPrompt(platformEnvironment),
      environment: shellEnvironment,
    ),
  );
  return launches;
}

String? _resolveWindowsPowerShell7(
  Map<String, String> environment, {
  String? configuredPath,
  required bool Function(String path) fileExists,
}) {
  // On Windows the PTY first starts Alera's Job Object bootstrap and only then
  // launches the requested shell. Returning a bare `pwsh.exe` when PowerShell 7
  // is not installed makes the bootstrap itself appear to start successfully,
  // so the caller never gets a chance to fall back to Windows PowerShell/cmd.
  final configured = configuredPath?.trim();
  final path = _windowsEnvironmentValue(environment, 'PATH');
  return _resolveFirstExistingWindowsPath(<String>[
    if (configured != null && configured.isNotEmpty) configured,
    if (_windowsEnvironmentValue(environment, 'ProgramFiles') case final dir?)
      _joinWindowsPath(dir, 'PowerShell', '7', 'pwsh.exe'),
    if (_windowsEnvironmentValue(environment, 'ProgramW6432') case final dir?)
      _joinWindowsPath(dir, 'PowerShell', '7', 'pwsh.exe'),
    if (_windowsEnvironmentValue(environment, 'SCOOP') case final dir?)
      _joinWindowsPath(dir, 'apps', 'pwsh', 'current', 'pwsh.exe'),
    if (_windowsEnvironmentValue(environment, 'USERPROFILE') case final dir?)
      _joinWindowsPath(dir, 'scoop', 'apps', 'pwsh', 'current\\pwsh.exe'),
    if (_windowsEnvironmentValue(environment, 'LOCALAPPDATA')
        case final dir?) ...<String>[
      _joinWindowsPath(dir, 'Microsoft', 'WindowsApps', 'pwsh.exe'),
      _joinWindowsPath(dir, 'Programs', 'PowerShell', '7', 'pwsh.exe'),
    ],
    if (path != null)
      for (final dir in path.split(';'))
        if (dir.trim().isNotEmpty) _joinWindowsPath(dir.trim(), 'pwsh.exe'),
  ], fileExists: fileExists);
}

String? _resolveWindowsPowerShell(Map<String, String> environment) {
  if (_windowsSystemRoot(environment) case final root?) {
    return _joinWindowsPath(
      root,
      'System32',
      'WindowsPowerShell',
      'v1.0',
      'powershell.exe',
    );
  }
  return 'powershell.exe';
}

String _resolveWindowsCommandPrompt(Map<String, String> environment) {
  final comspec = _windowsEnvironmentValue(environment, 'ComSpec');
  if (comspec != null && comspec.isNotEmpty) {
    return comspec;
  }
  if (_windowsSystemRoot(environment) case final root?) {
    return _joinWindowsPath(root, 'System32', 'cmd.exe');
  }
  return 'cmd.exe';
}

String? _resolveFirstExistingWindowsPath(
  Iterable<String> candidates, {
  required bool Function(String path) fileExists,
}) {
  for (final candidate in _uniqueWindowsPaths(candidates)) {
    if (candidate.isNotEmpty && fileExists(candidate)) {
      return candidate;
    }
  }
  return null;
}

String? _windowsSystemRoot(Map<String, String> environment) {
  return _windowsEnvironmentValue(environment, 'SystemRoot') ??
      _windowsEnvironmentValue(environment, 'windir');
}

String? _windowsEnvironmentValue(Map<String, String> environment, String key) {
  final exact = environment[key];
  if (exact != null && exact.isNotEmpty) {
    return exact;
  }
  final lowerKey = key.toLowerCase();
  for (final entry in environment.entries) {
    if (entry.key.toLowerCase() == lowerKey && entry.value.isNotEmpty) {
      return entry.value;
    }
  }
  return null;
}

List<String> _uniqueWindowsPaths(Iterable<String> paths) {
  final seen = <String>{};
  final unique = <String>[];
  for (final path in paths) {
    final key = path.toLowerCase();
    if (seen.add(key)) {
      unique.add(path);
    }
  }
  return unique;
}

String _joinWindowsPath(
  String first,
  String second, [
  String? third,
  String? fourth,
  String? fifth,
]) {
  final parts = <String>[first, second, ?third, ?fourth, ?fifth];
  final buffer = StringBuffer();
  for (final part in parts) {
    if (part.isEmpty) {
      continue;
    }
    if (buffer.isNotEmpty && !buffer.toString().endsWith(r'\')) {
      buffer.write(r'\');
    }
    buffer.write(part.replaceAll('/', r'\').replaceAll(RegExp(r'\\+$'), ''));
  }
  return buffer.toString();
}

bool _fileExists(String path) => File(path).existsSync();

bool get _isWindowsDesktopTerminalTarget {
  return !kIsWeb && defaultTargetPlatform == TargetPlatform.windows;
}

GhosttyTerminalShellLaunch _launchInWorkingDirectory(
  GhosttyTerminalShellLaunch launch,
  String workingDirectory,
) {
  if (workingDirectory.trim().isEmpty) {
    return launch;
  }
  if (_isWindowsCommandPromptLaunch(launch)) {
    final shellWorkingDirectory = _windowsShellWorkingDirectory(
      workingDirectory,
    );
    return GhosttyTerminalShellLaunch(
      label: launch.label,
      shell: launch.shell,
      arguments: <String>[
        ...launch.arguments,
        '/d',
        '/s',
        '/k',
        'cd /d ${_cmdQuote(shellWorkingDirectory)}',
      ],
      environment: launch.environment,
      setupCommand: launch.setupCommand,
    );
  }
  if (_isWindowsPowerShellLaunch(launch)) {
    final shellWorkingDirectory = _windowsShellWorkingDirectory(
      workingDirectory,
    );
    return GhosttyTerminalShellLaunch(
      label: launch.label,
      shell: launch.shell,
      arguments: launch.arguments,
      environment: launch.environment,
      setupCommand: _prependSetupCommand(
        launch.setupCommand,
        "Set-Location -LiteralPath ${_powerShellQuote(shellWorkingDirectory)}\r\n",
      ),
    );
  }
  final execCommand = StringBuffer(
    'cd ${_shQuote(workingDirectory)} || true; exec ',
  )..write(_shQuote(launch.shell));
  for (final argument in launch.arguments) {
    execCommand
      ..write(' ')
      ..write(_shQuote(argument));
  }
  return GhosttyTerminalShellLaunch(
    label: launch.label,
    shell: '/bin/sh',
    arguments: <String>['-c', execCommand.toString()],
    environment: launch.environment,
    setupCommand: launch.setupCommand,
  );
}

String _windowsShellWorkingDirectory(String workingDirectory) {
  const extendedUncPrefix = r'\\?\UNC\';
  if (workingDirectory.startsWith(extendedUncPrefix)) {
    return r'\\' + workingDirectory.substring(extendedUncPrefix.length);
  }

  const extendedPathPrefix = r'\\?\';
  if (workingDirectory.startsWith(extendedPathPrefix)) {
    final path = workingDirectory.substring(extendedPathPrefix.length);
    if (RegExp(r'^[A-Za-z]:\\').hasMatch(path)) {
      return path;
    }
  }
  return workingDirectory;
}

@visibleForTesting
GhosttyTerminalShellLaunch launchInWorkingDirectoryForTesting(
  GhosttyTerminalShellLaunch launch,
  String workingDirectory,
) {
  return _launchInWorkingDirectory(launch, workingDirectory);
}

List<GhosttyTerminalShellLaunch> terminalShellLaunches({
  String? powerShell7ExecutablePath,
}) {
  return _terminalShellLaunches(
    powerShell7ExecutablePath: powerShell7ExecutablePath,
  );
}

@visibleForTesting
Map<String, String> terminalPlatformEnvironmentForTesting() {
  return _terminalPlatformEnvironment();
}

@visibleForTesting
List<GhosttyTerminalShellLaunch> terminalShellLaunchesForTesting() {
  return _terminalShellLaunches();
}

@visibleForTesting
List<GhosttyTerminalShellLaunch> windowsTerminalShellLaunchesForTesting(
  Map<String, String> platformEnvironment, {
  String? powerShell7ExecutablePath,
  required bool Function(String path) fileExists,
}) {
  return _windowsTerminalShellLaunches(
    platformEnvironment,
    powerShell7ExecutablePath: powerShell7ExecutablePath,
    fileExists: fileExists,
  );
}

@visibleForTesting
List<GhosttyTerminalShellLaunch> resolvedLoginShellFallbackLaunchesForTesting(
  Map<String, String> platformEnvironment, {
  required bool Function(String path) fileExists,
}) {
  return _resolvedLoginShellFallbackLaunches(
    platformEnvironment,
    fileExists: fileExists,
  );
}

bool _isWindowsCommandPromptLaunch(GhosttyTerminalShellLaunch launch) {
  return _shellExecutableName(launch) == 'cmd';
}

bool _isWindowsPowerShellLaunch(GhosttyTerminalShellLaunch launch) {
  return switch (_shellExecutableName(launch)) {
    'pwsh' || 'powershell' => true,
    _ => false,
  };
}

String _shellExecutableName(GhosttyTerminalShellLaunch launch) {
  final executable = launch.shell.replaceAll(r'\', '/').split('/').last;
  final lower = executable.toLowerCase();
  return lower.endsWith('.exe') ? lower.substring(0, lower.length - 4) : lower;
}

String _cmdQuote(String value) {
  return '"${value.replaceAll('"', '""')}"';
}

String _powerShellQuote(String value) {
  return "'${value.replaceAll("'", "''")}'";
}

String _prependSetupCommand(String? existing, String setupCommand) {
  if (existing == null || existing.isEmpty) {
    return setupCommand;
  }
  return '$setupCommand$existing';
}

@visibleForTesting
String cmdQuoteForTesting(String value) {
  return _cmdQuote(value);
}

@visibleForTesting
String powerShellQuoteForTesting(String value) {
  return _powerShellQuote(value);
}

String _shQuote(String value) {
  if (value.isEmpty) {
    return "''";
  }
  return "'${value.replaceAll("'", "'\"'\"'")}'";
}

@visibleForTesting
String shQuoteForTesting(String value) {
  return _shQuote(value);
}
