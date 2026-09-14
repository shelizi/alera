import 'dart:io';

final _repoRoot = Directory.current.absolute;
final _guardScript = File(
  '${_repoRoot.path}${Platform.pathSeparator}tool${Platform.pathSeparator}quality${Platform.pathSeparator}agent_extension_guard.dart',
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

  await run('rejects a new synthetic agent-specific touchpoint', () async {
    final result = await _runFixture(
      files: <String, String>{
        'lib/src/features/example/example.dart': 'bool isSynthetic(AgentType type) => type == AgentType.synthetic;\n',
      },
    );
    _expectFailure(result, 'New agent-extension touchpoint');
  });

  await run('allows an explicitly baselined synthetic touchpoint', () async {
    const path = 'lib/src/features/example/example.dart';
    final result = await _runFixture(
      files: <String, String>{
        path: 'bool isSynthetic(AgentType type) => type == AgentType.synthetic;\n',
      },
      baseline: const <String>[path],
    );
    _expectSuccess(result);
  });

  await run('forces the touchpoint baseline to ratchet down', () async {
    const path = 'lib/src/features/example/example.dart';
    final result = await _runFixture(
      files: const <String, String>{path: 'void noop() {}\n'},
      baseline: const <String>[path],
    );
    _expectFailure(result, 'baseline contains stale entries');
  });

  await run('current repository stays within the ratchet', () async {
    final result = await Process.run(Platform.resolvedExecutable, <String>[
      _guardScript,
    ], workingDirectory: _repoRoot.path);
    _expectSuccess(result);
  });

  if (failures != 0) {
    stderr.writeln('$failures agent extension guard test(s) failed.');
    exitCode = 1;
    return;
  }
  stdout.writeln('Agent extension guard tests passed.');
}

Future<ProcessResult> _runFixture({
  required Map<String, String> files,
  List<String> baseline = const <String>[],
}) async {
  final root = await Directory.systemTemp.createTemp('alera-agent-extension-');
  try {
    _write(
      root,
      'lib/src/features/agent_status/domain/agent_status.dart',
      "enum AgentType(this.key) { synthetic('synthetic'); final String key; }\n",
    );
    for (final entry in files.entries) {
      _write(root, entry.key, entry.value);
    }
    _write(
      root,
      'tool/quality/agent_extension_touchpoints.txt',
      baseline.isEmpty ? '' : '${baseline.join('\n')}\n',
    );
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

void _expectSuccess(ProcessResult result) {
  if (result.exitCode != 0) {
    throw StateError(
      'expected guard success, got ${result.exitCode}: '
      '${result.stdout}\n${result.stderr}',
    );
  }
}
