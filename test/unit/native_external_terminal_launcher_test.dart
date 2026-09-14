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
    'launcher prefers configured Git Bash path after Windows Terminal fails',
    () async {
      final support = await Directory.systemTemp.createTemp(
        'alera-external-terminal-test',
      );
      addTearDown(() => support.delete(recursive: true));
      final runner = _FakeProcessRunner()
        ..startFailures['wt.exe'] = const ProcessException(
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
        gitBashExecutablePath: r'D:\PortableGit\git-bash.exe',
        gitBashExecutableReader: () => r'C:\Program Files\Git\git-bash.exe',
      );

      final result = await launcher.open(
        const ExternalTerminalOpenRequest(
          workspacePath: r'C:\Work Trees\Alera',
          terminalSessionId: 'session-1',
          title: 'Agent shell',
        ),
      );

      expect(result.ok, isTrue);
      expect(runner.starts, hasLength(2));
      expect(runner.starts.last.executable, r'D:\PortableGit\git-bash.exe');
    },
  );

  test(
    'launcher falls back to detected Git Bash when configured path fails',
    () async {
      final support = await Directory.systemTemp.createTemp(
        'alera-external-terminal-test',
      );
      addTearDown(() => support.delete(recursive: true));
      final runner = _FakeProcessRunner()
        ..startFailures['wt.exe'] = const ProcessException(
          'wt.exe',
          <String>[],
          'missing',
        )
        ..startFailures[r'D:\PortableGit\git-bash.exe'] =
            const ProcessException(
              r'D:\PortableGit\git-bash.exe',
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
        gitBashExecutablePath: r'D:\PortableGit\git-bash.exe',
        gitBashExecutableReader: () => r'C:\Program Files\Git\git-bash.exe',
      );

      final result = await launcher.open(
        const ExternalTerminalOpenRequest(
          workspacePath: r'C:\Work Trees\Alera',
          terminalSessionId: 'session-1',
          title: 'Agent shell',
        ),
      );

      expect(result.ok, isTrue);
      expect(runner.starts.map((invocation) => invocation.executable), <String>[
        'wt.exe',
        r'D:\PortableGit\git-bash.exe',
        r'C:\Program Files\Git\git-bash.exe',
      ]);
    },
  );

  test('launcher falls back from Windows Terminal to Git Bash', () async {
    final support = await Directory.systemTemp.createTemp(
      'alera-external-terminal-test',
    );
    addTearDown(() => support.delete(recursive: true));
    final runner = _FakeProcessRunner()
      ..startFailures['wt.exe'] = const ProcessException(
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
      gitBashExecutableReader: () => r'C:\Program Files\Git\git-bash.exe',
    );

    final result = await launcher.open(
      const ExternalTerminalOpenRequest(
        workspacePath: r'C:\Work Trees\Alera',
        terminalSessionId: 'session-1',
        title: 'Agent shell',
      ),
    );

    expect(result.ok, isTrue);
    expect(runner.starts, hasLength(2));
    expect(runner.starts.first.executable, 'wt.exe');
    final gitBash = runner.starts.last;
    expect(gitBash.executable, r'C:\Program Files\Git\git-bash.exe');
    expect(gitBash.workingDirectory, isNull);
    expect(gitBash.arguments, contains(r'--command=usr\bin\bash.exe'));
    expect(gitBash.arguments, contains('--login'));
    expect(gitBash.arguments, contains('-c'));
    final command = gitBash.arguments.last;
    expect(command, contains("exec 'C:/Alera/alera.exe'"));
    expect(command, contains("'terminal'"));
    expect(command, contains("'attach'"));
    expect(command, contains("'session-1'"));
  });

  test(
    'launcher reports unavailable after Windows Terminal and Git Bash fail',
    () async {
      final support = await Directory.systemTemp.createTemp(
        'alera-external-terminal-test',
      );
      addTearDown(() => support.delete(recursive: true));
      final runner = _FakeProcessRunner()
        ..startFailures['wt.exe'] = const ProcessException(
          'wt.exe',
          <String>[],
          'missing',
        )
        ..startFailures[r'C:\Program Files\Git\git-bash.exe'] =
            const ProcessException(
              r'C:\Program Files\Git\git-bash.exe',
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
        gitBashExecutableReader: () => r'C:\Program Files\Git\git-bash.exe',
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
      expect(runner.starts, hasLength(2));
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
  final Map<String, ProcessException> startFailures =
      <String, ProcessException>{};
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
    final failure = startFailures[executable] ?? startFailure;
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
