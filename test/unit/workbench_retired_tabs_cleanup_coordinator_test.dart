import 'package:alera/src/features/workbench/application/workbench_retired_tabs_cleanup_coordinator.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'available workspace schedules hosted release before local tab cleanup',
    () {
      final events = <String>[];
      final tabs = <WorkspaceTabRecord>[_tab('one'), _tab('two')];
      final coordinator = WorkbenchRetiredTabsCleanupCoordinator(
        releaseHostedReviewTabsInBackground: (workspace, tabs) => events.add(
          'hosted:${workspace.id}:${tabs.map((tab) => tab.id).join(',')}',
        ),
        releaseLocalTabs: (tabs) =>
            events.add('local:${tabs.map((tab) => tab.id).join(',')}'),
      );

      coordinator.cleanup(workspace: _workspace(), tabs: tabs);

      expect(events, <String>['hosted:workspace:one,two', 'local:one,two']);
    },
  );

  test(
    'missing workspace skips hosted review but still releases local tabs',
    () {
      final events = <String>[];
      final coordinator = WorkbenchRetiredTabsCleanupCoordinator(
        releaseHostedReviewTabsInBackground: (_, _) => events.add('hosted'),
        releaseLocalTabs: (tabs) => events.add('local:${tabs.single.id}'),
      );

      coordinator.cleanup(
        workspace: null,
        tabs: <WorkspaceTabRecord>[_tab('one')],
      );

      expect(events, <String>['local:one']);
    },
  );

  test('hosted review scheduling failure stops before local cleanup', () {
    final events = <String>[];
    final coordinator = WorkbenchRetiredTabsCleanupCoordinator(
      releaseHostedReviewTabsInBackground: (_, _) {
        events.add('hosted');
        throw StateError('schedule failed');
      },
      releaseLocalTabs: (_) => events.add('local'),
    );

    expect(
      () => coordinator.cleanup(
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
