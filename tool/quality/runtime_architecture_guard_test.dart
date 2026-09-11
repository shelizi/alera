import 'dart:io';

final _repoRoot = Directory.current.absolute;
final _guardScript = File(
  '${_repoRoot.path}${Platform.pathSeparator}tool${Platform.pathSeparator}quality${Platform.pathSeparator}runtime_architecture_guard.dart',
).absolute.path;

Future<void> main() async {
  var failures = 0;

  Future<void> run(String name, Future<void> Function() body) async {
    try {
      await body();
      stdout.writeln('PASS $name');
    } catch (error, stackTrace) {
      failures += 1;
      stderr.writeln('FAIL $name');
      stderr.writeln(error);
      stderr.writeln(stackTrace);
    }
  }

  await run('rejects direct application to presentation imports', () async {
    final result = await _runFixture(<String, String>{
      'lib/src/features/example/application/example_service.dart': "import 'package:alera/src/features/example/presentation/example_view.dart';\n",
      'lib/src/features/example/presentation/example_view.dart': '',
    });
    _expectFailure(result, 'Application code imports presentation code');
  });

  await run('rejects relative cross-feature workbench infra imports', () async {
    final result = await _runFixture(<String, String>{
      'lib/src/features/example/application/example_service.dart':
          "import '../../workbench/infra/runtime.dart';\n",
      'lib/src/features/workbench/infra/runtime.dart': '',
    });
    _expectFailure(
      result,
      'Application code imports Workbench infrastructure across features',
    );
  });

  await run(
    'rejects presentation imports hidden behind a barrel export',
    () async {
      final result = await _runFixture(<String, String>{
        'lib/src/features/example/application/example_service.dart':
            "import '../example_api.dart';\n",
        'lib/src/features/example/example_api.dart':
            "export 'presentation/example_view.dart';\n",
        'lib/src/features/example/presentation/example_view.dart': '',
      });
      _expectFailure(result, 'Application code reaches presentation code');
    },
  );

  await run('allows application dependencies on domain code', () async {
    final result = await _runFixture(<String, String>{
      'lib/src/features/example/application/example_service.dart':
          "import '../../other/domain/model.dart';\n",
      'lib/src/features/other/domain/model.dart': 'final class Model {}\n',
    });
    if (result.exitCode != 0) {
      throw StateError(
        'expected guard success, got ${result.exitCode}: ${result.stderr}',
      );
    }
  });

  await run('PR static checks execute the architecture guard', () async {
    final workflow = File(
      '${_repoRoot.path}${Platform.pathSeparator}.github${Platform.pathSeparator}workflows${Platform.pathSeparator}pr.yml',
    ).readAsStringSync();
    const invocation = 'dart tool/quality/runtime_architecture_guard.dart';
    if (!workflow.contains(invocation)) {
      throw StateError('missing PR static check: $invocation');
    }
  });

  if (failures != 0) {
    stderr.writeln('$failures architecture guard test(s) failed.');
    exitCode = 1;
    return;
  }
  stdout.writeln('Runtime architecture guard tests passed.');
}

Future<ProcessResult> _runFixture(Map<String, String> files) async {
  final root = await Directory.systemTemp.createTemp('alera-runtime-guard-');
  try {
    for (final path in <String>[
      'lib/src/platform/runtime_host/protocol/terminal_host_protocol.dart',
      'lib/src/platform/runtime_host/transport/terminal_host_frame_codec.dart',
      'lib/src/platform/runtime_host/transport/terminal_host_socket_isolate.dart',
      'lib/src/shared/infra/runtime/alera_cli_sidecar.dart',
    ]) {
      _write(root, path, '');
    }
    for (final entry in files.entries) {
      _write(root, entry.key, entry.value);
    }
    return await Process.run(Platform.resolvedExecutable, <String>[
      _guardScript,
    ], workingDirectory: root.path);
  } finally {
    await root.delete(recursive: true);
  }
}

void _write(Directory root, String relativePath, String content) {
  final path = relativePath.replaceAll('/', Platform.pathSeparator);
  final file = File('${root.path}${Platform.pathSeparator}$path');
  file.parent.createSync(recursive: true);
  file.writeAsStringSync(content);
}

void _expectFailure(ProcessResult result, String message) {
  if (result.exitCode == 0) {
    throw StateError(
      'expected guard failure containing "$message", but it passed',
    );
  }
  final output = '${result.stdout}\n${result.stderr}';
  if (!output.contains(message)) {
    throw StateError('expected "$message" in guard output:\n$output');
  }
}
