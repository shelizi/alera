import 'package:alera/src/features/workbench/presentation/workspace_git_diff_surface.dart';
import 'package:flutter_test/flutter_test.dart';

/// Dart projection CPU only (alignment, spacers, intraline highlights).
/// Excludes widget rebuild, paint, FFI, undo history and disk I/O.
/// flutter test --no-pub benchmark/editable_diff_projection_benchmark.dart
void main() {
  test('editable Diff projection CPU profile', () {
    for (final caseName in <String>[
      'small_edit_10000',
      'replace_300',
      'replace_600',
      'replace_middle_600',
    ]) {
      late final String baseline;
      late final String current;
      if (caseName == 'small_edit_10000') {
        final lines = [
          for (var i = 0; i < 10000; i++)
            'const item_' + i.toString() + ' = "original";',
        ];
        baseline = lines.join('\n');
        lines[5000] = 'const item_5000 = "changed";';
        current = lines.join('\n');
      } else {
        final changeCount = caseName == 'replace_300' ? 300 : 600;
        final leading = caseName == 'replace_middle_600'
            ? [for (var i = 0; i < 200; i++) 'context start ' + i.toString()]
            : <String>[];
        final trailing = caseName == 'replace_middle_600'
            ? [for (var i = 0; i < 200; i++) 'context end ' + i.toString()]
            : <String>[];
        baseline = [
          ...leading,
          for (var i = 0; i < changeCount; i++)
            'old_key_' + i.toString() + ' = 1234;',
          ...trailing,
        ].join('\n');
        current = [
          ...leading,
          for (var i = 0; i < changeCount; i++)
            'new_key_' + i.toString() + ' = 5678;',
          ...trailing,
        ].join('\n');
      }
      for (var warm = 0; warm < 2; warm++) {
        workspaceEditableDiffSnapshotForTesting(
          baseline: baseline,
          current: current,
        );
      }
      final samples = <int>[];
      for (var iteration = 0; iteration < 7; iteration++) {
        final watch = Stopwatch()..start();
        final snapshot = workspaceEditableDiffSnapshotForTesting(
          baseline: baseline,
          current: current,
        );
        watch.stop();
        expect(snapshot.added, greaterThan(0));
        expect(snapshot.oldHighlights, isNotEmpty);
        samples.add(watch.elapsedMicroseconds);
      }
      samples.sort();
      // ignore: avoid_print
      print(
        'PROFILE_EDITABLE_DIFF case=' +
            caseName +
            ' median_us=' +
            samples[3].toString() +
            ' samples_us=' +
            samples.toString() +
            ' baseline_chars=' +
            baseline.length.toString(),
      );
    }
  }, timeout: const Timeout(Duration(minutes: 2)));
}
