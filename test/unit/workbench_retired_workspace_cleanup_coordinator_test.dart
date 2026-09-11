import 'package:alera/src/features/workbench/application/workbench_retired_workspace_cleanup_coordinator.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('available workspace releases hosted review before focus and local resources', () {
    final events = <String>[];
    final workspace = _workspace();
    final tabs = <WorkspaceTabRecord>[_tab('one'), _tab('two')];
    final coordinator = WorkbenchRetiredWorkspaceCleanupCoordinator(
      releaseHostedReviewTabsInBackground: (workspace, tabs) => events.add(
        'hosted:${workspace.id}:${tabs.map((tab) => tab.id).join(',')}',
      ),
      forgetFocusHistory: (workspaceId) => events.add('focus:$workspaceId'),
      releaseLocalWorkspace: (workspaceId, tabs) => events.add(
        'local:$workspaceId:${tabs.map((tab) => tab.id).join(',')}',
      ),
    );

    coordinator.cleanup(
      workspaceId: workspace.id,
      workspace: workspace,
      tabs: tabs,
    );

    expect(events, <String>[
      'hosted:workspace:one,two',
      'focus:workspace',
      'local:workspace:one,two',
    ]);
  });

  test(
    'missing workspace skips hosted review but still releases local state',
    () {
      final events = <String>[];
      final tabs = <WorkspaceTabRecord>[_tab('one')];
      final coordinator = WorkbenchRetiredWorkspaceCleanupCoordinator(
        releaseHostedReviewTabsInBackground: (_, _) => events.add('hosted'),
        forgetFocusHistory: (workspaceId) => events.add('focus:$workspaceId'),
        releaseLocalWorkspace: (workspaceId, tabs) =>
            events.add('local:$workspaceId:${tabs.single.id}'),
      );

      coordinator.cleanup(
        workspaceId: 'workspace',
        workspace: null,
        tabs: tabs,
      );

      expect(events, <String>['focus:workspace', 'local:workspace:one']);
    },
  );

  test('hosted review scheduling failure stops local cleanup as before', () {
    final events = <String>[];
    final coordinator = WorkbenchRetiredWorkspaceCleanupCoordinator(
      releaseHostedReviewTabsInBackground: (_, _) {
        events.add('hosted');
        throw StateError('schedule failed');
      },
      forgetFocusHistory: (_) => events.add('focus'),
      releaseLocalWorkspace: (_, _) => events.add('local'),
    );

    expect(
      () => coordinator.cleanup(
        workspaceId: 'workspace',
        workspace: _workspace(),
        tabs: <WorkspaceTabRecord>[_tab('one')],
      ),
      throwsStateError,
    );
    expect(events, <String>['hosted']);
  });
}

Workspace _workspace() {
  final now = DateTime.utc(2026, 9, 11);
  return Workspace(
    id: 'workspace',
    projectId: 'project',
    name: 'Main',
    path: 'repo',
    createdAt: now,
    updatedAt: now,
    kind: WorkspaceKind.main,
    status: WorkspaceStatus.active,
  );
}

WorkspaceTabRecord _tab(String id) {
  final now = DateTime.utc(2026, 9, 11);
  return WorkspaceTabRecord(
    id: id,
    workspaceId: 'workspace',
    title: id,
    createdAt: now,
    updatedAt: now,
  );
}
