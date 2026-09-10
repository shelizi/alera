import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

final class WorkbenchTabPlacementPlan {
  const WorkbenchTabPlacementPlan({required this.tabs, required this.layout});

  final List<WorkspaceTabRecord> tabs;
  final WorkbenchLayout layout;
}

WorkbenchTabPlacementPlan planWorkbenchTabAddedToGroup({
  required List<WorkspaceTabRecord> previousTabs,
  required WorkbenchLayout layout,
  required WorkspaceTabRecord tab,
  required String? targetGroupId,
}) {
  final tabs = <WorkspaceTabRecord>[...previousTabs, tab];
  final groupId = targetGroupId ?? layout.activeGroupId;
  final nextLayout = layout
      .addTabToGroup(groupId: groupId, tabId: tab.id)
      .sanitize(tabs);
  return WorkbenchTabPlacementPlan(tabs: tabs, layout: nextLayout);
}

WorkbenchTabPlacementPlan planWorkbenchTabSplitIntoGroup({
  required List<WorkspaceTabRecord> previousTabs,
  required WorkbenchLayout layout,
  required WorkspaceTabRecord tab,
  required String targetGroupId,
  required WorkbenchDropZone zone,
  required String newGroupId,
}) {
  final tabs = <WorkspaceTabRecord>[...previousTabs, tab];
  final nextLayout = layout
      .splitWithGroup(
        targetGroupId: targetGroupId,
        zone: zone,
        newGroup: WorkbenchPaneGroup(
          id: newGroupId,
          tabIds: <String>[tab.id],
          activeTabId: tab.id,
        ),
      )
      .sanitize(tabs);
  return WorkbenchTabPlacementPlan(tabs: tabs, layout: nextLayout);
}
