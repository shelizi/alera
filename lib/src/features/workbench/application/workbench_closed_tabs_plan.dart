import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

final class WorkbenchTabCloseSnapshot {
  const WorkbenchTabCloseSnapshot({
    required this.closedTabIds,
    required this.closingTabs,
    required this.closedActiveTab,
  });

  final Set<String> closedTabIds;
  final Map<String, WorkspaceTabRecord> closingTabs;
  final bool closedActiveTab;
}

WorkbenchTabCloseSnapshot captureWorkbenchTabCloseSnapshot({
  required List<String> requestedTabIds,
  required List<WorkspaceTabRecord> currentTabs,
  required WorkbenchLayout? currentLayout,
}) {
  final closedTabIds = <String>{...requestedTabIds};
  final activeTabId = currentLayout?.activeTabId;
  return WorkbenchTabCloseSnapshot(
    closedTabIds: closedTabIds,
    closingTabs: <String, WorkspaceTabRecord>{
      for (final tab in currentTabs)
        if (closedTabIds.contains(tab.id)) tab.id: tab,
    },
    closedActiveTab: activeTabId != null && closedTabIds.contains(activeTabId),
  );
}

final class WorkbenchClosedTabsPlan {
  const WorkbenchClosedTabsPlan({
    required this.layout,
    required this.shouldForgetFocusHistory,
  });

  final WorkbenchLayout layout;
  final bool shouldForgetFocusHistory;
}

WorkbenchClosedTabsPlan planWorkbenchClosedTabs({
  required String workspaceId,
  required List<WorkspaceTabRecord> remainingTabs,
  required WorkbenchLayout? currentLayout,
  required Set<String> closedTabIds,
  required bool closedActiveTab,
  required String? mostRecentOpenTabId,
}) {
  if (remainingTabs.isEmpty) {
    return WorkbenchClosedTabsPlan(
      layout: WorkbenchLayout.single(
        workspaceId: workspaceId,
        tabIds: const <String>[],
      ),
      shouldForgetFocusHistory: true,
    );
  }

  var layout =
      (currentLayout ??
              WorkbenchLayout.single(
                workspaceId: workspaceId,
                tabIds: <String>[for (final tab in remainingTabs) tab.id],
              ))
          .sanitize(remainingTabs);
  for (final tabId in closedTabIds) {
    layout = layout.removeTab(tabId);
  }
  layout = layout.sanitize(remainingTabs);

  if (closedActiveTab && mostRecentOpenTabId != null) {
    final groupId = layout.groupIdForTab(mostRecentOpenTabId);
    if (groupId != null) {
      layout = layout.setActiveTab(
        groupId: groupId,
        tabId: mostRecentOpenTabId,
      );
    }
  }

  return WorkbenchClosedTabsPlan(
    layout: layout,
    shouldForgetFocusHistory: false,
  );
}
