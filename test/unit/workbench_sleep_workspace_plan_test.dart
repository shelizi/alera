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
      activeProjectId: 'project',
      activeWorkspaceId: 'sleeping',
      error: 'stale error',
      searchQuery: 'keep-search',
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

    final next = applyWorkbenchSleepWorkspaceState(
      state: state,
      workspaceId: 'sleeping',
    );

    expect(next.activeWorkspaceId, isNull);
    expect(next.activeProjectId, 'project');
    expect(next.tabsByWorkspace['sleeping'], isEmpty);
    expect(next.tabsByWorkspace['other'], <WorkspaceTabRecord>[otherTab]);
    expect(next.layoutByWorkspace.containsKey('sleeping'), isFalse);
    expect(next.layoutByWorkspace['other'], state.layoutByWorkspace['other']);
    expect(next.activeTabIdByWorkspace.containsKey('sleeping'), isFalse);
    expect(next.activeTabIdByWorkspace['other'], otherTab.id);
    expect(next.searchQuery, 'keep-search');
    expect(next.error, isNull);
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

    final next = applyWorkbenchSleepWorkspaceState(
      state: state,
      workspaceId: 'sleeping',
    );

    expect(next.activeWorkspaceId, 'active');
    expect(next.tabsByWorkspace['sleeping'], isEmpty);
    expect(next.layoutByWorkspace.containsKey('sleeping'), isFalse);
    expect(next.activeTabIdByWorkspace, <String, String>{
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
