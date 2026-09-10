import 'package:alera/src/features/workbench/application/workbench_sleep_workspace_plan.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('sleeping the active workspace clears only its local view state', () {
    final sleepingTab = _tab('sleeping-tab', workspaceId: 'sleeping');
    final otherTab = _tab('other-tab', workspaceId: 'other');
    final state = WorkbenchState(
      activeWorkspaceId: 'sleeping',
      tabsByWorkspace: <String, List<WorkspaceTabRecord>>{
        'sleeping': <WorkspaceTabRecord>[sleepingTab],
        'other': <WorkspaceTabRecord>[otherTab],
      },
      layoutByWorkspace: <String, WorkbenchLayout>{
        'sleeping': WorkbenchLayout.single(
          workspaceId: 'sleeping',
          tabIds: <String>[sleepingTab.id],
        ),
        'other': WorkbenchLayout.single(
          workspaceId: 'other',
          tabIds: <String>[otherTab.id],
        ),
      },
      activeTabIdByWorkspace: <String, String>{
        'sleeping': sleepingTab.id,
        'other': otherTab.id,
      },
    );

    final plan = planWorkbenchSleepWorkspace(
      state: state,
      workspaceId: 'sleeping',
    );

    expect(plan.activeWorkspaceId, isNull);
    expect(plan.tabsByWorkspace['sleeping'], isEmpty);
    expect(plan.tabsByWorkspace['other'], <WorkspaceTabRecord>[otherTab]);
    expect(plan.layoutByWorkspace.containsKey('sleeping'), isFalse);
    expect(plan.layoutByWorkspace['other'], state.layoutByWorkspace['other']);
    expect(plan.activeTabIdByWorkspace.containsKey('sleeping'), isFalse);
    expect(plan.activeTabIdByWorkspace['other'], otherTab.id);
  });

  test('sleeping an inactive workspace preserves the active workspace', () {
    final sleepingTab = _tab('sleeping-tab', workspaceId: 'sleeping');
    final state = WorkbenchState(
      activeWorkspaceId: 'active',
      tabsByWorkspace: <String, List<WorkspaceTabRecord>>{
        'sleeping': <WorkspaceTabRecord>[sleepingTab],
      },
      layoutByWorkspace: <String, WorkbenchLayout>{
        'sleeping': WorkbenchLayout.single(
          workspaceId: 'sleeping',
          tabIds: <String>[sleepingTab.id],
        ),
      },
      activeTabIdByWorkspace: <String, String>{
        'sleeping': sleepingTab.id,
        'active': 'active-tab',
      },
    );

    final plan = planWorkbenchSleepWorkspace(
      state: state,
      workspaceId: 'sleeping',
    );

    expect(plan.activeWorkspaceId, 'active');
    expect(plan.tabsByWorkspace['sleeping'], isEmpty);
    expect(plan.layoutByWorkspace.containsKey('sleeping'), isFalse);
    expect(plan.activeTabIdByWorkspace, <String, String>{
      'active': 'active-tab',
    });
  });
}

WorkspaceTabRecord _tab(String id, {required String workspaceId}) {
  final now = DateTime.utc(2026, 9, 10);
  return WorkspaceTabRecord(
    id: id,
    workspaceId: workspaceId,
    title: id,
    createdAt: now,
    updatedAt: now,
  );
}
