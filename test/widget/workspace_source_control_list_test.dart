import 'package:alera/src/features/settings/application/settings_controller.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:alera/src/features/workbench/application/source_control_watcher.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_source_control_scope.dart';
import 'package:alera/src/features/workbench/presentation/workspace_git_diff_panel.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_providers.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import '../unit/fake_git_backend.dart';
import '../unit/fake_source_control_watcher.dart';

void main() {
  testWidgets('Ctrl toggle selects separate Git files for one batch stage', (
    tester,
  ) async {
    final backend = FakeGitBackend()
      ..gitRepositoryStateResult = const GitRepositoryState(branch: 'main')
      ..gitStatusResult = const GitStatusResult(
        entries: <GitChangeEntry>[
          GitChangeEntry(path: 'a.dart', area: .unstaged, status: .modified),
          GitChangeEntry(path: 'b.dart', area: .unstaged, status: .modified),
          GitChangeEntry(path: 'c.dart', area: .untracked, status: .untracked),
        ],
      );
    await _pumpPanel(tester, backend);
    await tester.pumpAndSettle();

    await tester.tap(find.text('a.dart'));
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.tap(find.text('c.dart'));
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pump();
    expect(find.text('2 Selected'), findsOneWidget);

    await tester.tap(find.byTooltip('Stage Selected'));
    await tester.pumpAndSettle();
    expect(
      backend.calls
          .where((call) => call.method == 'stage')
          .map((call) => call.args['filePath'])
          .toList(),
      <Object?>['a.dart', 'c.dart'],
    );
    expect(find.text('2 Selected'), findsNothing);
  });

  testWidgets('Shift range selects files, discard leaves staged rows intact', (
    tester,
  ) async {
    final backend = FakeGitBackend()
      ..gitRepositoryStateResult = const GitRepositoryState(branch: 'main')
      ..gitStatusResult = const GitStatusResult(
        entries: <GitChangeEntry>[
          GitChangeEntry(path: 'a.dart', area: .staged, status: .modified),
          GitChangeEntry(path: 'b.dart', area: .unstaged, status: .modified),
          GitChangeEntry(path: 'c.dart', area: .unstaged, status: .modified),
        ],
      );
    await _pumpPanel(tester, backend);
    await tester.pumpAndSettle();

    await tester.tap(find.text('a.dart'));
    await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    await tester.tap(find.text('c.dart'));
    await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    await tester.pump();
    expect(find.text('3 Selected'), findsOneWidget);

    await tester.tap(find.byTooltip('Discard Selected'));
    await tester.pumpAndSettle();
    expect(find.textContaining('2 selected files'), findsOneWidget);
    await tester.tap(find.text('Discard').last);
    await tester.pumpAndSettle();
    expect(
      backend.calls
          .where((call) => call.method == 'discard')
          .map((call) => call.args['filePath'])
          .toList(),
      <Object?>['b.dart', 'c.dart'],
    );
  });

  testWidgets('large change lists render rows lazily', (tester) async {
    final backend = FakeGitBackend()
      ..gitRepositoryStateResult = const GitRepositoryState(branch: 'main')
      ..gitStatusResult = GitStatusResult(
        entries: <GitChangeEntry>[
          for (var index = 0; index < 2500; index += 1)
            GitChangeEntry(
              path: 'lib/file_$index.dart',
              area: .unstaged,
              status: .modified,
            ),
        ],
      );

    await _pumpPanel(tester, backend);
    await tester.pumpAndSettle();

    expect(find.text('lib/file_0.dart'), findsWidgets);
    // Rows far below the viewport must not be built eagerly: eagerly
    // materializing thousands of rows is what stalls the desktop UI when a
    // workspace reports a large change set.
    expect(find.text('lib/file_600.dart'), findsNothing);

    final position = tester
        .state<ScrollableState>(
          find.descendant(
            of: find.byType(CustomScrollView),
            matching: find.byType(Scrollable),
          ),
        )
        .position;
    while (find.text('lib/file_600.dart').evaluate().isEmpty &&
        position.pixels < position.maxScrollExtent) {
      position.jumpTo(position.pixels + position.viewportDimension);
      await tester.pump();
    }
    expect(find.text('lib/file_600.dart'), findsWidgets);
  });
}

Future<void> _pumpPanel(WidgetTester tester, FakeGitBackend backend) {
  final workspace = _workspace();
  return tester.pumpWidget(
    ProviderScope(
      overrides: [
        gitBackendProvider.overrideWithValue(backend),
        sourceControlWatcherProvider.overrideWithValue(
          FakeSourceControlWatcher(),
        ),
        settingsControllerProvider.overrideWith(
          () => _PanelSettingsController(),
        ),
      ],
      child: MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: 420,
            height: 520,
            child: WorkspaceGitDiffPanel(
              workspace: workspace,
              sourceControlScope: WorkspaceSourceControlScope(
                workspaceId: workspace.id,
                workspacePath: workspace.path,
                path: workspace.path,
              ),
              viewMode: .flat,
              onViewModeChanged: (_) {},
              groupMode: .byArea,
              onGroupModeChanged: (_) {},
              onOpenGitDiff: ({
                area,
                relativePath,
                gitDiffRoot,
                required scope,
                bool preview = false,
              }) async {},
              onOpenGitCommitDiff: ({
                relativePath,
                oldPath,
                required scope,
                gitDiffRoot,
                required commitOid,
                parentOid,
                required compareRef,
                subject,
                message,
                bool preview = false,
              }) async {},
            ),
          ),
        ),
      ),
    ),
  );
}

class _PanelSettingsController extends SettingsController {
  @override
  AleraSettings build() => AleraSettings.defaults;
}

Workspace _workspace() {
  final now = DateTime.utc(2026, 6, 6);
  return Workspace(
    id: 'workspace-1',
    projectId: 'project-1',
    name: 'Main',
    path: '/tmp/project',
    createdAt: now,
    updatedAt: now,
    kind: .main,
    status: .active,
  );
}
