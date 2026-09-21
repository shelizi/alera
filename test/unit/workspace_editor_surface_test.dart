import 'package:alera/src/features/language_intelligence/domain/language_id.dart';
import 'package:alera/src/features/language_intelligence/domain/language_intelligence_settings.dart';
import 'package:alera/src/features/language_intelligence/domain/source_location.dart';
import 'package:alera/src/features/language_intelligence/infra/builtin_language_extensions.dart';
import 'package:alera/src/features/workbench/application/workspace_file_service.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_source_control_scope.dart';
import 'package:alera/src/features/workbench/presentation/workspace_editor_surface.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

void main() {
  test('session registration does not consume pending reveal', () {
    final registry = EditorSessionRegistry();
    const target = WorkspaceEditorRevealTarget(
      line: 2,
      column: 3,
      matchLength: 4,
    );

    registry.reveal('tab-b', target);
    registry.register(
      'tab-b',
      EditorSessionHandle(
        isDirty: () => false,
        save: () async {},
        discard: () async {},
      ),
    );

    expect(registry.takePendingReveal('tab-b'), target);
    expect(registry.takePendingReveal('tab-b'), isNull);
  });

  test('native parser policy preserves legacy parsers and opts new grammars in explicitly', () {
    final registry = createBuiltinLanguageExtensionRegistry();
    final defaults = LanguageIntelligenceSettings.defaults;

    expect(
      workspaceEditorNativeSyntaxEnabled(
        filePath: 'lib/main.rs',
        registry: registry,
        settings: defaults,
      ),
      isTrue,
    );
    expect(
      workspaceEditorNativeSyntaxEnabled(
        filePath: 'src/main.py',
        registry: registry,
        settings: defaults,
      ),
      isTrue,
    );
    expect(
      workspaceEditorNativeSyntaxEnabled(
        filePath: 'src/main.cs',
        registry: registry,
        settings: defaults,
      ),
      isFalse,
    );
    expect(
      workspaceEditorNativeSyntaxEnabled(
        filePath: 'lib/main.dart',
        registry: registry,
        settings: defaults,
      ),
      isTrue,
      reason: 'Dart keeps its existing native parser enabled by descriptor',
    );
    expect(
      workspaceEditorNativeSyntaxEnabled(
        filePath: 'config/app.json',
        registry: registry,
        settings: defaults,
      ),
      isTrue,
      reason: 'non-catalog legacy native languages preserve the old policy',
    );

    final overrides = defaults
        .withLanguage(
          LanguageId('rust'),
          const LanguageActivationSettings(structuralParserEnabled: false),
        )
        .withLanguage(
          LanguageId('csharp'),
          const LanguageActivationSettings(structuralParserEnabled: true),
        );
    expect(
      workspaceEditorNativeSyntaxEnabled(
        filePath: 'lib/main.rs',
        registry: registry,
        settings: overrides,
      ),
      isFalse,
    );
    expect(
      workspaceEditorNativeSyntaxEnabled(
        filePath: 'src/main.cs',
        registry: registry,
        settings: overrides,
      ),
      isTrue,
    );
  });

  test('editor syntax ids use extension descriptors before the legacy map', () {
    final registry = createBuiltinLanguageExtensionRegistry();
    final cases = <String, String>{
      'script.csx': 'csharp',
      'stub.pyi': 'python',
      'template.phtml': 'php',
      'module.mts': 'typescript',
      'module.cts': 'typescript',
      'component.tsx': 'tsx',
      'component.jsx': 'jsx',
      'legacy.dart': 'dart',
    };

    for (final entry in cases.entries) {
      expect(
        workspaceEditorSyntaxLanguageIdForPath(
          filePath: entry.key,
          registry: registry,
        ),
        entry.value,
        reason: entry.key,
      );
    }
  });

  test('semantic source text preserves unchanged raw tabs', () {
    expect(
      workspaceEditorEncodeSemanticSourceText(
        currentDisplayText: '    alpha\n    beta\n',
        originalRawText: '\talpha\n\tbeta\n',
        originalDisplayText: '    alpha\n    beta\n',
      ),
      '\talpha\n\tbeta\n',
    );
  });

  test('semantic source text preserves raw tabs around changed lines', () {
    expect(
      workspaceEditorEncodeSemanticSourceText(
        currentDisplayText: '    alpha\n    beta changed\n    gamma\n',
        originalRawText: '\talpha\n\tbeta\n\tgamma\n',
        originalDisplayText: '    alpha\n    beta\n    gamma\n',
      ),
      '\talpha\n    beta changed\n\tgamma\n',
    );
  });

  test('semantic source text falls back when original snapshots disagree', () {
    const current = '    alpha\nchanged\n';
    expect(
      workspaceEditorEncodeSemanticSourceText(
        currentDisplayText: current,
        originalRawText: '\talpha\n',
        originalDisplayText: '    alpha\nother\n',
      ),
      current,
    );
  });

  test(
    'definition cursor maps expanded tabs back to source scalar columns',
    () {
      expect(
        workspaceEditorSourcePositionForDisplayOffset(
          displayText: '    foo\nbar',
          sourceText: '\tfoo\nbar',
          displayScalarOffset: 4,
          tabSize: 4,
        ),
        const SourcePosition(line: 0, scalarColumn: 1),
      );
      expect(
        workspaceEditorSourcePositionForDisplayOffset(
          displayText: '    foo\n🙂bar',
          sourceText: '\tfoo\n🙂bar',
          displayScalarOffset: 9,
          tabSize: 4,
        ),
        const SourcePosition(line: 1, scalarColumn: 1),
      );
    },
  );

  test('definition cursor clamps positions inside expanded tab whitespace', () {
    expect(
      workspaceEditorSourcePositionForDisplayOffset(
        displayText: '    foo',
        sourceText: '\tfoo',
        displayScalarOffset: 2,
        tabSize: 4,
      ),
      const SourcePosition(line: 0, scalarColumn: 0),
    );
  });

  test('source location becomes a workspace editor reveal target', () {
    final workspacePath = p.join('C:', 'repo', 'alera');
    final target = workspaceEditorNavigationTargetForLocation(
      workspaceId: 'ws-1',
      workspacePath: workspacePath,
      location: SourceLocation(
        workspaceId: 'ws-1',
        path: p.join(workspacePath, 'lib', 'target.rs'),
        range: const SourceRange(
          start: SourcePosition(line: 4, scalarColumn: 2),
          end: SourcePosition(line: 4, scalarColumn: 8),
        ),
      ),
    );

    expect(target, isNotNull);
    expect(target!.relativePath, p.join('lib', 'target.rs'));
    expect(target.reveal.line, 5);
    expect(target.reveal.column, 3);
    expect(target.reveal.matchLength, 6);
  });

  test('source navigation rejects targets outside the current workspace', () {
    final workspacePath = p.join('C:', 'repo', 'alera');
    expect(
      workspaceEditorNavigationTargetForLocation(
        workspaceId: 'ws-1',
        workspacePath: workspacePath,
        location: SourceLocation(
          workspaceId: 'ws-1',
          path: p.join('C:', 'sdk', 'library.rs'),
          range: const SourceRange(
            start: SourcePosition(line: 0, scalarColumn: 0),
            end: SourcePosition(line: 0, scalarColumn: 1),
          ),
        ),
      ),
      isNull,
    );
  });

  test('reveal range maps raw tab columns to expanded editor columns', () {
    final range = workspaceEditorDisplayRevealRange(
      rawText: '\tfoo\n',
      lineNumber: 1,
      rawColumn: 2,
      rawMatchLength: 3,
      tabSize: 4,
    );

    expect(range.columnIndex, 4);
    expect(range.matchLength, 3);
  });

  test('reveal range uses UTF-16 offsets after non-BMP characters', () {
    final range = workspaceEditorDisplayRevealRange(
      rawText: '🙂needle\n',
      lineNumber: 1,
      rawColumn: 2,
      rawMatchLength: 6,
      tabSize: 4,
    );

    expect(range.columnIndex, 2);
    expect(range.matchLength, 6);
  });

  test('reveal range includes non-BMP characters inside the match', () {
    final range = workspaceEditorDisplayRevealRange(
      rawText: '🙂ne🙂edle\n',
      lineNumber: 1,
      rawColumn: 2,
      rawMatchLength: 7,
      tabSize: 4,
    );

    expect(range.columnIndex, 2);
    expect(range.matchLength, 8);
  });

  test('load request matcher rejects stale editor reads', () {
    expect(
      workspaceEditorLoadRequestMatches(
        requestId: 2,
        currentRequestId: 2,
        workspacePath: '/repo/alera',
        activeWorkspacePath: '/repo/alera',
        filePath: 'lib/main.dart',
        activeFilePath: 'lib/main.dart',
      ),
      isTrue,
    );
    expect(
      workspaceEditorLoadRequestMatches(
        requestId: 1,
        currentRequestId: 2,
        workspacePath: '/repo/alera',
        activeWorkspacePath: '/repo/alera',
        filePath: 'lib/main.dart',
        activeFilePath: 'lib/main.dart',
      ),
      isFalse,
    );
    expect(
      workspaceEditorLoadRequestMatches(
        requestId: 2,
        currentRequestId: 2,
        workspacePath: '/repo/alera',
        activeWorkspacePath: '/repo/other',
        filePath: 'lib/main.dart',
        activeFilePath: 'lib/main.dart',
      ),
      isFalse,
    );
    expect(
      workspaceEditorLoadRequestMatches(
        requestId: 2,
        currentRequestId: 2,
        workspacePath: '/repo/alera',
        activeWorkspacePath: '/repo/alera',
        filePath: 'lib/main.dart',
        activeFilePath: 'lib/other.dart',
      ),
      isFalse,
    );
  });

  test('editor diff target uses focused source control root', () {
    final workspace = _workspace();

    final target = workspaceEditorDiffTargetForFile(
      workspace: workspace,
      filePath: 'packages/app/lib/main.dart',
      sourceControlScope: WorkspaceSourceControlScope(
        workspaceId: workspace.id,
        workspacePath: workspace.path,
        path: '/repo/alera/packages/app',
        relativeRoot: 'packages/app',
      ),
    );

    expect(target, isNotNull);
    expect(target!.gitPath, '/repo/alera/packages/app');
    expect(target.gitFilePath, 'lib/main.dart');
    expect(target.gitDiffRoot, 'packages/app');
  });

  test('editor diff target rejects files outside focused source root', () {
    final workspace = _workspace();

    final target = workspaceEditorDiffTargetForFile(
      workspace: workspace,
      filePath: 'docs/readme.md',
      sourceControlScope: WorkspaceSourceControlScope(
        workspaceId: workspace.id,
        workspacePath: workspace.path,
        path: '/repo/alera/packages/app',
        relativeRoot: 'packages/app',
      ),
    );

    expect(target, isNull);
  });

  test(
    'editor diff target uses workspace root without focused source root',
    () {
      final workspace = _workspace();

      final target = workspaceEditorDiffTargetForFile(
        workspace: workspace,
        filePath: 'lib/main.dart',
        sourceControlScope: null,
      );

      expect(target, isNotNull);
      expect(target!.gitPath, workspace.path);
      expect(target.gitFilePath, 'lib/main.dart');
      expect(target.gitDiffRoot, isNull);
    },
  );
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
