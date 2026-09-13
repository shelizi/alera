import 'package:alera/src/features/settings/application/settings_controller.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:alera/src/features/workbench/application/source_control_watcher.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_source_control_scope.dart';
import 'package:alera/src/features/workbench/presentation/workspace_git_diff_panel.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_providers.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import '../unit/fake_git_backend.dart';
import '../unit/fake_source_control_watcher.dart';

void main() {
  testWidgets('panel shares the complete commit menu and its delegates', (
    tester,
  ) async {
    final item = GitHistoryItem(
      id: 'panel123456',
      parentIds: const <String>['parent123'],
      subject: 'Panel Commit',
      message: 'Panel Commit',
    );
    final backend = FakeGitBackend()
      ..gitHistoryResult = GitHistoryResult(
        items: <GitHistoryItem>[item],
        hasIncomingChanges: false,
        hasOutgoingChanges: false,
        hasMore: false,
        limit: 50,
        currentRef: const GitHistoryItemRef(
          id: 'refs/heads/main',
          name: 'main',
          revision: 'panel123456',
          category: GitHistoryRefCategory.branches,
        ),
      );

    await _pumpPanel(tester, backend);
    await tester.tap(find.text('COMMITS'));
    await tester.pumpAndSettle();
    await tester.tap(find.text(item.subject), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();

    for (final label in <String>[
      'Add Tag…',
      'Create Branch Here…',
      'Cherry Pick',
      'Drop Commit',
      'Merge Into Current Branch',
      'Rebase Current Branch Onto This Commit',
      'Create Archive…',
    ]) {
      expect(find.text(label), findsOneWidget, reason: label);
    }

    await tester.tap(find.text('Cherry Pick'));
    await tester.pumpAndSettle();

    final call = backend.calls.lastWhere(
      (candidate) => candidate.method == 'cherryPickCommit',
    );
    expect(call.args['path'], '/tmp/project');
    expect(call.args['commitId'], item.id);
  });
}

Future<void> _pumpPanel(WidgetTester tester, FakeGitBackend backend) {
  final now = DateTime.utc(2026, 6, 6);
  final workspace = Workspace(
    id: 'workspace-1',
    projectId: 'project-1',
    name: 'Main',
    path: '/tmp/project',
    createdAt: now,
    updatedAt: now,
    kind: .main,
    status: .active,
  );
  return tester.pumpWidget(
    ProviderScope(
      overrides: [
        gitBackendProvider.overrideWithValue(backend),
        sourceControlWatcherProvider.overrideWithValue(
          FakeSourceControlWatcher(),
        ),
        settingsControllerProvider.overrideWith(
          () => _PanelSettingsController(.defaults),
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
  _PanelSettingsController(this._settings);

  final AleraSettings _settings;

  @override
  AleraSettings build() => _settings;
}
