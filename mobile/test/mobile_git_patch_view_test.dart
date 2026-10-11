import 'package:alera_mobile/src/features/workbench/presentation/mobile_git_patch_view.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets(
    'revision patch keeps line markers selectable and warns on truncation',
    (tester) async {
      await tester.pumpWidget(
        const MaterialApp(
          home: Scaffold(
            body: MobileGitPatchView(
              patch: '@@ -1 +1 @@\n-before\n+after\n',
              truncated: true,
            ),
          ),
        ),
      );
      expect(find.text('Patch truncated at 512 KiB.'), findsOneWidget);
      final selectable = tester.widget<SelectableText>(
        find.byType(SelectableText),
      );
      expect(selectable.textSpan?.toPlainText(), contains('-before'));
      expect(selectable.textSpan?.toPlainText(), contains('+after'));
    },
  );

  testWidgets('empty historical patch has an informative message', (
    tester,
  ) async {
    await tester.pumpWidget(
      const MaterialApp(
        home: Scaffold(body: MobileGitPatchView(patch: '', truncated: false)),
      ),
    );
    expect(find.text('No printable patch for this file.'), findsOneWidget);
  });

  testWidgets(
    'intraline Git changes are emphasized but copied text stays exact',
    (tester) async {
      const patch = '@@ -1 +1 @@\n-const n = 123;\n+const n = 456;\n';
      await tester.pumpWidget(
        const MaterialApp(
          home: Scaffold(
            body: MobileGitPatchView(patch: patch, truncated: false),
          ),
        ),
      );
      final selectable = tester.widget<SelectableText>(
        find.byType(SelectableText),
      );
      expect(selectable.textSpan?.toPlainText(), patch);
      final rows = selectable.textSpan!.children!.cast<TextSpan>();
      final removed = rows.elementAt(1);
      final added = rows.elementAt(2);
      expect((removed.children![1] as TextSpan).text, '123');
      expect((added.children![1] as TextSpan).text, '456');
      expect(
        (removed.children![1] as TextSpan).style?.backgroundColor,
        isNotNull,
      );
      expect(
        (added.children![1] as TextSpan).style?.backgroundColor,
        isNotNull,
      );
    },
  );
}
