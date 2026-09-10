import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/application/workbench_tab_set_sync.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('sanitizes layout and active tab when persisted tabs disappear', () {
    final now = DateTime.utc(2026, 9, 10);
    final liveTab = _tab('live', now);
    final removedTab = _tab('removed', now);
    final layout = WorkbenchLayout.single(
      workspaceId: 'workspace',
      tabIds: <String>[liveTab.id, removedTab.id],
    );
    final state = WorkbenchState(
      tabsByWorkspace: <String, List<WorkspaceTabRecord>>{
        'workspace': <WorkspaceTabRecord>[liveTab, removedTab],
      },
      layoutByWorkspace: <String, WorkbenchLayout>{'workspace': layout},
      activeTabIdByWorkspace: <String, String>{'workspace': removedTab.id},
    );

    final plan = planWorkbenchTabSetSync(
      state: state,
      workspaceId: 'workspace',
      tabs: <WorkspaceTabRecord>[liveTab],
      layoutWasCleared: false,
    );

    expect(plan.removedTabs, <WorkspaceTabRecord>[removedTab]);
    expect(plan.tabsByWorkspace['workspace'], <WorkspaceTabRecord>[liveTab]);
    expect(plan.layoutByWorkspace['workspace']?.activeTabId, liveTab.id);
    expect(plan.activeTabIdByWorkspace['workspace'], liveTab.id);
    expect(plan.layoutToPersist, isNotNull);
    expect(plan.shouldLoadLayout, isFalse);
  });

  test('applies tab sync plan while preserving unrelated state', () {
    final now = DateTime.utc(2026, 9, 10);
    final liveTab = _tab('live', now);
    final removedTab = _tab('removed', now);
    final state = WorkbenchState(
      activeProjectId: 'project',
      activeWorkspaceId: 'workspace',
      tabsByWorkspace: <String, List<WorkspaceTabRecord>>{
        'workspace': <WorkspaceTabRecord>[liveTab, removedTab],
      },
      layoutByWorkspace: <String, WorkbenchLayout>{
        'workspace': WorkbenchLayout.single(
          workspaceId: 'workspace',
          tabIds: <String>[liveTab.id, removedTab.id],
        ),
      },
      activeTabIdByWorkspace: <String, String>{'workspace': removedTab.id},
      searchQuery: 'keep-search',
      error: 'keep-error',
    );
    final plan = planWorkbenchTabSetSync(
      state: state,
      workspaceId: 'workspace',
      tabs: <WorkspaceTabRecord>[liveTab],
      layoutWasCleared: false,
    );

    final next = applyWorkbenchTabSetSyncPlan(state: state, plan: plan);

    expect(next.tabsByWorkspace, plan.tabsByWorkspace);
    expect(next.layoutByWorkspace, plan.layoutByWorkspace);
    expect(next.activeTabIdByWorkspace, plan.activeTabIdByWorkspace);
    expect(next.activeProjectId, 'project');
    expect(next.activeWorkspaceId, 'workspace');
    expect(next.searchQuery, 'keep-search');
    expect(next.error, 'keep-error');
  });

  test('drops a cleared layout after its last tab disappears', () {
    final now = DateTime.utc(2026, 9, 10);
    final removedTab = _tab('removed', now);
    final state = WorkbenchState(
      tabsByWorkspace: <String, List<WorkspaceTabRecord>>{
        'workspace': <WorkspaceTabRecord>[removedTab],
      },
      layoutByWorkspace: <String, WorkbenchLayout>{
        'workspace': WorkbenchLayout.single(
          workspaceId: 'workspace',
          tabIds: <String>[removedTab.id],
        ),
      },
      activeTabIdByWorkspace: <String, String>{'workspace': removedTab.id},
    );

    final plan = planWorkbenchTabSetSync(
      state: state,
      workspaceId: 'workspace',
      tabs: const <WorkspaceTabRecord>[],
      layoutWasCleared: true,
    );

    expect(plan.layoutByWorkspace.containsKey('workspace'), isFalse);
    expect(plan.activeTabIdByWorkspace.containsKey('workspace'), isFalse);
    expect(plan.layoutToPersist, isNull);
    expect(plan.shouldLoadLayout, isFalse);
  });

  test('requests layout loading when tabs arrive before a layout', () {
    final now = DateTime.utc(2026, 9, 10);
    final tab = _tab('live', now);

    final plan = planWorkbenchTabSetSync(
      state: const WorkbenchState(),
      workspaceId: 'workspace',
      tabs: <WorkspaceTabRecord>[tab],
      layoutWasCleared: false,
    );

    expect(plan.tabsByWorkspace['workspace'], <WorkspaceTabRecord>[tab]);
    expect(plan.shouldLoadLayout, isTrue);
    expect(plan.layoutToPersist, isNull);
  });

  test('direct tab state replacement updates only the target workspace', () {
    final now = DateTime.utc(2026, 9, 10);
    final oldTab = _tab('old', now);
    final nextTab = _tab('next', now);
    final otherTab = WorkspaceTabRecord(
      id: 'other',
      workspaceId: 'other-workspace',
      title: 'other',
      createdAt: now,
      updatedAt: now,
    );
    final layout = WorkbenchLayout.single(
      workspaceId: 'workspace',
      tabIds: <String>[oldTab.id],
    );
    final state = WorkbenchState(
      tabsByWorkspace: <String, List<WorkspaceTabRecord>>{
        'workspace': <WorkspaceTabRecord>[oldTab],
        'other-workspace': <WorkspaceTabRecord>[otherTab],
      },
      layoutByWorkspace: <String, WorkbenchLayout>{'workspace': layout},
      activeTabIdByWorkspace: <String, String>{'workspace': oldTab.id},
      searchQuery: 'keep-search',
    );

    final next = applyWorkbenchTabsState(
      state: state,
      workspaceId: 'workspace',
      tabs: <WorkspaceTabRecord>[nextTab],
    );

    expect(next.tabsByWorkspace['workspace'], <WorkspaceTabRecord>[nextTab]);
    expect(next.tabsByWorkspace['other-workspace'], <WorkspaceTabRecord>[
      otherTab,
    ]);
    expect(next.layoutByWorkspace, same(state.layoutByWorkspace));
    expect(next.activeTabIdByWorkspace, same(state.activeTabIdByWorkspace));
    expect(next.searchQuery, 'keep-search');
  });
}

WorkspaceTabRecord _tab(String id, DateTime now) => WorkspaceTabRecord(
  id: id,
  workspaceId: 'workspace',
  title: id,
  createdAt: now,
  updatedAt: now,
);
