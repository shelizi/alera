import 'dart:async';
import 'dart:io';

import 'package:alera/src/features/external_editor/domain/external_editor_launch_result.dart';
import 'package:alera/src/features/external_editor/domain/external_editor_launcher.dart';
import 'package:alera/src/features/external_editor/domain/external_editor_spec.dart';
import 'package:alera/src/features/external_editor/infra/cli_external_editor_launcher.dart';
import 'package:alera/src/shared/infra/process/process_runner.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('zed spec', () {
    test(
      'workspace launch uses --new and one absolute path argument',
      () async {
        final runner = _FakeProcessRunner();
        final launcher = _launcher(
          runner,
          existing: <String>{'/repo with spaces'},
        );

        final result = await launcher.openWorkspace('/repo with spaces');

        expect(result.ok, isTrue);
        expect(runner.starts.single.executable, 'zed');
        expect(runner.starts.single.arguments, <String>[
          '--new',
          '/repo with spaces',
        ]);
        expect(runner.starts.single.workingDirectory, '/repo with spaces');
      },
    );

    test('Windows extended-length workspace path is converted before validation and launch', () async {
      final runner = _FakeProcessRunner();
      const externalPath = r'E:\Dropbox\work\hermes';
      final launcher = _launcher(runner, existing: <String>{externalPath});

      final result = await launcher.openWorkspace(
        r'\\?\E:\Dropbox\work\hermes',
      );

      expect(result.ok, isTrue);
      expect(runner.starts.single.arguments, <String>['--new', externalPath]);
      expect(runner.starts.single.workingDirectory, externalPath);
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

        expect(
          result.failureKind,
          ExternalEditorLaunchFailureKind.invalidTarget,
        );
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

    test('bulk file launch validates all targets and starts once', () async {
      final runner = _FakeProcessRunner();
      const first = '/repo/lib/one.dart';
      const second = '/repo/lib/two.dart';
      final launcher = _launcher(
        runner,
        existing: <String>{'/repo', first, second},
      );

      final result = await launcher.openFiles(
        const ExternalEditorOpenFilesRequest(
          workspacePath: '/repo',
          filePaths: <String>[first, second, first],
        ),
      );

      expect(result.ok, isTrue);
      expect(runner.starts, hasLength(1));
      expect(runner.starts.single.arguments, <String>[first, second]);
    });

    test('bulk file launch rejects one escaping target before spawn', () async {
      final runner = _FakeProcessRunner();
      const inside = '/repo/lib/one.dart';
      const escaped = '/repo/link/secret.dart';
      final launcher = _launcher(
        runner,
        existing: <String>{'/repo', inside, escaped},
        canonicalPaths: const <String, String>{
          '/repo': '/repo',
          inside: inside,
          escaped: '/outside/secret.dart',
        },
      );

      final result = await launcher.openFiles(
        const ExternalEditorOpenFilesRequest(
          workspacePath: '/repo',
          filePaths: <String>[inside, escaped],
        ),
      );

      expect(result.failureKind, ExternalEditorLaunchFailureKind.invalidTarget);
      expect(runner.starts, isEmpty);
    });

    test(
      'spawn failure returns an actionable editor-specific result',
      () async {
        final runner = _FakeProcessRunner()
          ..startFailure = const ProcessException('zed', <String>[], 'missing');
        final launcher = _launcher(runner, existing: <String>{'/repo'});

        final result = await launcher.openWorkspace('/repo');

        expect(result.ok, isFalse);
        expect(result.failureKind, ExternalEditorLaunchFailureKind.unavailable);
        expect(result.message, contains('Zed'));
        expect(result.message, contains('Settings > Editor'));
      },
    );

    test('availability check uses a non-destructive --version probe', () async {
      final runner = _FakeProcessRunner(
        runOutput: const ProcessRunOutput(
          stdout: 'Zed 0.201.3\n',
          stderr: '',
          exitCode: 0,
        ),
      );
      final launcher = _launcher(
        runner,
        existing: <String>{'/tools/zed'},
        environment: <String, String>{'PATH': '/tools'},
      );

      final result = await launcher.checkAvailability();

      expect(result.available, isTrue);
      expect(result.version, 'Zed 0.201.3');
      expect(runner.runs.single.executable, 'zed');
      expect(runner.runs.single.arguments, <String>['--version']);
    });
  });

  group('vscode spec', () {
    CliExternalEditorLauncher vscodeLauncher(
      _FakeProcessRunner runner, {
      required Set<String> existing,
      String? command,
      ExternalEditorWorkspaceMode workspaceMode = .newWindow,
      Map<String, String>? canonicalPaths,
      Map<String, String>? environment,
    }) => _launcher(
      runner,
      spec: externalEditorSpecs[ExternalEditorKind.vscode]!,
      existing: existing,
      command: command,
      workspaceMode: workspaceMode,
      canonicalPaths: canonicalPaths,
      environment: environment,
    );

    test('new-window workspace launch uses --new-window', () async {
      final runner = _FakeProcessRunner();
      final launcher = vscodeLauncher(runner, existing: <String>{'/repo'});

      final result = await launcher.openWorkspace('/repo');

      expect(result.ok, isTrue);
      expect(runner.starts.single.arguments, <String>['--new-window', '/repo']);
    });

    test('default workspace mode reuses the window', () async {
      final runner = _FakeProcessRunner();
      final launcher = vscodeLauncher(
        runner,
        existing: <String>{'/repo'},
        workspaceMode: .defaultWindow,
      );

      await launcher.openWorkspace('/repo');

      expect(runner.starts.single.arguments, <String>[
        '--reuse-window',
        '/repo',
      ]);
    });

    test('file launch uses --goto with line and column', () async {
      final runner = _FakeProcessRunner();
      const file = '/repo/src/main.dart';
      final launcher = vscodeLauncher(
        runner,
        existing: <String>{'/repo', file},
      );

      await launcher.openFile(
        const ExternalEditorOpenRequest(
          workspacePath: '/repo',
          filePath: file,
          line: 12,
          column: 4,
        ),
      );

      expect(runner.starts.single.arguments, <String>['--goto', '$file:12:4']);
    });

    test('file launch without position still uses --goto', () async {
      final runner = _FakeProcessRunner();
      const file = '/repo/src/main.dart';
      final launcher = vscodeLauncher(
        runner,
        existing: <String>{'/repo', file},
      );

      await launcher.openFile(
        const ExternalEditorOpenRequest(workspacePath: '/repo', filePath: file),
      );

      expect(runner.starts.single.arguments, <String>['--goto', file]);
    });

    test('bulk file launch passes paths without --goto', () async {
      final runner = _FakeProcessRunner();
      const first = '/repo/lib/one.dart';
      const second = '/repo/lib/two.dart';
      final launcher = vscodeLauncher(
        runner,
        existing: <String>{'/repo', first, second},
      );

      await launcher.openFiles(
        const ExternalEditorOpenFilesRequest(
          workspacePath: '/repo',
          filePaths: <String>[first, second],
        ),
      );

      expect(runner.starts.single.arguments, <String>[first, second]);
    });

    test('availability parses the first line of code --version', () async {
      final runner = _FakeProcessRunner(
        runOutput: const ProcessRunOutput(
          stdout: '1.96.0\nabc123\nx64\n',
          stderr: '',
          exitCode: 0,
        ),
      );
      final launcher = vscodeLauncher(
        runner,
        existing: const <String>{},
        command: '/opt/vscode/bin/code',
      );

      final result = await launcher.checkAvailability();

      expect(result.available, isTrue);
      expect(result.version, '1.96.0');
    });

    test('spawn failure names Visual Studio Code', () async {
      final runner = _FakeProcessRunner()
        ..startFailure = const ProcessException('code', <String>[], 'missing');
      final launcher = vscodeLauncher(runner, existing: <String>{'/repo'});

      final result = await launcher.openWorkspace('/repo');

      expect(result.failureKind, ExternalEditorLaunchFailureKind.unavailable);
      expect(result.message, contains('Visual Studio Code'));
    });
  });

  group('installation probe', () {
    test('isInstalled resolves a command on PATH', () async {
      final launcher = _launcher(
        _FakeProcessRunner(),
        existing: <String>{'/tools/zed'},
        environment: <String, String>{'PATH': '/tools'},
      );

      expect(await launcher.isInstalled(), isTrue);
    });

    test('isInstalled is false when nothing resolves', () async {
      final launcher = _launcher(
        _FakeProcessRunner(),
        existing: const <String>{},
        environment: <String, String>{'PATH': '/empty'},
      );

      expect(await launcher.isInstalled(), isFalse);
    });

    test('isInstalled honors a configured executable path', () async {
      final launcher = _launcher(
        _FakeProcessRunner(),
        existing: <String>{'/custom/zed'},
        command: '/custom/zed',
        environment: <String, String>{'PATH': '/empty'},
      );

      expect(await launcher.isInstalled(), isTrue);
    });

    test('isInstalled falls back to expanded install paths', () async {
      final launcher = _launcher(
        _FakeProcessRunner(),
        spec: externalEditorSpecs[ExternalEditorKind.vscode]!,
        existing: <String>{
          r'C:\Users\me\AppData\Local\Programs\Microsoft VS Code\bin\code.cmd',
        },
        environment: <String, String>{
          'PATH': '/empty',
          'LOCALAPPDATA': r'C:\Users\me\AppData\Local',
        },
      );

      expect(await launcher.isInstalled(), isTrue);
    });

    test('checkAvailability reports missing when nothing resolves', () async {
      final launcher = _launcher(
        _FakeProcessRunner(),
        existing: const <String>{},
        environment: <String, String>{'PATH': '/empty'},
      );

      final result = await launcher.checkAvailability();

      expect(result.available, isFalse);
      expect(result.message, contains('Zed'));
    });

    test(
      'availability uses version arguments declared by the editor spec',
      () async {
        final runner = _FakeProcessRunner(
          runOutput: const ProcessRunOutput(
            stdout: 'Custom 1.2.3\n',
            stderr: '',
            exitCode: 0,
          ),
        );
        final launcher = _launcher(
          runner,
          spec: ExternalEditorSpec(
            kind: ExternalEditorKind.zed,
            displayName: 'Custom',
            shortName: 'Custom',
            commandCandidates: const <String>['custom-editor'],
            workspaceArgs: _plainWorkspaceArgs,
            fileArgs: _plainFileArgs,
            filesArgs: _plainFilesArgs,
            versionArgs: const <String>['version', '--short'],
          ),
          existing: const <String>{'/tools/custom-editor'},
          environment: const <String, String>{'PATH': '/tools'},
        );

        final result = await launcher.checkAvailability();

        expect(result.available, isTrue);
        expect(runner.runs.single.arguments, <String>['version', '--short']);
      },
    );

    test(
      'unsupported workspace mode ignores the global new-window preference',
      () async {
        final runner = _FakeProcessRunner();
        final launcher = _launcher(
          runner,
          spec: ExternalEditorSpec(
            kind: ExternalEditorKind.zed,
            displayName: 'Custom',
            shortName: 'Custom',
            commandCandidates: const <String>['custom-editor'],
            workspaceArgs: _newWindowAwareWorkspaceArgs,
            fileArgs: _plainFileArgs,
            filesArgs: _plainFilesArgs,
            supportsWorkspaceWindowMode: false,
          ),
          existing: const <String>{'/repo', '/tools/custom-editor'},
          environment: const <String, String>{'PATH': '/tools'},
          workspaceMode: .newWindow,
        );

        final result = await launcher.openWorkspace('/repo');

        expect(result.ok, isTrue);
        expect(runner.starts.single.arguments, <String>['/repo']);
      },
    );
  });
}

List<String> _plainWorkspaceArgs({required bool newWindow}) => const <String>[];

List<String> _newWindowAwareWorkspaceArgs({required bool newWindow}) =>
    newWindow ? const <String>['--new'] : const <String>[];

List<String> _plainFileArgs(String filePath, {int? line, int? column}) =>
    <String>[filePath];

List<String> _plainFilesArgs(List<String> filePaths) => filePaths;

CliExternalEditorLauncher _launcher(
  _FakeProcessRunner runner, {
  required Set<String> existing,
  ExternalEditorSpec? spec,
  String? command,
  ExternalEditorWorkspaceMode workspaceMode = .newWindow,
  Map<String, String>? canonicalPaths,
  Map<String, String>? environment,
}) => CliExternalEditorLauncher(
  spec: spec ?? externalEditorSpecs[ExternalEditorKind.zed]!,
  processRunner: runner,
  commandReader: () => command,
  workspaceModeReader: () => workspaceMode,
  pathExists: existing.contains,
  pathCanonicalizer: (path) => canonicalPaths?[path] ?? path,
  environmentReader: () => environment ?? const <String, String>{'PATH': ''},
  isWindows: false,
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
