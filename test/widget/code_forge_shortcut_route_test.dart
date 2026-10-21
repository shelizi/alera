import 'dart:io';

import 'package:alera/src/features/workbench/presentation/workspace_editor_surface.dart';
import 'package:code_forge/code_forge.dart' as code_forge;
import 'package:code_forge/src/rust/frb_generated.dart' as code_forge_rust;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show ExternalLibrary;
import 'package:flutter_test/flutter_test.dart';

void main() {
  final dll = Platform.environment['ALERA_CODE_FORGE_TEST_DLL'];
  final canRun = Platform.isWindows && dll != null && File(dll).existsSync();
  setUpAll(() async {
    if (!canRun) return;
    await code_forge_rust.RustLib.init(
      externalLibrary: ExternalLibrary.open(dll),
    );
  });
  testWidgets(
    'Ctrl+S reaches the parent save handler from focused CodeForge',
    (tester) async {
      final controller = code_forge.CodeForgeController();
      final editorFocus = FocusNode(debugLabel: 'real CodeForge editor');
      addTearDown(controller.dispose);
      addTearDown(editorFocus.dispose);
      var saveCalls = 0;
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SizedBox(
              width: 800,
              height: 480,
              child: Focus(
                canRequestFocus: false,
                onKeyEvent: (_, event) {
                  if (workspaceEditorIsSaveShortcut(
                    event,
                    controlPressed: HardwareKeyboard.instance.isControlPressed,
                    metaPressed: HardwareKeyboard.instance.isMetaPressed,
                  )) {
                    saveCalls += 1;
                    return KeyEventResult.handled;
                  }
                  return KeyEventResult.ignored;
                },
                child: code_forge.CodeForge(
                  controller: controller,
                  focusNode: editorFocus,
                  initialText: 'Test document\n',
                  autoFocus: true,
                  enableFolding: false,
                  enableGuideLines: false,
                  enableNativeSyntax: false,
                ),
              ),
            ),
          ),
        ),
      );
      await tester.pump();
      editorFocus.requestFocus();
      await tester.pump();
      expect(editorFocus.hasPrimaryFocus, isTrue);

      await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.keyS);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
      await tester.pump();
      expect(saveCalls, 1);
    },
    skip: !canRun,
  );
}
