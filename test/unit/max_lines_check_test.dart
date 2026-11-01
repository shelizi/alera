import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

import 'dart_script_test_support.dart';

void main() {
  test(
    'max-lines baseline paths resolve from every desktop platform',
    () async {
      // Validate platform-independent relative baseline paths on a fresh
      // fixture, not against unrelated repository max-lines debt.
      final script = File('tool/quality/check_max_lines.dart').absolute.path;
      final root = await Directory.systemTemp.createTemp('alera-lines-');
      addTearDown(() async {
        if (root.existsSync()) {
          await root.delete(recursive: true);
        }
      });
      await File(p.join(root.path, 'pubspec.yaml'))
          .writeAsString('name: fixture\n');
      await File(p.join(root.path, 'AGENTS.md')).writeAsString('# Fixture\n');
      final baseline = File(
        p.join(root.path, 'tool', 'quality', 'max_lines_baseline.txt'),
      );
      await baseline.create(recursive: true);
      await baseline.writeAsString('510\tlib/oversize.dart\n');
      final source = File(p.join(root.path, 'lib', 'oversize.dart'));
      await source.create(recursive: true);
      await source.writeAsString(
        List.filled(510, '// fixture').join('\n') + '\n',
      );

      Future<ProcessResult> check() => Process.run(
        dartScriptTestExecutable(),
        <String>[script, '--roots', 'lib'],
        workingDirectory: root.path,
      );

      final allowed = await check();
      expect(allowed.exitCode, 0, reason: allowed.stderr as String);
      expect(allowed.stdout, contains('max-lines ratchet ok'));

      // Growth past the recorded baseline must still be rejected on every OS.
      await source.writeAsString(
        List.filled(511, '// fixture').join('\n') + '\n',
      );
      final oversized = await check();
      expect(oversized.exitCode, 1, reason: oversized.stderr as String);
      expect(oversized.stderr, contains('grew from baseline 510'));
    },
  );
}
