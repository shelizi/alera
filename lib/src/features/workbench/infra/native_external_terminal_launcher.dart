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
  }) : _processRunner = processRunner,
       _cliResolver = cliResolver,
       _applicationSupportDirectory =
           applicationSupportDirectory ?? getApplicationSupportDirectory,
       _operatingSystemReader =
           operatingSystemReader ?? (() => Platform.operatingSystem),
       _executableReader = executableReader ?? _defaultTerminalExecutable;

  final ProcessRunner _processRunner;
  final AleraCliResolver _cliResolver;
  final Future<Directory> Function() _applicationSupportDirectory;
  final ExternalTerminalOperatingSystemReader _operatingSystemReader;
  final ExternalTerminalExecutableReader _executableReader;

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
      final invocation = buildExternalTerminalInvocation(
        operatingSystem: operatingSystem,
        terminalExecutable: executable,
        cli: cli,
        runtimeDirectory: runtimeDirectory.path,
        request: request,
      );
      final process = await _processRunner.start(
        invocation.executable,
        invocation.arguments,
        workingDirectory: invocation.workingDirectory,
      );
      process.stdout.listen((_) {}, onError: (Object _) {});
      process.stderr.listen((_) {}, onError: (Object _) {});
      unawaited(process.exitCode.catchError((Object _) => -1));
      return ExternalTerminalLaunchResultFactories.opened;
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
