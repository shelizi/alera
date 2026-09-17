import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'CodeForge controller owns one revisioned native document edit stream',
    () {
      final controller = File(
        'third_party/code_forge/lib/code_forge/controller.dart',
      ).readAsStringSync();

      expect(controller, contains('NativeEditorDocument.open('));
      expect(controller, contains('.applyEdits('));
      expect(controller, contains('queryNativeSyntaxSpans('));
      expect(controller, contains('.close()'));
      expect(controller, contains('_commitDocumentEdit('));
      expect(controller, isNot(contains('_currentVersion++;')));
    },
  );

  test('CodeForge production surface enables viewport native highlighting', () {
    final codeArea = File(
      'third_party/code_forge/lib/code_forge/code_area.dart',
    ).readAsStringSync();
    final syntaxHighlighter = File(
      'third_party/code_forge/lib/code_forge/syntax_highlighter.dart',
    ).readAsStringSync();
    final workspaceEditor = File(
      'lib/src/features/workbench/presentation/workspace_editor_surface.dart',
    ).readAsStringSync();

    expect(codeArea, contains('controller.configureNativeSyntaxDocument('));
    expect(codeArea, contains('controller.queryNativeSyntaxSpans'));
    expect(syntaxHighlighter, contains('SyntaxSpanResponse'));
    expect(syntaxHighlighter, contains('_nativeSpanCacheRevision'));
    expect(codeArea, contains('.preHighlightLines('));
    expect(workspaceEditor, contains('languageId:'));
    expect(workspaceEditor, contains('_languageIdForPath(filePath)'));
  });

  test('CodeForge structural selection stays on the retained native tree', () {
    final controller = File(
      'third_party/code_forge/lib/code_forge/controller.dart',
    ).readAsStringSync();
    final codeArea = File(
      'third_party/code_forge/lib/code_forge/code_area.dart',
    ).readAsStringSync();
    final shortcuts = File('third_party/code_forge/lib/code_forge/utils.dart')
        .readAsStringSync();

    expect(controller, contains('queryNativeStructuralSelection('));
    expect(controller, contains('document.queryStructuralSelection('));
    expect(controller, contains('_pendingNativeEditorEdits.isNotEmpty'));
    expect(controller, contains('expandStructuralSelection()'));
    expect(controller, contains('shrinkStructuralSelection()'));
    expect(codeArea, contains('.expandStructuralSelection'));
    expect(codeArea, contains('.shrinkStructuralSelection'));
    expect(shortcuts, contains('expandStructuralSelection'));
    expect(shortcuts, contains('shrinkStructuralSelection'));
  });
}
