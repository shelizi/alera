import 'package:alera/src/features/workbench/application/workbench_hosted_review_retention_service.dart';
import 'package:alera/src/features/workbench/application/workbench_remove_project_cleanup_coordinator.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_focus_history.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'project cleanup forgets each workspace before releasing its tabs',
    () async {
      final events = <String>[];
      final focus = _RecordingFocusHistory(events)
        ..record('workspace-1', 'tab-1')
        ..record('workspace-2', 'tab-3');
      final retention = _FakeRetention(events);
      Future<void> removeProject() async => events.add('remove-project');
      final coordinator = WorkbenchRemoveProjectCleanupCoordinator(
        closeLocalWorkspace: (workspaceId) => events.add('close:$workspaceId'),
        hostedReviewRetention: retention,
        tabFocusHistory: focus,
      );

      await coordinator.remove(
        removeProject: removeProject,
        removedWorkspaces: <WorkbenchRemovedProjectWorkspace>[
          WorkbenchRemovedProjectWorkspace(
            workspace: _workspace('workspace-1'),
            tabs: <WorkspaceTabRecord>[
              _tab('workspace-1', 'tab-1'),
              _tab('workspace-1', 'tab-2'),
            ],
          ),
          WorkbenchRemovedProjectWorkspace(
            workspace: _workspace('workspace-2'),
            tabs: <WorkspaceTabRecord>[_tab('workspace-2', 'tab-3')],
          ),
        ],
      );

      expect(events, <String>[
        'close:workspace-1',
        'close:workspace-2',
        'remove-project',
        'forget:workspace-1',
        'release:workspace-1:tab-1',
        'release:workspace-1:tab-2',
        'forget:workspace-2',
        'release:workspace-2:tab-3',
      ]);
    },
  );

  test(
    'project removal failure closes local workspaces but skips cleanup',
    () async {
      final events = <String>[];
      final focus = _RecordingFocusHistory(events)
        ..record('workspace-1', 'tab-1')
        ..record('workspace-2', 'tab-2');
      final coordinator = WorkbenchRemoveProjectCleanupCoordinator(
        closeLocalWorkspace: (workspaceId) => events.add('close:$workspaceId'),
        hostedReviewRetention: _FakeRetention(events),
        tabFocusHistory: focus,
      );

      await expectLater(
        coordinator.remove(
          removeProject: () async {
            events.add('remove-project');
            throw StateError('cannot remove');
          },
          removedWorkspaces: <WorkbenchRemovedProjectWorkspace>[
            WorkbenchRemovedProjectWorkspace(
              workspace: _workspace('workspace-1'),
              tabs: <WorkspaceTabRecord>[_tab('workspace-1', 'tab-1')],
            ),
            WorkbenchRemovedProjectWorkspace(
              workspace: _workspace('workspace-2'),
              tabs: <WorkspaceTabRecord>[_tab('workspace-2', 'tab-2')],
            ),
          ],
        ),
        throwsStateError,
      );

      expect(events, <String>[
        'close:workspace-1',
        'close:workspace-2',
        'remove-project',
      ]);
      expect(focus.mostRecentOpen('workspace-1', <String>{'tab-1'}), 'tab-1');
      expect(focus.mostRecentOpen('workspace-2', <String>{'tab-2'}), 'tab-2');
    },
  );

  test('retention failure stops before cleaning later workspaces', () async {
    final events = <String>[];
    final focus = _RecordingFocusHistory(events)
      ..record('workspace-1', 'tab-1')
      ..record('workspace-2', 'tab-2');
    final retention = _FakeRetention(events, failOnTabId: 'tab-1');
    final coordinator = WorkbenchRemoveProjectCleanupCoordinator(
      closeLocalWorkspace: (workspaceId) => events.add('close:$workspaceId'),
      hostedReviewRetention: retention,
      tabFocusHistory: focus,
    );

    await expectLater(
      coordinator.cleanup(<WorkbenchRemovedProjectWorkspace>[
        WorkbenchRemovedProjectWorkspace(
          workspace: _workspace('workspace-1'),
          tabs: <WorkspaceTabRecord>[_tab('workspace-1', 'tab-1')],
        ),
        WorkbenchRemovedProjectWorkspace(
          workspace: _workspace('workspace-2'),
          tabs: <WorkspaceTabRecord>[_tab('workspace-2', 'tab-2')],
        ),
      ]),
      throwsA(isA<StateError>()),
    );

    expect(events, <String>['forget:workspace-1', 'release:workspace-1:tab-1']);
    expect(focus.mostRecentOpen('workspace-1', <String>{'tab-1'}), isNull);
    expect(focus.mostRecentOpen('workspace-2', <String>{'tab-2'}), 'tab-2');
  });
}

final class _RecordingFocusHistory extends WorkspaceTabFocusHistory {
  _RecordingFocusHistory(this.events);

  final List<String> events;

  @override
  void forget(String workspaceId) {
    events.add('forget:$workspaceId');
    super.forget(workspaceId);
  }
}

final class _FakeRetention implements WorkbenchHostedReviewTabRetention {
  _FakeRetention(this.events, {this.failOnTabId});

  final List<String> events;
  final String? failOnTabId;

  @override
  Future<void> releaseTab(
    Workspace workspace,
    WorkspaceTabRecord tab, {
    String? fallbackWorkspacePath,
  }) async {
    events.add('release:${workspace.id}:${tab.id}');
    if (tab.id == failOnTabId) throw StateError('retention failed');
  }
}

Workspace _workspace(String id) => Workspace(
  id: id,
  projectId: 'project',
  name: id,
  path: '$id-path',
  createdAt: DateTime.utc(2026, 9, 11),
  updatedAt: DateTime.utc(2026, 9, 11),
  kind: WorkspaceKind.linked,
  status: WorkspaceStatus.active,
);

WorkspaceTabRecord _tab(String workspaceId, String id) {
  final now = DateTime.utc(2026, 9, 11);
  return WorkspaceTabRecord(
    id: id,
    workspaceId: workspaceId,
    title: id,
    createdAt: now,
    updatedAt: now,
  );
}
