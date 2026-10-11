import 'package:alera_mobile/src/features/workbench/application/mobile_git_patch_lines.dart';
import 'package:test/test.dart';

void main() {
  test('unified patch distinguishes hunk, file markers and added lines', () {
    const patch =
        'diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-old\n+new\n context\n';
    final kinds = classifyMobileGitPatchLines(patch)
        .map((line) => line.kind)
        .toList();
    expect(kinds, <MobileGitPatchLineKind>[
      .header,
      .header,
      .header,
      .hunk,
      .removed,
      .added,
      .context,
    ]);
    expect(classifyMobileGitPatchLines(patch)[5].text, '+new');
  });

  test(
    'paired removed and added lines highlight changed source characters',
    () {
      final lines = classifyMobileGitPatchLines(
        '@@ -1 +1 @@\n-const n = 123;\n+const n = 456;\n',
      );
      expect(lines[1].kind, MobileGitPatchLineKind.removed);
      expect(lines[2].kind, MobileGitPatchLineKind.added);
      expect(
        lines[1].text.substring(lines[1].changedStart!, lines[1].changedEnd),
        '123',
      );
      expect(
        lines[2].text.substring(lines[2].changedStart!, lines[2].changedEnd),
        '456',
      );
      expect(lines[1].text, '-const n = 123;');
      expect(lines[2].text, '+const n = 456;');
    },
  );

  test('Unicode runes never split surrogate pairs in intraline highlights', () {
    final lines = classifyMobileGitPatchLines('-print("🐱");\n+print("🐶");');
    expect(
      lines[0].text.substring(lines[0].changedStart!, lines[0].changedEnd),
      '🐱',
    );
    expect(
      lines[1].text.substring(lines[1].changedStart!, lines[1].changedEnd),
      '🐶',
    );
  });

  test(
    'unequal and oversized change blocks retain whole-line highlighting',
    () {
      final unequal = classifyMobileGitPatchLines('-a\n-b\n+c\n');
      expect(unequal.map((line) => line.changedStart), everyElement(isNull));
      final longLine = 'x' * 4200;
      final huge = classifyMobileGitPatchLines('-$longLine\n+$longLine!.');
      expect(huge.map((line) => line.changedStart), everyElement(isNull));
    },
  );

  test('file markers do not receive intraline styling and equal content is skipped', () {
    final header = classifyMobileGitPatchLines(
      '--- a/file\n+++ b/file\n-same\n+same\n',
    );
    expect(header[0].kind, MobileGitPatchLineKind.header);
    expect(header[1].kind, MobileGitPatchLineKind.header);
    expect(header[2].changedStart, isNull);
    expect(header[3].changedStart, isNull);
  });
}
