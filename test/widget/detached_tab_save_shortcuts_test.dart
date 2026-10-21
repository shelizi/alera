import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/presentation/detached_tab_child_app.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  Future<void> pumpFocusedTab(
    WidgetTester tester, {
    required WorkspaceTabKind kind,
    required VoidCallback onSave,
  }) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: DetachedTabSaveShortcuts(
            kind: kind,
            onSave: onSave,
            child: const TextField(autofocus: true),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(FocusManager.instance.primaryFocus, isNotNull);
  }

  testWidgets('Ctrl+S saves a detached editor while text input has focus', (
    tester,
  ) async {
    var saves = 0;
    await pumpFocusedTab(
      tester,
      kind: WorkspaceTabKind.editor,
      onSave: () => saves += 1,
    );
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyS);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    expect(saves, 1);
  });

  testWidgets('Meta+S also saves a detached editor', (tester) async {
    var saves = 0;
    await pumpFocusedTab(
      tester,
      kind: WorkspaceTabKind.editor,
      onSave: () => saves += 1,
    );
    await tester.sendKeyDownEvent(LogicalKeyboardKey.metaLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyS);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.metaLeft);
    expect(saves, 1);
  });

  testWidgets('Ctrl+S in a detached terminal does not invoke editor save', (
    tester,
  ) async {
    var saves = 0;
    await pumpFocusedTab(
      tester,
      kind: WorkspaceTabKind.terminal,
      onSave: () => saves += 1,
    );
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyS);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    expect(saves, 0);
  });
}
