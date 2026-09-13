import 'dart:async';

import 'package:alera/src/design_system/surfaces/hover_container.dart';
import 'package:alera/src/features/settings/application/settings_controller.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:alera/src/features/workbench/application/source_control_watcher.dart';
import 'package:alera/src/features/workbench/application/workbench_providers.dart';
import 'package:alera/src/features/workbench/application/workbench_repository.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_source_control_scope.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/presentation/workspace_git_diff_panel.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_providers.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import '../unit/fake_git_backend.dart';
import '../unit/fake_source_control_watcher.dart';

part 'workspace_git_history_panel_scope_cases.dart';
part 'workspace_git_history_panel_ref_menu_test.dart';

void main() {
  _registerHistoryScopeTests();
  _registerHistoryRefMenuTests();

  testWidgets('commit rows truncate ref badges instead of overflowing', (
    tester,
  ) async {
    final backend = _multiRefBackend();

    await _pumpPanel(tester, backend: backend, width: 420, height: 520);
    await tester.pumpAndSettle();
    await tester.tap(find.text('COMMITS'));
    await tester.pumpAndSettle();

    expect(find.text('Add Feature'), findsOneWidget);
    expect(find.text('feat/settings-fullscreen-modal'), findsOneWidget);
    expect(find.text('+2'), findsOneWidget);
  });

  testWidgets('narrow commit rows collapse ref badges into the hidden count', (
    tester,
  ) async {
    final backend = _multiRefBackend();

    tester.view.physicalSize = const Size(400, 1400);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);
    await _pumpPanel(tester, backend: backend, width: 200, height: 700);
    await tester.pumpAndSettle();
    await tester.tap(find.text('COMMITS'));
    await tester.pumpAndSettle();

    expect(find.text('Add Feature'), findsOneWidget);
    expect(find.text('feat/settings-fullscreen-modal'), findsNothing);
    expect(find.text('+4'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('local branch refs expose a switch action on secondary click', (
    tester,
  ) async {
    final backend = _multiRefBackend();
    final switched = <String>[];

    await _pumpPanel(
      tester,
      backend: backend,
      width: 420,
      height: 520,
      onSwitchBranch: (branch) async => switched.add(branch),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('COMMITS'));
    await tester.pumpAndSettle();

    await tester.tap(find.text('main'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    expect(find.text('Switch to Branch'), findsOneWidget);

    await tester.tap(find.text('Switch to Branch'));
    await tester.pumpAndSettle();
    expect(switched, <String>['main']);
  });

  testWidgets('current branch ref still exposes its context menu', (
    tester,
  ) async {
    final backend = _multiRefBackend();
    final switched = <String>[];

    await _pumpPanel(
      tester,
      backend: backend,
      width: 420,
      height: 520,
      onSwitchBranch: (branch) async => switched.add(branch),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('COMMITS'));
    await tester.pumpAndSettle();

    await tester.tap(
      find.text('feat/settings-fullscreen-modal'),
      buttons: kSecondaryMouseButton,
    );
    await tester.pumpAndSettle();

    expect(find.text('Current Branch'), findsOneWidget);
    expect(switched, isEmpty);
  });

  testWidgets('commits header toggles from anywhere except its actions', (
    tester,
  ) async {
    final backend = _multiRefBackend();

    await _pumpPanel(tester, backend: backend, width: 420, height: 520);
    await tester.pumpAndSettle();

    final header = tester.getRect(find.byType(HoverContainer));
    final openGraph = tester.getRect(find.byTooltip('Open Commit Graph'));

    // Empty space between the label and the header actions still expands.
    await tester.tapAt(Offset(openGraph.left - 8, header.center.dy));
    await tester.pumpAndSettle();
    expect(find.text('Add Feature'), findsOneWidget);

    // The refresh action reloads instead of collapsing the section again.
    await tester.tap(find.byTooltip('Refresh Commits'));
    await tester.pumpAndSettle();
    expect(find.text('Add Feature'), findsOneWidget);
    expect(backend.calls.where((call) => call.method == 'history').length, 2);
  });

  testWidgets('the commit graph button opens a main-area graph tab', (
    tester,
  ) async {
    final backend = _multiRefBackend();
    final repository = _FakeWorkbenchRepository();

    await _pumpPanel(
      tester,
      backend: backend,
      width: 420,
      height: 520,
      repository: repository,
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byTooltip('Open Commit Graph'));
    await tester.pumpAndSettle();

    final graphTabs = repository.tabs
        .where((tab) => tab.kind == WorkspaceTabKind.gitHistory)
        .toList();
    expect(graphTabs, hasLength(1));
    expect(graphTabs.single.title, 'Commit Graph');
    expect(graphTabs.single.gitHistoryAllBranches, isTrue);
    // The panel stays on its current-branch history scope.
    expect(
      backend.calls
          .where((call) => call.method == 'history')
          .map((call) => call.args['includeAllRefs']),
      everyElement(isNot(isTrue)),
    );
  });
}

FakeGitBackend _multiRefBackend() {
  return FakeGitBackend()
    ..gitRepositoryStateResult = const GitRepositoryState(
      branch: 'feat/settings-fullscreen-modal',
      upstream: 'origin/feat/settings-fullscreen-modal',
    )
    ..gitHistoryResult = GitHistoryResult(
      currentRef: const GitHistoryItemRef(
        id: 'refs/heads/feat/settings-fullscreen-modal',
        name: 'feat/settings-fullscreen-modal',
        revision: 'abc123456789',
      ),
      hasIncomingChanges: false,
      hasOutgoingChanges: false,
      hasMore: false,
      limit: 50,
      items: <GitHistoryItem>[
        GitHistoryItem(
          id: 'abc123456789',
          parentIds: const <String>[],
          subject: 'Add Feature',
          message: 'Add Feature',
          references: const <GitHistoryItemRef>[
            GitHistoryItemRef(
              id: 'refs/heads/feat/settings-fullscreen-modal',
              name: 'feat/settings-fullscreen-modal',
              revision: 'abc123456789',
            ),
            GitHistoryItemRef(
              id: 'refs/heads/main',
              name: 'main',
              revision: 'abc123456789',
            ),
            GitHistoryItemRef(
              id: 'refs/remotes/origin/main',
              name: 'origin/main',
              revision: 'abc123456789',
            ),
            GitHistoryItemRef(
              id: 'refs/tags/v0.14.0',
              name: 'v0.14.0',
              revision: 'abc123456789',
            ),
          ],
        ),
      ],
    );
}

Future<void> _pumpPanel(
  WidgetTester tester, {
  required FakeGitBackend backend,
  Workspace? workspace,
  WorkspaceSourceControlScope? sourceControlScope,
  FakeSourceControlWatcher? watcher,
  double width = 420,
  double height = 520,
  Future<void> Function(String branch)? onSwitchBranch,
  _FakeWorkbenchRepository? repository,
}) {
  final resolvedWorkspace = workspace ?? _workspace();
  return tester.pumpWidget(
    ProviderScope(
      overrides: [
        gitBackendProvider.overrideWithValue(backend),
        if (repository != null)
          workbenchRepositoryProvider.overrideWithValue(repository),
        sourceControlWatcherProvider.overrideWithValue(
          watcher ?? FakeSourceControlWatcher(),
        ),
        settingsControllerProvider.overrideWith(
          () => _PanelSettingsController(.defaults),
        ),
      ],
      child: MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: width,
            height: height,
            child: WorkspaceGitDiffPanel(
              workspace: resolvedWorkspace,
              sourceControlScope:
                  sourceControlScope ??
                  WorkspaceSourceControlScope(
                    workspaceId: resolvedWorkspace.id,
                    workspacePath: resolvedWorkspace.path,
                    path: resolvedWorkspace.path,
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
              onSwitchBranch: onSwitchBranch,
            ),
          ),
        ),
      ),
    ),
  );
}

GitHistoryResult _historyWithCommit({
  required String id,
  required String subject,
}) => GitHistoryResult(
  currentRef: GitHistoryItemRef(
    id: 'refs/heads/main',
    name: 'main',
    revision: id,
  ),
  hasIncomingChanges: false,
  hasOutgoingChanges: false,
  hasMore: false,
  limit: 50,
  items: <GitHistoryItem>[
    GitHistoryItem(
      id: id,
      parentIds: const <String>[],
      subject: subject,
      message: subject,
    ),
  ],
);

WorkspaceSourceControlScope _nestedScope(Workspace workspace) =>
    WorkspaceSourceControlScope(
      workspaceId: workspace.id,
      workspacePath: workspace.path,
      path: '${workspace.path}/packages/app',
      relativeRoot: 'packages/app',
    );

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

class _PanelSettingsController(final AleraSettings _settings)
    extends SettingsController {
  @override
  AleraSettings build() => _settings;
}

class _FakeWorkbenchRepository implements WorkbenchRepository {
  final List<WorkspaceTabRecord> tabs = <WorkspaceTabRecord>[];
  final Map<String, WorkbenchLayout> layouts = <String, WorkbenchLayout>{};

  @override
  Future<Workspace?> findWorkspaceById(String workspaceId) async => null;

  @override
  Future<WorkspaceTabRecord?> findWorkspaceTabById(String tabId) async {
    for (final tab in tabs) {
      if (tab.id == tabId) {
        return tab;
      }
    }
    return null;
  }

  @override
  Future<WorkbenchLayout?> findWorkbenchLayout(String workspaceId) async {
    return layouts[workspaceId];
  }

  @override
  Future<List<WorkspaceTabRecord>> listWorkspaceTabs(String workspaceId) async {
    return tabs
        .where((tab) => tab.workspaceId == workspaceId)
        .toList(growable: false);
  }

  @override
  Future<List<Workspace>> listWorkspaces(String projectId) async =>
      const <Workspace>[];

  @override
  Future<void> removeWorkspaceTab(String tabId) async {
    tabs.removeWhere((tab) => tab.id == tabId);
  }

  @override
  Future<void> removeWorkspaceTabsForWorkspace(String workspaceId) async {}

  @override
  Future<void> removeWorkspace(
    String workspaceId, {
    bool cascadeTabs = true,
  }) async {}

  @override
  Future<void> removeWorkspacesForProject(String projectId) async {}

  @override
  Future<void> removeWorkbenchLayout(String workspaceId) async {
    layouts.remove(workspaceId);
  }

  @override
  Future<WorkspaceTabRecord> upsertWorkspaceTab(
    WorkspaceTabRecord tab, {
    bool manualRename = false,
  }) async {
    final index = tabs.indexWhere((record) => record.id == tab.id);
    if (index == -1) {
      tabs.add(tab);
    } else {
      tabs[index] = tab;
    }
    return tab;
  }

  @override
  Future<WorkbenchLayout> upsertWorkbenchLayout(WorkbenchLayout layout) async {
    layouts[layout.workspaceId] = layout;
    return layout;
  }

  @override
  Future<Workspace> upsertWorkspace(Workspace workspace) async => workspace;

  @override
  Future<Workspace> setWorkspacePinned(
    String workspaceId,
    bool isPinned,
  ) async => throw StateError('Workspace not found');

  @override
  Stream<List<WorkspaceTabRecord>> watchWorkspaceTabs(String workspaceId) =>
      const Stream<List<WorkspaceTabRecord>>.empty();

  @override
  Stream<List<Workspace>> watchWorkspaces(String projectId) =>
      const Stream<List<Workspace>>.empty();
}
