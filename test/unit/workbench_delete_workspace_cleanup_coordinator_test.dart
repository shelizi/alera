import 'package:alera/src/features/workbench/application/workbench_delete_workspace_cleanup_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_explicit_resource_cleaner.dart';
import 'package:alera/src/features/workbench/application/workbench_hosted_review_retention_service.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_focus_history.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('delete cleanup preserves local-retention-observer ordering', () async {
    final events = <String>[];
    final cleaner = _FakeWorkspaceCleaner(events);
    final retention = _FakeRetention(events);
    final focus = WorkspaceTabFocusHistory()..record('workspace', 'tab-2');
    final coordinator = WorkbenchDeleteWorkspaceCleanupCoordinator(
      resourceCleaner: cleaner,
      hostedReviewRetention: retention,
      tabFocusHistory: focus,
    );
    final tabs = <WorkspaceTabRecord>[_tab('tab-1'), _tab('tab-2')];

    await coordinator.cleanup(
      workspace: _workspace(),
      tabs: tabs,
      fallbackWorkspacePath: 'repo-path',
    );

    expect(events, <String>[
      'close-local:workspace',
      'release:tab-1:repo-path',
      'release:tab-2:repo-path',
      'clear-observers:workspace',
    ]);
    expect(focus.mostRecentOpen('workspace', <String>{'tab-2'}), isNull);
  });

  test('retention failure stops before focus and observer cleanup', () async {
    final events = <String>[];
    final cleaner = _FakeWorkspaceCleaner(events);
    final retention = _FakeRetention(events, failOnTabId: 'tab-2');
    final focus = WorkspaceTabFocusHistory()..record('workspace', 'tab-1');
    final coordinator = WorkbenchDeleteWorkspaceCleanupCoordinator(
      resourceCleaner: cleaner,
      hostedReviewRetention: retention,
      tabFocusHistory: focus,
    );
    final tabs = <WorkspaceTabRecord>[_tab('tab-1'), _tab('tab-2')];

    await expectLater(
      coordinator.cleanup(
        workspace: _workspace(),
        tabs: tabs,
        fallbackWorkspacePath: 'repo-path',
      ),
      throwsA(isA<StateError>()),
    );

    expect(events, <String>[
      'close-local:workspace',
      'release:tab-1:repo-path',
      'release:tab-2:repo-path',
    ]);
    expect(focus.mostRecentOpen('workspace', <String>{'tab-1'}), 'tab-1');
  });
}

final class _FakeWorkspaceCleaner
    implements WorkbenchExplicitWorkspaceResourceCleaner {
  _FakeWorkspaceCleaner(this.events);

  final List<String> events;

  @override
  void closeWorkspaceLocalResources(
    String workspaceId,
    Iterable<WorkspaceTabRecord> tabs,
  ) {
    events.add('close-local:$workspaceId');
  }

  @override
  void clearDeletedWorkspaceObservers(
    String workspaceId,
    Iterable<WorkspaceTabRecord> tabs,
  ) {
    events.add('clear-observers:$workspaceId');
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
    events.add('release:${tab.id}:$fallbackWorkspacePath');
    if (tab.id == failOnTabId) throw StateError('retention failed');
  }
}

Workspace _workspace() => Workspace(
  id: 'workspace',
  projectId: 'project',
  name: 'Workspace',
  path: 'workspace-path',
  createdAt: DateTime.utc(2026, 9, 11),
  updatedAt: DateTime.utc(2026, 9, 11),
  kind: WorkspaceKind.linked,
  status: WorkspaceStatus.active,
);

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
