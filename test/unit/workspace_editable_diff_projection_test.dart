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
