import 'dart:math' as math;

import 'package:alera/src/features/workbench/presentation/workspace_git_diff_surface.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('editable alignment includes CodeForge terminal visual rows', () {
    final newlineOnly = workspaceEditableDiffAlignmentForTesting(
      baseline: 'first\nlast\n',
      current: 'first\nlast',
    );
    expect(newlineOnly.oldSpacers, isEmpty);
    expect(newlineOnly.newSpacers, <({int afterLine, int count})>[
      (afterLine: 1, count: 1),
    ]);
    expect(newlineOnly.visualLineCount, 3);

    final insertedAndNewline = workspaceEditableDiffAlignmentForTesting(
      baseline: 'first\nlast\n',
      current: 'first\ninserted\nlast',
    );
    expect(insertedAndNewline.oldSpacers, <({int afterLine, int count})>[
      (afterLine: 0, count: 1),
    ]);
    expect(insertedAndNewline.newSpacers, <({int afterLine, int count})>[
      (afterLine: 2, count: 1),
    ]);
    expect(insertedAndNewline.visualLineCount, 4);

    final emptyBaseline = workspaceEditableDiffAlignmentForTesting(
      baseline: '',
      current: 'first\nlast',
    );
    expect(emptyBaseline.oldSpacers, <({int afterLine, int count})>[
      (afterLine: -1, count: 2),
    ]);
    expect(emptyBaseline.newSpacers, <({int afterLine, int count})>[
      (afterLine: 1, count: 1),
    ]);
    expect(emptyBaseline.visualLineCount, 3);
  });

  test(
    'editable diff memo skips repeated parent updates, invalidates options',
    () {
      final flags = workspaceEditableDiffMemoForTesting();
      expect(flags.reused, isTrue);
      expect(flags.invalidatedOnText, isTrue);
      expect(flags.invalidatedOnMode, isTrue);
    },
  );

  test(
    'editable diff computes aligned lines and UTF-16 highlights together',
    () {
      const before = 'first\nconst total = 10;\n😀 unchanged\n';
      const after = 'first\nconst total = 20;\n😀 unchanged\n';
      final result = workspaceEditableDiffSnapshotForTesting(
        baseline: before,
        current: after,
      );
      final prior = workspaceEditableDiffAlignmentForTesting(
        baseline: before,
        current: after,
      );
      expect(result.oldSpacers, prior.oldSpacers);
      expect(result.newSpacers, prior.newSpacers);
      expect(result.added, 1);
      expect(result.removed, 1);
      expect(result.visualLineCount, prior.visualLineCount);
      expect(result.oldHighlights, <({int start, int end})>[
        (start: before.indexOf('10'), end: before.indexOf('10') + 1),
      ]);
      expect(result.newHighlights, <({int start, int end})>[
        (start: after.indexOf('20'), end: after.indexOf('20') + 1),
      ]);
    },
  );

  test('editable diff honors whitespace comparison and CRLF', () {
    const before = 'first\r\n  const result = 1;\r\n😀 last\r\n';
    const after = 'first\r\nconst   result=1;\r\n😀 last\r\n';
    final normal = workspaceEditableDiffSnapshotForTesting(
      baseline: before,
      current: after,
    );
    expect(normal.added, 1);
    expect(normal.removed, 1);
    final ignore = workspaceEditableDiffSnapshotForTesting(
      baseline: before,
      current: after,
      whitespaceMode: GitDiffWhitespaceMode.ignoreAll,
    );
    expect(ignore.added, 0);
    expect(ignore.removed, 0);
    expect(ignore.oldHighlights, isEmpty);
    expect(ignore.newHighlights, isEmpty);
  });

  test('editable diff reports supplementary Unicode character offsets', () {
    const before = 'a😀b\n';
    const after = 'a😃b\n';
    final result = workspaceEditableDiffSnapshotForTesting(
      baseline: before,
      current: after,
    );
    expect(result.oldHighlights, <({int start, int end})>[(start: 1, end: 3)]);
    expect(result.newHighlights, <({int start, int end})>[(start: 1, end: 3)]);
  });

  test('large replacement preserves unchanged prefix and suffix anchors', () {
    final leading = [for (var i = 0; i < 200; i++) 'unchanged prefix $i'];
    final trailing = [for (var i = 0; i < 200; i++) 'unchanged suffix $i'];
    final before = [
      ...leading,
      for (var i = 0; i < 600; i++) 'before name $i',
      ...trailing,
    ].join('\n');
    final after = [
      ...leading,
      for (var i = 0; i < 600; i++) 'after value $i',
      ...trailing,
    ].join('\n');
    final result = workspaceEditableDiffSnapshotForTesting(
      baseline: before,
      current: after,
    );
    expect(result.added, 600);
    expect(result.removed, 600);
    expect(result.oldSpacers, isEmpty);
    expect(result.newSpacers, isEmpty);
    expect(result.visualLineCount, 1000);
    final prefixEnd = leading.join('\n').length + 1;
    final suffixStart = before.indexOf('unchanged suffix 0');
    expect(
      result.oldHighlights.every(
        (range) => range.start >= prefixEnd && range.end <= suffixStart,
      ),
      isTrue,
    );
  });

  test('shared line inside large replacement must not be marked changed', () {
    final before = [
      for (var i = 0; i < 250; i++) 'before_$i',
      'unchanged anchor',
      for (var i = 0; i < 250; i++) 'old_$i',
    ].join('\n');
    final after = [
      for (var i = 0; i < 250; i++) 'after_$i',
      'unchanged anchor',
      for (var i = 0; i < 250; i++) 'new_$i',
    ].join('\n');
    final result = workspaceEditableDiffSnapshotForTesting(
      baseline: before,
      current: after,
    );
    expect(result.added, 500);
    expect(result.removed, 500);
    final anchorStart = before.indexOf('unchanged anchor');
    final anchorEnd = anchorStart + 'unchanged anchor'.length;
    expect(
      result.oldHighlights.any(
        (range) => range.start < anchorEnd && range.end > anchorStart,
      ),
      isFalse,
    );
  });

  test('large replacements keep unique unchanged interior anchors', () {
    final before = <String>[
      for (var index = 0; index < 1200; index++) 'old prefix $index',
      '  // unique unchanged anchor',
      for (var index = 0; index < 1200; index++) 'old suffix $index',
    ].join('\n');
    final after = <String>[
      for (var index = 0; index < 1200; index++) 'new prefix $index',
      '  // unique unchanged anchor',
      for (var index = 0; index < 1200; index++) 'new suffix $index',
    ].join('\n');

    final result = workspaceEditableDiffSnapshotForTesting(
      baseline: before,
      current: after,
    );

    expect(result.added, 2400);
    expect(result.removed, 2400);
    expect(result.oldSpacers, isEmpty);
    expect(result.newSpacers, isEmpty);
    final oldAnchor = before.indexOf('unique unchanged anchor');
    final newAnchor = after.indexOf('unique unchanged anchor');
    expect(
      result.oldHighlights.any(
        (range) => range.start < oldAnchor + 23 && range.end > oldAnchor,
      ),
      isFalse,
    );
    expect(
      result.newHighlights.any(
        (range) => range.start < newAnchor + 23 && range.end > newAnchor,
      ),
      isFalse,
    );
  });

  test('large repeated unchanged runs survive high-distance replacements', () {
    // Unlike a unique comment or identifier, common closing braces and
    // blank lines occur repeatedly and cannot be used as unique anchors.
    const shared = '  }';
    final before = [
      for (var i = 0; i < 1100; i++) 'old head ' + i.toString(),
      for (var i = 0; i < 100; i++) shared,
      for (var i = 0; i < 1100; i++) 'old tail ' + i.toString(),
    ].join('\n');
    final after = [
      for (var i = 0; i < 1100; i++) 'new head ' + i.toString(),
      for (var i = 0; i < 100; i++) shared,
      for (var i = 0; i < 1100; i++) 'new tail ' + i.toString(),
    ].join('\n');

    final result = workspaceEditableDiffSnapshotForTesting(
      baseline: before,
      current: after,
    );
    expect(result.added, 2200);
    expect(result.removed, 2200);
    final firstShared = before.indexOf(shared);
    final afterShared = after.indexOf(shared);
    final oldSharedEnd = firstShared + 100 * (shared.length + 1) - 1;
    final newSharedEnd = afterShared + 100 * (shared.length + 1) - 1;
    expect(
      result.oldHighlights.any(
        (range) => range.start < oldSharedEnd && range.end > firstShared,
      ),
      isFalse,
      reason: 'Repeated but unchanged middle rows must not be replaced',
    );
    expect(
      result.newHighlights.any(
        (range) => range.start < newSharedEnd && range.end > afterShared,
      ),
      isFalse,
    );
  });

  test('unique anchors do not hide later repeated unchanged runs', () {
    // A unique header is a valid first anchor, but the large region after
    // it may contain a shared block with no individually unique line.
    const shared = '    }';
    final before = [
      'unique shared header',
      for (var i = 0; i < 1100; i++) 'old first ' + i.toString(),
      for (var i = 0; i < 70; i++) shared,
      for (var i = 0; i < 1100; i++) 'old second ' + i.toString(),
      'unique shared footer',
    ].join('\n');
    final after = [
      'unique shared header',
      for (var i = 0; i < 1100; i++) 'new first ' + i.toString(),
      for (var i = 0; i < 70; i++) shared,
      for (var i = 0; i < 1100; i++) 'new second ' + i.toString(),
      'unique shared footer',
    ].join('\n');
    final snapshot = workspaceEditableDiffSnapshotForTesting(
      baseline: before,
      current: after,
    );
    expect(snapshot.added, 2200);
    expect(snapshot.removed, 2200);
    final oldBlockStart = before.indexOf(shared);
    final newBlockStart = after.indexOf(shared);
    final blockChars = 70 * (shared.length + 1) - 1;
    expect(
      snapshot.oldHighlights.any(
        (range) =>
            range.start < oldBlockStart + blockChars &&
            range.end > oldBlockStart,
      ),
      isFalse,
      reason: 'Recursive large chunks must preserve repeated equal rows',
    );
    expect(
      snapshot.newHighlights.any(
        (range) =>
            range.start < newBlockStart + blockChars &&
            range.end > newBlockStart,
      ),
      isFalse,
    );
  });

  test('thousands of identical rows with one insertion remain aligned', () {
    const repeated = '    }';
    final before = List<String>.filled(2600, repeated).join('\n');
    final after = [
      ...List<String>.filled(1300, repeated),
      '// genuinely added line',
      ...List<String>.filled(1300, repeated),
    ].join('\n');
    final snapshot = workspaceEditableDiffSnapshotForTesting(
      baseline: before,
      current: after,
    );
    expect(snapshot.added, 1);
    expect(snapshot.removed, 0);
    expect(snapshot.visualLineCount, 2601);
    expect(snapshot.oldSpacers, <({int afterLine, int count})>[
      (afterLine: 1299, count: 1),
    ]);
    expect(snapshot.newSpacers, isEmpty);
  });

  test('seeded edits match an independent small-file LCS oracle', () {
    final random = math.Random(20261118);
    const vocabulary = [
      '',
      '}',
      '  }',
      'const value = 1;',
      'const value = 2;',
      '😀 中文',
      '\tfoo',
      'foo  bar',
      '{',
      'a',
      'a',
      'a',
    ];
    List<String> splitLines(String text) {
      if (text.isEmpty) return <String>[];
      final normalized = text.replaceAll('\r\n', '\n').replaceAll('\r', '\n');
      final lines = normalized.split('\n');
      if (normalized.endsWith('\n')) lines.removeLast();
      return lines;
    }

    int lcsLength(List<String> oldLines, List<String> newLines) {
      final matrix = List.generate(
        oldLines.length + 1,
        (_) => List<int>.filled(newLines.length + 1, 0),
      );
      for (var i = oldLines.length - 1; i >= 0; i--) {
        for (var j = newLines.length - 1; j >= 0; j--) {
          matrix[i][j] = oldLines[i] == newLines[j]
              ? 1 + matrix[i + 1][j + 1]
              : math.max(matrix[i + 1][j], matrix[i][j + 1]);
        }
      }
      return matrix[0][0];
    }

    String encode(List<String> lines) {
      final separator = switch (random.nextInt(3)) {
        0 => '\n',
        1 => '\r\n',
        _ => '\r',
      };
      if (lines.isEmpty) return '';
      return lines.join(separator) + (random.nextBool() ? separator : '');
    }

    for (var scenario = 0; scenario < 240; scenario++) {
      final old = List.generate(
        random.nextInt(32),
        (_) => vocabulary[random.nextInt(vocabulary.length)],
      );
      final current = List<String>.of(old);
      for (var edit = 0; edit < 1 + random.nextInt(15); edit++) {
        final operation = random.nextInt(3);
        final index = random.nextInt(current.length + 1);
        if (operation == 0 || current.isEmpty) {
          current.insert(index, vocabulary[random.nextInt(vocabulary.length)]);
        } else if (operation == 1) {
          current.removeAt(math.min(index, current.length - 1));
        } else {
          current[math.min(index, current.length - 1)] =
              vocabulary[random.nextInt(vocabulary.length)];
        }
      }
      final before = encode(old);
      final after = encode(current);
      final oldLines = splitLines(before);
      final newLines = splitLines(after);
      final matching = lcsLength(oldLines, newLines);
      final actual = workspaceEditableDiffSnapshotForTesting(
        baseline: before,
        current: after,
      );
      expect(
        actual.removed,
        oldLines.length - matching,
        reason: 'old-side edit distance in scenario ' + scenario.toString(),
      );
      expect(
        actual.added,
        newLines.length - matching,
        reason: 'new-side edit distance in scenario ' + scenario.toString(),
      );

      final oldRendered =
          oldLines.length +
          (before.isEmpty || before.endsWith('\n') || before.endsWith('\r')
              ? 1
              : 0);
      final newRendered =
          newLines.length +
          (after.isEmpty || after.endsWith('\n') || after.endsWith('\r')
              ? 1
              : 0);
      final projectedOld =
          oldRendered +
          actual.oldSpacers.fold<int>(0, (sum, spacer) => sum + spacer.count);
      final projectedNew =
          newRendered +
          actual.newSpacers.fold<int>(0, (sum, spacer) => sum + spacer.count);
      expect(
        projectedOld,
        actual.visualLineCount,
        reason: 'left projection length in scenario ' + scenario.toString(),
      );
      expect(
        projectedNew,
        actual.visualLineCount,
        reason: 'right projection length in scenario ' + scenario.toString(),
      );
      for (final spacer in actual.oldSpacers) {
        expect(spacer.count, greaterThan(0));
        expect(spacer.afterLine, inInclusiveRange(-1, oldLines.length - 1));
      }
      for (final spacer in actual.newSpacers) {
        expect(spacer.count, greaterThan(0));
        expect(spacer.afterLine, inInclusiveRange(-1, newLines.length - 1));
      }
    }
  });

  test('unrelated replaced lines still report actual changes', () {
    final before = [for (var i = 0; i < 600; i++) 'old_key_$i = 1234;']
        .join('\n');
    final after = [for (var i = 0; i < 600; i++) 'new_key_$i = 5678;']
        .join('\n');
    final result = workspaceEditableDiffSnapshotForTesting(
      baseline: before,
      current: after,
    );
    expect(result.added, 600);
    expect(result.removed, 600);
    expect(result.oldSpacers, isEmpty);
    expect(result.newSpacers, isEmpty);
    expect(result.oldHighlights, isNotEmpty);
    expect(result.newHighlights, isNotEmpty);
  });
}
