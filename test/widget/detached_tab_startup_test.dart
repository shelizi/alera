import 'package:alera/src/features/workbench/presentation/detached_tab_child_app.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('only editor and diff tabs need the CodeForge native engine', () {
    expect(detachedTabRequiresCodeForge(WorkspaceTabKind.editor), isTrue);
    expect(detachedTabRequiresCodeForge(WorkspaceTabKind.gitDiff), isTrue);
    for (final kind in [
      WorkspaceTabKind.terminal,
      WorkspaceTabKind.markdownViewer,
      WorkspaceTabKind.pdf,
      WorkspaceTabKind.gitHistory,
    ]) {
      expect(detachedTabRequiresCodeForge(kind), isFalse);
    }
  });

  testWidgets('new detached window shows immediate opening feedback', (
    tester,
  ) async {
    await tester.pumpWidget(const DetachedTabStartupApp(title: 'notes.md'));

    expect(find.text('Opening tab...'), findsOneWidget);
    expect(find.text('notes.md'), findsOneWidget);
    expect(find.byType(CircularProgressIndicator), findsOneWidget);
  });

  testWidgets('failed startup shows a close action rather than hanging', (
    tester,
  ) async {
    await tester.pumpWidget(
      const DetachedTabStartupApp(title: 'notes.md', failed: true),
    );

    expect(find.text('Could not open the separate window'), findsOneWidget);
    expect(find.text('Close window'), findsOneWidget);
    expect(find.byType(CircularProgressIndicator), findsNothing);
  });
}
