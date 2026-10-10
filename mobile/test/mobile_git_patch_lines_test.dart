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
}
