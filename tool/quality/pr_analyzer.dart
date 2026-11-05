import 'dart:async';
import 'dart:convert';
import 'dart:io';

/// Keeps analyzer errors fatal and rejects new Riverpod warnings while the
/// repository's existing info-level lint debt is addressed separately.
///
/// The Riverpod 3.1.7 analyzer plugin currently reports these five warnings
/// despite the narrowly scoped ignore_for_file annotations at their sources.
/// Do not add entries without investigating the underlying warning.
const _knownWarnings = <String, int>{
  'lib/main.dart|missing_provider_scope': 3,
  'lib/src/features/ai_dictation/application/ai_dictation_providers.dart|unsupported_provider_value':
      1,
  'lib/src/features/workbench/application/workbench_providers.dart|unsupported_provider_value':
      1,
};

final _ansiEscape = RegExp(r'\x1B\[[0-9;]*[A-Za-z]');
final _warning = RegExp(
  r'^warning\s+•\s+.+\s+•\s+(.+):\d+:\d+\s+•\s+([a-zA-Z0-9_]+)\s*$',
);

String? _warningKey(String line) {
  final normalized = line.replaceAll(_ansiEscape, '').trim();
  if (!normalized.startsWith('warning ')) return null;
  final match = _warning.firstMatch(normalized);
  if (match == null) return '!unparsed: ' + normalized;
  return match.group(1)!.replaceAll('\\', '/') + '|' + match.group(2)!;
}

List<String> _unexpectedWarnings(Iterable<String> output) {
  final seen = <String, int>{};
  final failures = <String>[];
  for (final line in output) {
    final key = _warningKey(line);
    if (key == null) continue;
    final count = (seen[key] ?? 0) + 1;
    seen[key] = count;
    if (count > (_knownWarnings[key] ?? 0)) failures.add(key);
  }
  return failures;
}

void _selfTest() {
  const known = <String>[
    'warning • Missing root scope • lib/main.dart:95:5 • missing_provider_scope',
    'warning • Missing root scope • lib/main.dart:199:5 • missing_provider_scope',
    'warning • Missing root scope • lib/main.dart:260:5 • missing_provider_scope',
    'warning • Unsupported service • lib/src/features/ai_dictation/application/ai_dictation_providers.dart:47:20 • unsupported_provider_value',
    'warning • Unsupported service • lib/src/features/workbench/application/workbench_providers.dart:261:23 • unsupported_provider_value',
  ];
  if (_unexpectedWarnings(known).isNotEmpty ||
      _unexpectedWarnings(known.sublist(0, 2)).isNotEmpty ||
      _unexpectedWarnings([...known, known.first]).length != 1 ||
      _unexpectedWarnings([
            ...known,
            'warning • New warning • lib/new.dart:1:1 • bad_lint',
          ]).length !=
          1 ||
      _unexpectedWarnings(['warning format changed']).length != 1 ||
      _unexpectedWarnings(['info • Advisory • lib/new.dart:2:1 • info_lint'])
          .isNotEmpty) {
    stderr.writeln('PR analyzer warning ratchet self-test failed');
    exitCode = 1;
    return;
  }
  stdout.writeln('PR analyzer warning ratchet self-test passed');
}

Future<void> main(List<String> args) async {
  if (args.length == 1 && args.single == '--self-test') {
    _selfTest();
    return;
  }
  if (args.isNotEmpty) {
    stderr.writeln('Usage: dart tool/quality/pr_analyzer.dart [--self-test]');
    exitCode = 2;
    return;
  }

  final lines = <String>[];
  late final Process child;
  try {
    child = await Process.start(
      Platform.isWindows ? 'flutter.bat' : 'flutter',
      ['analyze', '--no-fatal-infos', '--no-fatal-warnings'],
      runInShell: Platform.isWindows,
    );
  } on ProcessException catch (error) {
    stderr.writeln('Unable to run Flutter analyzer: ' + error.toString());
    exitCode = 1;
    return;
  }

  Future<void> collect(Stream<List<int>> bytes, IOSink output) async {
    await for (final line
        in bytes.transform(utf8.decoder).transform(const LineSplitter())) {
      output.writeln(line);
      lines.add(line);
    }
  }

  final copyOutput = Future.wait<void>([
    collect(child.stdout, stdout),
    collect(child.stderr, stderr),
  ]);
  final result = await child.exitCode;
  await copyOutput;
  if (result != 0) {
    stderr.writeln(
      'Flutter analyzer failed with exit code ' + result.toString(),
    );
    exitCode = result;
    return;
  }

  final newWarnings = _unexpectedWarnings(lines);
  if (newWarnings.isNotEmpty) {
    stderr.writeln(
      'New or increased analyzer warnings (not in narrow baseline):',
    );
    for (final warning in newWarnings) {
      stderr.writeln('  ' + warning);
    }
    exitCode = 1;
    return;
  }
  stdout.writeln(
    'PR analyzer passed (errors fatal; new warnings rejected; info advisory).',
  );
}
