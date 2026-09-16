import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/presentation/workspace_editor_surface.dart';
import 'package:alera/src/features/settings/domain/editor_syntax_theme_catalog.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

void main() {
  test('normalizes editor tab size to the supported range', () {
    expect(normalizeWorkspaceEditorTabSize(4), 4);
    expect(normalizeWorkspaceEditorTabSize(0), 1);
    expect(normalizeWorkspaceEditorTabSize(12), 8);
  });

  testWidgets('suppresses stale focus callbacks during editor teardown', (
    tester,
  ) async {
    final focusNode = WorkspaceEditorFocusNode();
    addTearDown(focusNode.dispose);
    var calls = 0;
    focusNode.addListener(() {
      calls += 1;
    });

    await tester.pumpWidget(
      Focus(focusNode: focusNode, child: const SizedBox()),
    );
    focusNode.requestFocus();
    await tester.pump();

    expect(calls, greaterThan(0));
    final callsBeforeSuppression = calls;

    focusNode.suppressThirdPartyListeners();
    focusNode.unfocus();
    await tester.pump();

    expect(calls, callsBeforeSuppression);
  });

  testWidgets('removes wrapped focus listeners by their original callback', (
    tester,
  ) async {
    final focusNode = WorkspaceEditorFocusNode();
    addTearDown(focusNode.dispose);
    var calls = 0;
    void listener() {
      calls += 1;
    }

    focusNode.addListener(listener);
    focusNode.removeListener(listener);

    await tester.pumpWidget(
      Focus(focusNode: focusNode, child: const SizedBox()),
    );
    focusNode.requestFocus();
    await tester.pump();

    expect(calls, 0);
  });

  test('uses workspace-relative paths in the editor file bar', () {
    final workspace = _workspace();

    expect(
      workspaceEditorDisplayPath(
        workspace: workspace,
        filePath: 'src/main.dart',
      ),
      'src/main.dart',
    );
    expect(
      workspaceEditorDisplayPath(
        workspace: workspace,
        filePath: '/repo/alera/src/main.dart',
      ),
      p.join('src', 'main.dart'),
    );
  });

  test('includes syntax theme in the CodeForge widget key', () {
    final aleraKey = workspaceEditorCodeForgeKey(
      tabId: 'tab-1',
      filePath: 'lib/main.dart',
      themeName: EditorSyntaxThemeNames.alera,
    );
    final monokaiKey = workspaceEditorCodeForgeKey(
      tabId: 'tab-1',
      filePath: 'lib/main.dart',
      themeName: EditorSyntaxThemeNames.monokai,
    );

    expect(aleraKey, isNot(monokaiKey));
  });

  test('shows an interactive vertical editor scrollbar', () {
    final decoration = workspaceEditorScrollbarDecoration();

    expect(decoration.thickness, greaterThan(0));
    expect(decoration.thumbColor, isNot(Colors.transparent));
    expect(decoration.interactive, isTrue);
    expect(decoration.trackVisibility, isFalse);
  });

  test('keeps wrapping and guides for ordinary source files', () {
    final profile = workspaceEditorPerformanceProfile(
      lineCount: workspaceEditorLargeFileLineThreshold - 1,
      contentLength: workspaceEditorLargeFileCharacterThreshold - 1,
    );

    expect(profile.lineWrap, isTrue);
    expect(profile.guideLines, isTrue);
    expect(profile.syntaxHighlighting, isTrue);
  });

  test('disables wrapping and guides for long files', () {
    final profile = workspaceEditorPerformanceProfile(
      lineCount: workspaceEditorLargeFileLineThreshold,
      contentLength: 1,
    );

    expect(profile.lineWrap, isFalse);
    expect(profile.guideLines, isFalse);
    expect(profile.syntaxHighlighting, isFalse);
  });

  test('uses large-file mode for a huge single-line file', () {
    final profile = workspaceEditorPerformanceProfile(
      lineCount: 1,
      contentLength: workspaceEditorLargeFileCharacterThreshold,
    );

    expect(profile.lineWrap, isFalse);
    expect(profile.guideLines, isFalse);
    expect(profile.syntaxHighlighting, isFalse);
  });

  test('skips surface refresh when editor-visible state stays stable', () {
    final profile = workspaceEditorPerformanceProfile(
      lineCount: 10,
      contentLength: 100,
    );

    expect(
      workspaceEditorShouldRefreshSurface(
        wasDirty: true,
        isDirty: true,
        previousProfile: profile,
        currentProfile: profile,
      ),
      isFalse,
    );
  });

  test('refreshes surface when dirty state or performance profile changes', () {
    final ordinary = workspaceEditorPerformanceProfile(
      lineCount: 10,
      contentLength: 100,
    );
    final large = workspaceEditorPerformanceProfile(
      lineCount: workspaceEditorLargeFileLineThreshold,
      contentLength: 100,
    );

    expect(
      workspaceEditorShouldRefreshSurface(
        wasDirty: false,
        isDirty: true,
        previousProfile: ordinary,
        currentProfile: ordinary,
      ),
      isTrue,
    );
    expect(
      workspaceEditorShouldRefreshSurface(
        wasDirty: true,
        isDirty: true,
        previousProfile: ordinary,
        currentProfile: large,
      ),
      isTrue,
    );
  });

  test('defers document snapshots only for large files', () {
    expect(
      workspaceEditorShouldDeferDocumentSnapshot(
        lineCount: workspaceEditorLargeFileLineThreshold - 1,
        contentLength: workspaceEditorLargeFileCharacterThreshold - 1,
      ),
      isFalse,
    );
    expect(
      workspaceEditorShouldDeferDocumentSnapshot(
        lineCount: workspaceEditorLargeFileLineThreshold,
        contentLength: 1,
      ),
      isTrue,
    );
    expect(
      workspaceEditorShouldDeferDocumentSnapshot(
        lineCount: 1,
        contentLength: workspaceEditorLargeFileCharacterThreshold,
      ),
      isTrue,
    );
  });

  test('syncs controller text only when the document version changes', () {
    expect(
      workspaceEditorShouldSyncControllerText(
        previousDocumentVersion: 7,
        currentDocumentVersion: 7,
      ),
      isFalse,
    );
    expect(
      workspaceEditorShouldSyncControllerText(
        previousDocumentVersion: 7,
        currentDocumentVersion: 8,
      ),
      isTrue,
    );
  });

  test('offers Text Actions only for a valid editor selection', () {
    expect(
      workspaceEditorHasTextActionSelection(
        text: 'Selected text',
        selection: const TextSelection(baseOffset: 0, extentOffset: 8),
      ),
      isTrue,
    );
    expect(
      workspaceEditorHasTextActionSelection(
        text: 'Selected text',
        selection: const .collapsed(offset: 4),
      ),
      isFalse,
    );
    expect(
      workspaceEditorHasTextActionSelection(
        text: 'Selected text',
        selection: const TextSelection(baseOffset: 0, extentOffset: 20),
      ),
      isFalse,
    );
  });
}

Workspace _workspace() {
  final now = DateTime(2026, 6, 6);
  return Workspace(
    id: 'ws-1',
    projectId: 'project-1',
    name: 'alera',
    path: '/repo/alera',
    createdAt: now,
    updatedAt: now,
    kind: .main,
    status: .active,
  );
}
