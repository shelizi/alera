import 'package:alera/src/features/workbench/application/workbench_tab_placement_plan.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('appended tab uses the active group by default', () {
    final now = DateTime.utc(2026, 9, 11);
    final first = _tab('first', now);
    final added = _tab('added', now);
    final layout = WorkbenchLayout.single(
      workspaceId: 'workspace',
      tabIds: <String>[first.id],
    );

    final plan = planWorkbenchTabAddedToGroup(
      previousTabs: <WorkspaceTabRecord>[first],
      layout: layout,
      tab: added,
      targetGroupId: null,
    );

    expect(plan.tabs, <WorkspaceTabRecord>[first, added]);
    expect(plan.layout.groupIdForTab(added.id), layout.activeGroupId);
    expect(plan.layout.activeTabId, added.id);
  });

  test('appended tab honors an explicit target group', () {
    final now = DateTime.utc(2026, 9, 11);
    final first = _tab('first', now);
    final side = _tab('side', now);
    final added = _tab('added', now);
    final base = WorkbenchLayout.single(
      workspaceId: 'workspace',
      tabIds: <String>[first.id],
    );
    final layout = base.splitWithGroup(
      targetGroupId: base.activeGroupId,
      zone: WorkbenchDropZone.right,
      newGroup: WorkbenchPaneGroup(
        id: 'side-group',
        tabIds: <String>[side.id],
        activeTabId: side.id,
      ),
    );

    final plan = planWorkbenchTabAddedToGroup(
      previousTabs: <WorkspaceTabRecord>[first, side],
      layout: layout,
      tab: added,
      targetGroupId: base.activeGroupId,
    );

    expect(plan.tabs, <WorkspaceTabRecord>[first, side, added]);
    expect(plan.layout.groupIdForTab(added.id), base.activeGroupId);
    expect(plan.layout.activeGroupId, base.activeGroupId);
    expect(plan.layout.groups[base.activeGroupId]?.activeTabId, added.id);
  });

  test('reusable new tab appends into the requested group', () {
    final now = DateTime.utc(2026, 9, 11);
    final first = _tab('first', now);
    final side = _tab('side', now);
    final opened = _tab('opened', now);
    final base = WorkbenchLayout.single(
      workspaceId: 'workspace',
      tabIds: <String>[first.id],
    );
    final layout = base.splitWithGroup(
      targetGroupId: base.activeGroupId,
      zone: WorkbenchDropZone.right,
      newGroup: WorkbenchPaneGroup(
        id: 'side-group',
        tabIds: <String>[side.id],
        activeTabId: side.id,
      ),
    );

    final plan = planWorkbenchReusableTabToGroup(
      previousTabs: <WorkspaceTabRecord>[first, side],
      layout: layout,
      tab: opened,
      targetGroupId: base.activeGroupId,
    );

    expect(plan.tabs, <WorkspaceTabRecord>[first, side, opened]);
    expect(plan.layout.groupIdForTab(opened.id), base.activeGroupId);
    expect(plan.layout.activeGroupId, base.activeGroupId);
    expect(plan.layout.activeTabId, opened.id);
  });

  test(
    'reusable existing tab stays unique and activates its original group',
    () {
      final now = DateTime.utc(2026, 9, 11);
      final first = _tab('first', now);
      final existing = _tab('existing', now);
      final base = WorkbenchLayout.single(
        workspaceId: 'workspace',
        tabIds: <String>[first.id],
      );
      final layout = base
          .splitWithGroup(
            targetGroupId: base.activeGroupId,
            zone: WorkbenchDropZone.right,
            newGroup: WorkbenchPaneGroup(
              id: 'existing-group',
              tabIds: <String>[existing.id],
              activeTabId: existing.id,
            ),
          )
          .setActiveTab(groupId: base.activeGroupId, tabId: first.id);

      final plan = planWorkbenchReusableTabToGroup(
        previousTabs: <WorkspaceTabRecord>[first, existing],
        layout: layout,
        tab: existing,
        targetGroupId: base.activeGroupId,
      );

      expect(plan.tabs, <WorkspaceTabRecord>[first, existing]);
      expect(plan.tabs.where((tab) => tab.id == existing.id), hasLength(1));
      expect(plan.layout.activeGroupId, 'existing-group');
      expect(plan.layout.activeTabId, existing.id);
    },
  );

  test(
    'reusable existing tab missing from layout falls back to requested group',
    () {
      final now = DateTime.utc(2026, 9, 11);
      final first = _tab('first', now);
      final existing = _tab('existing', now);
      final layout = WorkbenchLayout.single(
        workspaceId: 'workspace',
        tabIds: <String>[first.id],
      );

      final plan = planWorkbenchReusableTabToGroup(
        previousTabs: <WorkspaceTabRecord>[first, existing],
        layout: layout,
        tab: existing,
        targetGroupId: layout.activeGroupId,
      );

      expect(plan.tabs, <WorkspaceTabRecord>[first, existing]);
      expect(plan.layout.groupIdForTab(existing.id), layout.activeGroupId);
      expect(plan.layout.activeTabId, first.id);
    },
  );
  test('split tab creates a new active group with the supplied id', () {
    final now = DateTime.utc(2026, 9, 11);
    final first = _tab('first', now);
    final added = _tab('added', now);
    final layout = WorkbenchLayout.single(
      workspaceId: 'workspace',
      tabIds: <String>[first.id],
    );

    final plan = planWorkbenchTabSplitIntoGroup(
      previousTabs: <WorkspaceTabRecord>[first],
      layout: layout,
      tab: added,
      targetGroupId: layout.activeGroupId,
      zone: WorkbenchDropZone.right,
      newGroupId: 'new-group',
    );

    expect(plan.tabs, <WorkspaceTabRecord>[first, added]);
    expect(plan.layout.paneGroupIds, hasLength(2));
    expect(plan.layout.groupIdForTab(first.id), layout.activeGroupId);
    expect(plan.layout.groupIdForTab(added.id), 'new-group');
    expect(plan.layout.activeGroupId, 'new-group');
    expect(plan.layout.groups['new-group']?.tabIds, <String>[added.id]);
    expect(plan.layout.groups['new-group']?.activeTabId, added.id);
  });
}

WorkspaceTabRecord _tab(String id, DateTime now) => WorkspaceTabRecord(
  id: id,
  workspaceId: 'workspace',
  title: id,
  createdAt: now,
  updatedAt: now,
);
