import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'CodeForge controller owns one revisioned native document edit stream',
    () {
      final controller = File(
        'third_party/code_forge/lib/code_forge/controller.dart',
      ).readAsStringSync();

      expect(controller, contains('Rope.openWorkspaceFile('));
      expect(
        controller,
        contains('NativeEditorDocument.openFromRopeCancellable('),
      );
      expect(controller, contains('NativeParseCancellation.create()'));
      expect(controller, contains('previousCancellation?.cancel()'));
      expect(controller, contains('parseCancellation?.cancel()'));
      expect(controller, contains('.applyEdits('));
      expect(controller, contains('queryNativeSyntaxSpans('));
      expect(controller, contains('.close()'));
      expect(controller, contains('_commitDocumentEdit('));
      expect(controller, contains('final expectedRevision = _currentVersion;'));
      expect(controller, contains('final newRevision = expectedRevision + 1;'));
      expect(controller, contains('_currentVersion = newRevision;'));
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
    expect(codeArea, contains('final bool enableNativeSyntax;'));
    expect(codeArea, contains('this.enableNativeSyntax = false'));
    expect(codeArea, contains('if (_enableNativeSyntax &&'));
    expect(codeArea, contains('controller.queryNativeSyntaxSpans'));
    expect(syntaxHighlighter, contains('SyntaxSpanResponse'));
    expect(syntaxHighlighter, contains('_nativeSpanCacheRevision'));
    expect(codeArea, contains('.preHighlightLines('));
    expect(workspaceEditor, contains('languageId:'));
    expect(
      workspaceEditor,
      contains('workspaceEditorSyntaxLanguageIdForPath('),
    );
    expect(workspaceEditor, contains('workspaceEditorNativeSyntaxEnabled('));
    expect(workspaceEditor, contains('enableNativeSyntax: enableNativeSyntax'));
  });

  test(
    'language metadata alone does not activate the retained native parser',
    () {
      final codeArea = File(
        'third_party/code_forge/lib/code_forge/code_area.dart',
      ).readAsStringSync();
      final controller = File(
        'third_party/code_forge/lib/code_forge/controller.dart',
      ).readAsStringSync();

      expect(codeArea, contains('this.enableNativeSyntax = false'));
      expect(codeArea, contains('required bool enableNativeSyntax'));
      expect(codeArea, contains('if (_enableNativeSyntax &&'));
      expect(codeArea, contains('..enableNativeSyntax = enableNativeSyntax'));
      expect(controller, contains('void clearNativeSyntaxDocument()'));
    },
  );

  test('external editor reload keeps persistent viewport scroll state', () {
    final workspaceEditor = File(
      'lib/src/features/workbench/presentation/workspace_editor_surface.dart',
    ).readAsStringSync();
    final loading = File(
      'lib/src/features/workbench/presentation/workspace_editor_loading.dart',
    ).readAsStringSync();
    final reveal = File(
      'lib/src/features/workbench/presentation/workspace_editor_reveal.dart',
    ).readAsStringSync();

    expect(workspaceEditor, contains('_verticalScrollController'));
    expect(workspaceEditor, contains('_horizontalScrollController'));
    expect(
      workspaceEditor,
      contains('verticalScrollController: _verticalScrollController'),
    );
    expect(
      workspaceEditor,
      contains('horizontalScrollController: _horizontalScrollController'),
    );
    expect(reveal, contains('_captureEditorScrollPosition()'));
    expect(loading, contains('restoreScrollPosition'));
    expect(loading, contains('_scheduleEditorScrollRestore('));
  });

  test(
    'large-file retained parsing starts only after first-frame admission',
    () {
      final controller = File(
        'third_party/code_forge/lib/code_forge/controller.dart',
      ).readAsStringSync();
      final codeArea = File(
        'third_party/code_forge/lib/code_forge/code_area.dart',
      ).readAsStringSync();

      expect(
        codeArea,
        contains('deferInitialParse: _largeFilePerformanceMode'),
      );
      expect(controller, contains('bool deferInitialParse = false'));
      expect(controller, contains('SchedulerBinding.instance.endOfFrame'));
      expect(
        controller,
        contains('final initialRope = _rope.core.deepClone();'),
      );
      expect(controller, contains('await openFuture;'));
      expect(controller, contains('_pendingNativeEditorEdits.add('));
    },
  );

  test(
    'file switches invalidate retained parser work before Rope replacement',
    () {
      final controller = File(
        'third_party/code_forge/lib/code_forge/controller.dart',
      ).readAsStringSync();

      final openedFileSetter = controller.substring(
        controller.indexOf('set openedFile(String? file)'),
        controller.indexOf(
          'List<LspErrors> get diagnostics',
          controller.indexOf('set openedFile(String? file)'),
        ),
      );
      expect(openedFileSetter, contains('if (previousFile != file)'));
      expect(openedFileSetter, contains('_resetNativeSyntaxDocument();'));
      expect(
        openedFileSetter.indexOf('_resetNativeSyntaxDocument();'),
        lessThan(openedFileSetter.indexOf('_openedFile = file;')),
      );
    },
  );

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

  test(
    'editor outline prefers LSP and falls back to retained native symbols',
    () {
      final controller = File(
        'third_party/code_forge/lib/code_forge/controller.dart',
      ).readAsStringSync();
      final workspaceEditor = File(
        'lib/src/features/workbench/presentation/workspace_editor_surface.dart',
      ).readAsStringSync();
      final outline = File(
        'lib/src/features/workbench/presentation/workspace_editor_outline.dart',
      ).readAsStringSync();
      final widgets = File(
        'lib/src/features/workbench/presentation/workspace_editor_widgets.dart',
      ).readAsStringSync();

      expect(controller, contains('getDocumentSymbols(file)'));
      expect(controller, contains('document.queryDocumentSymbols('));
      expect(controller, contains('utf16ToScalarOffset('));
      expect(controller, contains('navigateToDocumentSymbol('));
      expect(
        workspaceEditor,
        contains("part 'workspace_editor_outline.dart';"),
      );
      expect(workspaceEditor, contains('_EditorOutlinePanel('));
      expect(outline, contains('_controller.queryDocumentSymbols('));
      expect(outline, contains('_workspaceEditorOutlineRefreshDebounce'));
      expect(widgets, contains('AleraIcons.outline'));
      expect(widgets, contains("'Tree-sitter'"));
    },
  );
}
