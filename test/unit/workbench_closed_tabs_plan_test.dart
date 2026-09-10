import 'package:alera/src/features/workbench/application/workbench_closed_tabs_plan.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('close snapshot deduplicates ids and captures current tab records', () {
    final first = _tab('first');
    final second = _tab('second');
    final layout = WorkbenchLayout.single(
      workspaceId: 'workspace',
      tabIds: <String>[first.id, second.id],
    );

    final snapshot = captureWorkbenchTabCloseSnapshot(
      requestedTabIds: <String>[second.id, second.id, 'missing'],
      currentTabs: <WorkspaceTabRecord>[first, second],
      currentLayout: layout,
    );

    expect(snapshot.closedTabIds, <String>{second.id, 'missing'});
    expect(snapshot.closingTabs, <String, WorkspaceTabRecord>{
      second.id: second,
    });
    expect(snapshot.closedActiveTab, isTrue);
  });

  test(
    'close snapshot does not mark active closed when another tab is requested',
    () {
      final first = _tab('first');
      final second = _tab('second');
      final base = WorkbenchLayout.single(
        workspaceId: 'workspace',
        tabIds: <String>[first.id, second.id],
      );
      final layout = base.setActiveTab(
        groupId: base.activeGroupId,
        tabId: first.id,
      );

      final snapshot = captureWorkbenchTabCloseSnapshot(
        requestedTabIds: <String>[second.id],
        currentTabs: <WorkspaceTabRecord>[first, second],
        currentLayout: layout,
      );

      expect(snapshot.closedTabIds, <String>{second.id});
      expect(snapshot.closingTabs.keys, <String>{second.id});
      expect(snapshot.closedActiveTab, isFalse);
    },
  );

  test('empty close request produces an empty snapshot', () {
    final snapshot = captureWorkbenchTabCloseSnapshot(
      requestedTabIds: const <String>[],
      currentTabs: <WorkspaceTabRecord>[_tab('first')],
      currentLayout: null,
    );

    expect(snapshot.closedTabIds, isEmpty);
    expect(snapshot.closingTabs, isEmpty);
    expect(snapshot.closedActiveTab, isFalse);
  });

  test('removes closed tabs and refocuses the supplied recent survivor', () {
    final first = _tab('first');
    final second = _tab('second');
    final third = _tab('third');
    final layout =
        WorkbenchLayout.single(
          workspaceId: 'workspace',
          tabIds: <String>[first.id, second.id, third.id],
        ).setActiveTab(
          groupId: WorkbenchLayout.defaultGroupId('workspace'),
          tabId: first.id,
        );

    final plan = planWorkbenchClosedTabs(
      workspaceId: 'workspace',
      remainingTabs: <WorkspaceTabRecord>[second, third],
      currentLayout: layout,
      closedTabIds: <String>{first.id},
      closedActiveTab: true,
      mostRecentOpenTabId: third.id,
    );

    expect(plan.layout.activeTabId, third.id);
    expect(plan.layout.groupIdForTab(first.id), isNull);
    expect(plan.shouldForgetFocusHistory, isFalse);
  });

  test(
    'empty remainder creates an empty layout and clears active workspace',
    () {
      final plan = planWorkbenchClosedTabs(
        workspaceId: 'workspace',
        remainingTabs: const <WorkspaceTabRecord>[],
        currentLayout: null,
        closedTabIds: const <String>{'last'},
        closedActiveTab: true,
        mostRecentOpenTabId: null,
      );

      expect(plan.layout.activeTabId, isNull);
      expect(plan.layout.groups[plan.layout.activeGroupId]?.tabIds, isEmpty);
      expect(plan.shouldForgetFocusHistory, isTrue);
    },
  );

  test('does not steal focus when the closed tab was not active', () {
    final first = _tab('first');
    final second = _tab('second');
    final layout =
        WorkbenchLayout.single(
          workspaceId: 'workspace',
          tabIds: <String>[first.id, second.id],
        ).setActiveTab(
          groupId: WorkbenchLayout.defaultGroupId('workspace'),
          tabId: second.id,
        );

    final plan = planWorkbenchClosedTabs(
      workspaceId: 'workspace',
      remainingTabs: <WorkspaceTabRecord>[second],
      currentLayout: layout,
      closedTabIds: <String>{first.id},
      closedActiveTab: false,
      mostRecentOpenTabId: second.id,
    );

    expect(plan.layout.activeTabId, second.id);
    expect(plan.shouldForgetFocusHistory, isFalse);
  });
}

WorkspaceTabRecord _tab(String id) {
  final now = DateTime.utc(2026, 9, 10);
  return WorkspaceTabRecord(
    id: id,
    workspaceId: 'workspace',
    title: id,
    createdAt: now,
    updatedAt: now,
  );
}
