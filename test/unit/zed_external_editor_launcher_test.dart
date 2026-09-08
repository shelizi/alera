import 'dart:async';
import 'dart:io';

import 'package:alera/src/features/external_editor/domain/external_editor_launch_result.dart';
import 'package:alera/src/features/external_editor/domain/external_editor_launcher.dart';
import 'package:alera/src/features/external_editor/infra/zed_external_editor_launcher.dart';
import 'package:alera/src/shared/infra/process/process_runner.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('workspace launch uses --new and one absolute path argument', () async {
    final runner = _FakeProcessRunner();
    final launcher = _launcher(runner, existing: <String>{'/repo with spaces'});

    final result = await launcher.openWorkspace('/repo with spaces');

    expect(result.ok, isTrue);
    expect(runner.starts.single.executable, 'zed');
    expect(runner.starts.single.arguments, <String>[
      '--new',
      '/repo with spaces',
    ]);
    expect(runner.starts.single.workingDirectory, '/repo with spaces');
  });

  test('default workspace mode omits --new', () async {
    final runner = _FakeProcessRunner();
    final launcher = _launcher(
      runner,
      existing: <String>{'/repo'},
      workspaceMode: .defaultWindow,
    );

    await launcher.openWorkspace('/repo');

    expect(runner.starts.single.arguments, <String>['/repo']);
  });

  test('explicit executable path overrides the zed command name', () async {
    final runner = _FakeProcessRunner();
    final launcher = _launcher(
      runner,
      existing: <String>{'/repo'},
      command: ' /opt/Zed Preview/bin/zed ',
    );

    await launcher.openWorkspace('/repo');

    expect(runner.starts.single.executable, '/opt/Zed Preview/bin/zed');
  });

  test(
    'file launch preserves spaces unicode and a leading dash in one argument',
    () async {
      final runner = _FakeProcessRunner();
      const workspace = '/repo 專案';
      const file = '/repo 專案/src/-測試 file.dart';
      final launcher = _launcher(runner, existing: <String>{workspace, file});

      final result = await launcher.openFile(
        const ExternalEditorOpenRequest(
          workspacePath: workspace,
          filePath: file,
        ),
      );

      expect(result.ok, isTrue);
      expect(runner.starts.single.arguments, <String>[file]);
    },
  );

  test('line and column are appended to the single file argument', () async {
    final runner = _FakeProcessRunner();
    const file = '/repo/src/main.dart';
    final launcher = _launcher(runner, existing: <String>{'/repo', file});

    await launcher.openFile(
      const ExternalEditorOpenRequest(
        workspacePath: '/repo',
        filePath: file,
        line: 42,
        column: 7,
      ),
    );

    expect(runner.starts.single.arguments, <String>['$file:42:7']);
  });

  test('Windows drive-letter file target remains one argument', () async {
    final runner = _FakeProcessRunner();
    const workspace = r'C:\Work Trees\Alera';
    const file = r'C:\Work Trees\Alera\src\main.rs';
    final launcher = _launcher(runner, existing: <String>{workspace, file});

    await launcher.openFile(
      const ExternalEditorOpenRequest(
        workspacePath: workspace,
        filePath: file,
        line: 9,
        column: 3,
      ),
    );

    expect(runner.starts.single.arguments, <String>[
      r'C:\Work Trees\Alera\src\main.rs:9:3',
    ]);
  });

  test(
    'missing and outside-workspace targets fail before process spawn',
    () async {
      final runner = _FakeProcessRunner();
      final launcher = _launcher(
        runner,
        existing: <String>{'/repo', '/other/file.dart'},
      );

      final missing = await launcher.openWorkspace('/missing');
      final outside = await launcher.openFile(
        const ExternalEditorOpenRequest(
          workspacePath: '/repo',
          filePath: '/other/file.dart',
        ),
      );

      expect(
        missing.failureKind,
        ExternalEditorLaunchFailureKind.invalidTarget,
      );
      expect(
        outside.failureKind,
        ExternalEditorLaunchFailureKind.invalidTarget,
      );
      expect(runner.starts, isEmpty);
    },
  );

  test(
    'canonical paths reject a symlink escape outside the workspace',
    () async {
      final runner = _FakeProcessRunner();
      const workspace = '/repo';
      const file = '/repo/link/secret.dart';
      final launcher = _launcher(
        runner,
        existing: <String>{workspace, file},
        canonicalPaths: const <String, String>{
          workspace: workspace,
          file: '/outside/secret.dart',
        },
      );

      final result = await launcher.openFile(
        const ExternalEditorOpenRequest(
          workspacePath: workspace,
          filePath: file,
        ),
      );

      expect(result.failureKind, ExternalEditorLaunchFailureKind.invalidTarget);
      expect(result.message, contains('outside'));
      expect(runner.starts, isEmpty);
    },
  );

  test(
    'canonical paths allow a workspace symlink to its real target',
    () async {
      final runner = _FakeProcessRunner();
      const workspace = '/repo-link';
      const file = '/repo-link/src/main.dart';
      final launcher = _launcher(
        runner,
        existing: <String>{workspace, file},
        canonicalPaths: const <String, String>{
          workspace: '/real/repo',
          file: '/real/repo/src/main.dart',
        },
      );

      final result = await launcher.openFile(
        const ExternalEditorOpenRequest(
          workspacePath: workspace,
          filePath: file,
        ),
      );

      expect(result.ok, isTrue);
      expect(runner.starts.single.arguments, <String>[file]);
      expect(runner.starts.single.workingDirectory, workspace);
    },
  );
  test('spawn failure returns an actionable Zed-specific result', () async {
    final runner = _FakeProcessRunner()
      ..startFailure = const ProcessException('zed', <String>[], 'missing');
    final launcher = _launcher(runner, existing: <String>{'/repo'});

    final result = await launcher.openWorkspace('/repo');

    expect(result.ok, isFalse);
    expect(result.failureKind, ExternalEditorLaunchFailureKind.unavailable);
    expect(result.message, contains('Zed'));
    expect(result.message, contains('Settings > Editor'));
  });

  test('availability check uses a non-destructive --version probe', () async {
    final runner = _FakeProcessRunner(
      runOutput: const ProcessRunOutput(
        stdout: 'Zed 0.201.3\n',
        stderr: '',
        exitCode: 0,
      ),
    );
    final launcher = _launcher(runner, existing: const <String>{});

    final result = await launcher.checkAvailability();

    expect(result.available, isTrue);
    expect(result.version, 'Zed 0.201.3');
    expect(runner.runs.single.arguments, <String>['--version']);
  });
}

ZedExternalEditorLauncher _launcher(
  _FakeProcessRunner runner, {
  required Set<String> existing,
  String? command,
  ExternalEditorWorkspaceMode workspaceMode = .newWindow,
  Map<String, String>? canonicalPaths,
}) => ZedExternalEditorLauncher(
  processRunner: runner,
  commandReader: () => command,
  workspaceModeReader: () => workspaceMode,
  pathExists: existing.contains,
  pathCanonicalizer: (path) => canonicalPaths?[path] ?? path,
);

class _FakeProcessRunner implements ProcessRunner {
  _FakeProcessRunner({
    this.runOutput = const ProcessRunOutput(
      stdout: '',
      stderr: '',
      exitCode: 0,
    ),
  });

  final ProcessRunOutput runOutput;
  final List<_ProcessCall> starts = <_ProcessCall>[];
  final List<_ProcessCall> runs = <_ProcessCall>[];
  Object? startFailure;

  @override
  Future<ProcessRunOutput> run(
    String executable,
    List<String> arguments, {
    String? workingDirectory,
    Map<String, String>? environment,
  }) async {
    runs.add(
      _ProcessCall(executable, List<String>.of(arguments), workingDirectory),
    );
    return runOutput;
  }

  @override
  Future<StartedProcess> start(
    String executable,
    List<String> arguments, {
    String? workingDirectory,
    Map<String, String>? environment,
    bool includeParentEnvironment = true,
  }) async {
    starts.add(
      _ProcessCall(executable, List<String>.of(arguments), workingDirectory),
    );
    final failure = startFailure;
    if (failure != null) throw failure;
    return StartedProcess(
      stdinWrite: (_) {},
      stdout: const Stream<List<int>>.empty(),
      stderr: const Stream<List<int>>.empty(),
      pid: 1,
      exitCode: Completer<int>().future,
      kill: ([dynamic _]) => true,
    );
  }
}

class const _ProcessCall(
  final String executable,
  final List<String> arguments,
  final String? workingDirectory,
);
