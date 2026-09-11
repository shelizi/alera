import 'dart:async';
import 'dart:io';

import 'package:alera/src/features/workbench/domain/external_terminal_launcher.dart';
import 'package:alera/src/features/workbench/infra/native_external_terminal_launcher.dart';
import 'package:alera/src/shared/infra/runtime/alera_cli_sidecar.dart';
import 'package:alera/src/shared/infra/process/process_runner.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('Windows invocation opens a new Windows Terminal tab', () {
    final invocation = buildExternalTerminalInvocation(
      operatingSystem: 'windows',
      terminalExecutable: 'wt.exe',
      cli: const AleraCliCommand(executable: r'C:\Alera\alera.exe'),
      runtimeDirectory: r'C:\Users\tester\AppData\Local\Alera\terminal_host',
      request: const ExternalTerminalOpenRequest(
        workspacePath: r'C:\Work Trees\Alera',
        terminalSessionId: 'session-1',
        title: 'Agent shell',
      ),
    );

    expect(invocation.executable, 'wt.exe');
    expect(invocation.workingDirectory, isNull);
    expect(invocation.arguments, <String>[
      'new-tab',
      '--title',
      'Agent shell',
      '--startingDirectory',
      r'C:\Work Trees\Alera',
      r'C:\Alera\alera.exe',
      'terminal',
      '--runtime-dir',
      r'C:\Users\tester\AppData\Local\Alera\terminal_host',
      'attach',
      '--handle',
      'session-1',
    ]);
  });

  test('Linux invocation keeps the CLI arguments separate', () {
    final invocation = buildExternalTerminalInvocation(
      operatingSystem: 'linux',
      terminalExecutable: 'x-terminal-emulator',
      cli: const AleraCliCommand(executable: '/opt/alera/alera'),
      runtimeDirectory: '/home/tester/.local/share/alera/terminal_host',
      request: const ExternalTerminalOpenRequest(
        workspacePath: '/work/alera project',
        terminalSessionId: 'session with spaces',
        title: 'ignored on linux',
      ),
    );

    expect(invocation.arguments, <String>[
      '-e',
      '/opt/alera/alera',
      'terminal',
      '--runtime-dir',
      '/home/tester/.local/share/alera/terminal_host',
      'attach',
      '--handle',
      'session with spaces',
    ]);
    expect(invocation.workingDirectory, '/work/alera project');
  });

  test(
    'macOS invocation quotes paths before asking Terminal.app to run them',
    () {
      final invocation = buildExternalTerminalInvocation(
        operatingSystem: 'macos',
        terminalExecutable: '/usr/bin/osascript',
        cli: const AleraCliCommand(
          executable: '/Users/tester/Alera CLI/alera',
          prefixArguments: <String>['--profile', 'work profile'],
        ),
        runtimeDirectory:
            '/Users/tester/Library/Application Support/Alera/terminal_host',
        request: const ExternalTerminalOpenRequest(
          workspacePath: '/Users/tester/Work Trees/Alera',
          terminalSessionId: 'session-1',
          title: 'Agent shell',
        ),
      );

      expect(invocation.executable, '/usr/bin/osascript');
      expect(invocation.workingDirectory, isNull);
      expect(
        invocation.arguments,
        contains('tell application "Terminal" to do script (item 1 of argv)'),
      );
      final command = invocation.arguments.last;
      expect(command, contains("cd '/Users/tester/Work Trees/Alera'"));
      expect(command, contains("'/Users/tester/Alera CLI/alera'"));
      expect(command, contains("'work profile'"));
      expect(command, contains("'session-1'"));
    },
  );

  test(
    'launcher reports a missing native terminal without losing the session',
    () async {
      final support = await Directory.systemTemp.createTemp(
        'alera-external-terminal-test',
      );
      addTearDown(() => support.delete(recursive: true));
      final runner = _FakeProcessRunner()
        ..startFailure = const ProcessException(
          'wt.exe',
          <String>[],
          'missing',
        );
      final launcher = NativeExternalTerminalLauncher(
        processRunner: runner,
        cliResolver: _FakeAleraCliResolver(
          const AleraCliCommand(executable: r'C:\Alera\alera.exe'),
        ),
        applicationSupportDirectory: () async => support,
        operatingSystemReader: () => 'windows',
      );

      final result = await launcher.open(
        const ExternalTerminalOpenRequest(
          workspacePath: r'C:\Work Trees\Alera',
          terminalSessionId: 'session-1',
          title: 'Agent shell',
        ),
      );

      expect(result.ok, isFalse);
      expect(result.failureKind, ExternalTerminalLaunchFailureKind.unavailable);
      expect(runner.starts, hasLength(1));
    },
  );
}

final class _FakeAleraCliResolver implements AleraCliResolver {
  const _FakeAleraCliResolver(this.command);

  final AleraCliCommand command;

  @override
  Future<AleraCliCommand> resolve({required String runtimeDir}) async =>
      command;
}

final class _FakeProcessRunner implements ProcessRunner {
  final List<_StartedInvocation> starts = <_StartedInvocation>[];
  ProcessException? startFailure;

  @override
  Future<ProcessRunOutput> run(
    String executable,
    List<String> arguments, {
    String? workingDirectory,
    Map<String, String>? environment,
  }) async => const ProcessRunOutput(exitCode: 0, stdout: '', stderr: '');

  @override
  Future<StartedProcess> start(
    String executable,
    List<String> arguments, {
    String? workingDirectory,
    Map<String, String>? environment,
    bool includeParentEnvironment = true,
  }) async {
    starts.add(
      _StartedInvocation(
        executable: executable,
        arguments: arguments,
        workingDirectory: workingDirectory,
      ),
    );
    final failure = startFailure;
    if (failure != null) {
      throw failure;
    }
    return StartedProcess(
      stdinWrite: (_) {},
      stdout: const Stream<List<int>>.empty(),
      stderr: const Stream<List<int>>.empty(),
      pid: 1,
      exitCode: Future<int>.value(0),
      kill: ([dynamic _]) => true,
    );
  }
}

final class _StartedInvocation {
  const _StartedInvocation({
    required this.executable,
    required this.arguments,
    required this.workingDirectory,
  });

  final String executable;
  final List<String> arguments;
  final String? workingDirectory;
}
