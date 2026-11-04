import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

import 'dart_script_test_support.dart';

void main() {
  test(
    'max-lines warns by default and enforces the baseline in strict mode',
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
        '${List.filled(510, '// fixture').join('\n')}\n',
      );

      Future<ProcessResult> check({bool strict = false}) => Process.run(
        dartScriptTestExecutable(),
        <String>[script, '--roots', 'lib', if (strict) '--strict'],
        workingDirectory: root.path,
      );

      final allowed = await check();
      expect(allowed.exitCode, 0, reason: allowed.stderr as String);
      expect(allowed.stdout, contains('max-lines ratchet ok'));

      // Growth past the baseline is reported without blocking the default CI.
      await source.writeAsString(
        '${List.filled(511, '// fixture').join('\n')}\n',
      );
      final advisory = await check();
      expect(advisory.exitCode, 0, reason: advisory.stderr as String);
      expect(advisory.stderr, contains('max-lines advisory: 1 file(s)'));
      expect(advisory.stderr, contains('grew from baseline 510'));

      // An explicit strict check still fails on baseline growth.
      final oversized = await check(strict: true);
      expect(oversized.exitCode, 1, reason: oversized.stderr as String);
      expect(oversized.stderr, contains('max-lines ratchet failed'));
      expect(oversized.stderr, contains('grew from baseline 510'));

      // New oversized files also warn by default and fail in strict mode.
      final newFile = File(p.join(root.path, 'lib', 'new_oversize.dart'));
      await newFile.writeAsString(
        '${List.filled(501, '// fixture').join('\n')}\n',
      );
      final newAdvisory = await check();
      expect(newAdvisory.exitCode, 0, reason: newAdvisory.stderr as String);
      expect(newAdvisory.stderr, contains('new oversize'));
      expect((await check(strict: true)).exitCode, 1);
    },
  );
}
