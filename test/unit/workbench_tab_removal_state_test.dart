import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/application/workbench_tab_removal_state.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('empty tab remainder clears the active workspace and stale error', () {
    final state = WorkbenchState(
      activeProjectId: 'project',
      activeWorkspaceId: 'workspace',
      error: 'stale error',
      searchQuery: 'keep-search',
      viewPrefs: WorkbenchViewPrefs.defaults.copyWith(
        rightSidebarVisible: false,
      ),
    );

    final next = completeWorkbenchTabRemovalState(
      state: state,
      workspaceId: 'workspace',
      remainingTabs: const <WorkspaceTabRecord>[],
    );

    expect(next.activeWorkspaceId, isNull);
    expect(next.activeProjectId, 'project');
    expect(next.error, isNull);
    expect(next.searchQuery, 'keep-search');
    expect(next.viewPrefs, same(state.viewPrefs));
  });

  test('empty tab remainder keeps another active workspace selected', () {
    const state = WorkbenchState(
      activeProjectId: 'project',
      activeWorkspaceId: 'other-workspace',
      error: 'stale error',
    );

    final next = completeWorkbenchTabRemovalState(
      state: state,
      workspaceId: 'workspace',
      remainingTabs: const <WorkspaceTabRecord>[],
    );

    expect(next.activeWorkspaceId, 'other-workspace');
    expect(next.error, isNull);
  });

  test('non-empty tab remainder keeps the current active workspace', () {
    const state = WorkbenchState(
      activeProjectId: 'project',
      activeWorkspaceId: 'workspace',
      error: 'stale error',
    );
    final tab = WorkspaceTabRecord(
      id: 'tab',
      workspaceId: 'workspace',
      title: 'Tab',
      createdAt: DateTime.utc(2026, 9, 10),
      updatedAt: DateTime.utc(2026, 9, 10),
    );

    final next = completeWorkbenchTabRemovalState(
      state: state,
      workspaceId: 'workspace',
      remainingTabs: <WorkspaceTabRecord>[tab],
    );

    expect(next.activeWorkspaceId, 'workspace');
    expect(next.error, isNull);
  });
}
