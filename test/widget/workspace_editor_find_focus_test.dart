import 'package:code_forge/code_forge.dart' as code_forge;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('repeated Ctrl+F refocuses and selects the existing find query', (
    tester,
  ) async {
    final findController = TextEditingController(text: 'needle');
    final findFocusNode = FocusNode();
    final editorFocusNode = FocusNode();
    final findVisible = ValueNotifier<bool>(true);
    addTearDown(() {
      findController.dispose();
      findFocusNode.dispose();
      editorFocusNode.dispose();
      findVisible.dispose();
    });

    void refocusFind() => code_forge.codeForgeRefocusFindInput(
      focusNode: findFocusNode,
      controller: findController,
      isActive: () => findVisible.value,
    );

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: CallbackShortcuts(
            bindings: <ShortcutActivator, VoidCallback>{
              const SingleActivator(LogicalKeyboardKey.keyF, control: true):
                  refocusFind,
            },
            child: ValueListenableBuilder<bool>(
              valueListenable: findVisible,
              builder: (context, visible, _) => Column(
                children: <Widget>[
                  if (visible)
                    TextField(
                      controller: findController,
                      focusNode: findFocusNode,
                    ),
                  TextField(focusNode: editorFocusNode),
                ],
              ),
            ),
          ),
        ),
      ),
    );

    editorFocusNode.requestFocus();
    await tester.pump();
    expect(editorFocusNode.hasPrimaryFocus, isTrue);

    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyF);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pump();
    expect(findFocusNode.hasPrimaryFocus, isTrue);
    expect(
      findController.selection,
      const TextSelection(baseOffset: 0, extentOffset: 6),
    );

    // Re-focus without destroying or clearing the existing search text.
    findController.selection = const TextSelection.collapsed(offset: 3);
    editorFocusNode.requestFocus();
    await tester.pump();
    expect(editorFocusNode.hasPrimaryFocus, isTrue);
    refocusFind();
    await tester.pump();
    expect(findFocusNode.hasPrimaryFocus, isTrue);
    expect(findController.text, 'needle');
    expect(
      findController.selection,
      const TextSelection(baseOffset: 0, extentOffset: 6),
    );

    // A pending callback cannot reopen a closed search bar or steal focus.
    editorFocusNode.requestFocus();
    refocusFind();
    findVisible.value = false;
    await tester.pump();
    expect(editorFocusNode.hasPrimaryFocus, isTrue);
    expect(findVisible.value, isFalse);
  });
}
