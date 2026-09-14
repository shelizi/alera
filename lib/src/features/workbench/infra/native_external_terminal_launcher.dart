import 'dart:async';
import 'dart:io';

import 'package:alera/src/features/workbench/domain/external_terminal_launcher.dart';
import 'package:alera/src/shared/infra/runtime/alera_cli_sidecar.dart';
import 'package:alera/src/shared/infra/process/process_runner.dart';
import 'package:path/path.dart' as p;
import 'package:path_provider/path_provider.dart';

typedef ExternalTerminalOperatingSystemReader = String Function();
typedef ExternalTerminalExecutableReader = String? Function(
  String operatingSystem,
);
typedef GitBashExecutableReader = String? Function();

final class const ExternalTerminalInvocation({
  required final String executable,
  required final List<String> arguments,
  final String? workingDirectory,
});

class NativeExternalTerminalLauncher implements ExternalTerminalLauncher {
  NativeExternalTerminalLauncher({
    required ProcessRunner processRunner,
    required AleraCliResolver cliResolver,
    Future<Directory> Function()? applicationSupportDirectory,
    ExternalTerminalOperatingSystemReader? operatingSystemReader,
    ExternalTerminalExecutableReader? executableReader,
    String? gitBashExecutablePath,
    GitBashExecutableReader? gitBashExecutableReader,
  }) : _processRunner = processRunner,
       _cliResolver = cliResolver,
       _applicationSupportDirectory =
           applicationSupportDirectory ?? getApplicationSupportDirectory,
       _operatingSystemReader =
           operatingSystemReader ?? (() => Platform.operatingSystem),
       _executableReader = executableReader ?? _defaultTerminalExecutable,
       _gitBashExecutablePath = gitBashExecutablePath?.trim(),
       _gitBashExecutableReader =
           gitBashExecutableReader ?? _defaultGitBashExecutable;

  final ProcessRunner _processRunner;
  final AleraCliResolver _cliResolver;
  final Future<Directory> Function() _applicationSupportDirectory;
  final ExternalTerminalOperatingSystemReader _operatingSystemReader;
  final ExternalTerminalExecutableReader _executableReader;
  final String? _gitBashExecutablePath;
  final GitBashExecutableReader _gitBashExecutableReader;

  @override
  Future<ExternalTerminalLaunchResult> open(
    ExternalTerminalOpenRequest request,
  ) async {
    if (request.terminalSessionId.trim().isEmpty) {
      return externalTerminalLaunchFailure(
        .invalidTarget,
        'The terminal session is not available for external attach.',
      );
    }

    final operatingSystem = _operatingSystemReader();
    final executable = _executableReader(operatingSystem);
    if (executable == null || executable.trim().isEmpty) {
      return externalTerminalLaunchFailure(
        .unavailable,
        'External terminal attach is not supported on this platform.',
      );
    }

    try {
      final support = await _applicationSupportDirectory();
      final runtimeDirectory = Directory(p.join(support.path, 'terminal_host'));
      if (!await runtimeDirectory.exists()) {
        await runtimeDirectory.create(recursive: true);
      }
      final cli = await _cliResolver.resolve(runtimeDir: runtimeDirectory.path);
      final primary = buildExternalTerminalInvocation(
        operatingSystem: operatingSystem,
        terminalExecutable: executable,
        cli: cli,
        runtimeDirectory: runtimeDirectory.path,
        request: request,
      );
      if (await _tryStart(primary)) {
        return ExternalTerminalLaunchResultFactories.opened;
      }
      if (operatingSystem == 'windows') {
        final gitBashExecutables = <String>[];
        void addGitBashExecutable(String? candidate) {
          final normalized = candidate?.trim();
          if (normalized == null ||
              normalized.isEmpty ||
              gitBashExecutables.contains(normalized)) {
            return;
          }
          gitBashExecutables.add(normalized);
        }

        addGitBashExecutable(_gitBashExecutablePath);
        addGitBashExecutable(_gitBashExecutableReader());
        for (final gitBashExecutable in gitBashExecutables) {
          final fallback = buildGitBashExternalTerminalInvocation(
            gitBashExecutable: gitBashExecutable,
            cli: cli,
            runtimeDirectory: runtimeDirectory.path,
            request: request,
          );
          if (await _tryStart(fallback)) {
            return ExternalTerminalLaunchResultFactories.opened;
          }
        }
      }
      return externalTerminalLaunchFailure(
        .unavailable,
        'Could not start the native terminal. Check that a terminal emulator is installed.',
      );
    } on ProcessException {
      return externalTerminalLaunchFailure(
        .unavailable,
        'Could not start the native terminal. Check that a terminal emulator is installed.',
      );
    } on Object {
      return externalTerminalLaunchFailure(
        .failed,
        'Could not open this terminal in a native window.',
      );
    }
  }

  Future<bool> _tryStart(ExternalTerminalInvocation invocation) async {
    try {
      final process = await _processRunner.start(
        invocation.executable,
        invocation.arguments,
        workingDirectory: invocation.workingDirectory,
      );
      process.stdout.listen((_) {}, onError: (Object _) {});
      process.stderr.listen((_) {}, onError: (Object _) {});
      unawaited(process.exitCode.catchError((Object _) => -1));
      return true;
    } on ProcessException {
      return false;
    }
  }
}

ExternalTerminalInvocation buildExternalTerminalInvocation({
  required String operatingSystem,
  required String terminalExecutable,
  required AleraCliCommand cli,
  required String runtimeDirectory,
  required ExternalTerminalOpenRequest request,
}) {
  final cliArguments = <String>[
    ...cli.prefixArguments,
    'terminal',
    '--runtime-dir',
    runtimeDirectory,
    'attach',
    '--handle',
    request.terminalSessionId,
  ];
  final workingDirectory = cli.workingDirectory ?? request.workspacePath;

  return switch (operatingSystem) {
    'windows' => ExternalTerminalInvocation(
      executable: terminalExecutable,
      arguments: <String>[
        'new-tab',
        '--title',
        _nonBlank(request.title) ?? 'Alera Terminal',
        '--startingDirectory',
        workingDirectory,
        cli.executable,
        ...cliArguments,
      ],
      workingDirectory: null,
    ),
    'macos' => ExternalTerminalInvocation(
      executable: terminalExecutable,
      arguments: <String>[
        '-e',
        'on run argv',
        '-e',
        'tell application "Terminal" to do script (item 1 of argv)',
        '-e',
        'tell application "Terminal" to activate',
        '-e',
        'end run',
        _buildPosixCommand(
          cli: cli,
          arguments: cliArguments,
          workingDirectory: workingDirectory,
        ),
      ],
      workingDirectory: null,
    ),
    _ => ExternalTerminalInvocation(
      executable: terminalExecutable,
      arguments: <String>['-e', cli.executable, ...cliArguments],
      workingDirectory: workingDirectory,
    ),
  };
}

ExternalTerminalInvocation buildGitBashExternalTerminalInvocation({
  required String gitBashExecutable,
  required AleraCliCommand cli,
  required String runtimeDirectory,
  required ExternalTerminalOpenRequest request,
}) {
  final cliArguments = <String>[
    ...cli.prefixArguments,
    'terminal',
    '--runtime-dir',
    runtimeDirectory,
    'attach',
    '--handle',
    request.terminalSessionId,
  ];
  final workingDirectory = cli.workingDirectory ?? request.workspacePath;
  final executable = _gitBashPath(cli.executable);
  final command = <String>[
    'cd ${_posixShellQuote(_gitBashPath(workingDirectory))}',
    '&&',
    'exec ${_posixShellQuote(executable)}',
    for (final argument in cliArguments) _posixShellQuote(argument),
  ].join(' ');
  return ExternalTerminalInvocation(
    executable: gitBashExecutable,
    arguments: <String>[
      r'--command=usr\bin\bash.exe',
      '--login',
      '-c',
      command,
    ],
    workingDirectory: null,
  );
}

String _gitBashPath(String value) => value.replaceAll('\\', '/');

String? _defaultGitBashExecutable() {
  final environment = Platform.environment;
  final candidates = <String>[
    if (environment['ProgramFiles'] case final root?)
      p.join(root, 'Git', 'git-bash.exe'),
    if (environment['ProgramW6432'] case final root?)
      p.join(root, 'Git', 'git-bash.exe'),
    if (environment['ProgramFiles(x86)'] case final root?)
      p.join(root, 'Git', 'git-bash.exe'),
    if (environment['LOCALAPPDATA'] case final root?)
      p.join(root, 'Programs', 'Git', 'git-bash.exe'),
  ];
  for (final candidate in candidates) {
    if (File(candidate).existsSync()) {
      return candidate;
    }
  }
  // A PATH installation remains worth trying after the standard locations.
  return 'git-bash.exe';
}

String? _defaultTerminalExecutable(String operatingSystem) =>
    switch (operatingSystem) {
      'windows' => 'wt.exe',
      'macos' => '/usr/bin/osascript',
      'linux' => 'x-terminal-emulator',
      _ => null,
    };

String _buildPosixCommand({
  required AleraCliCommand cli,
  required List<String> arguments,
  required String workingDirectory,
}) {
  final command = <String>[
    'exec',
    _posixShellQuote(cli.executable),
    for (final argument in arguments) _posixShellQuote(argument),
  ].join(' ');
  return 'cd ${_posixShellQuote(workingDirectory)} && $command';
}

String _posixShellQuote(String value) {
  final doubleQuote = String.fromCharCode(34);
  final escaped = value.replaceAll("'", "'$doubleQuote'$doubleQuote'");
  return "'" + escaped + "'";
}

String? _nonBlank(String? value) {
  final trimmed = value?.trim();
  return trimmed == null || trimmed.isEmpty ? null : trimmed;
}
