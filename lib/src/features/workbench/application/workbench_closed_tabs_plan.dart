import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

final class WorkbenchClosedTabsPlan {
  const WorkbenchClosedTabsPlan({
    required this.layout,
    required this.activeWorkspaceId,
    required this.shouldForgetFocusHistory,
  });

  final WorkbenchLayout layout;
  final String? activeWorkspaceId;
  final bool shouldForgetFocusHistory;
}

WorkbenchClosedTabsPlan planWorkbenchClosedTabs({
  required String workspaceId,
  required List<WorkspaceTabRecord> remainingTabs,
  required WorkbenchLayout? currentLayout,
  required Set<String> closedTabIds,
  required bool closedActiveTab,
  required String? mostRecentOpenTabId,
  required String? activeWorkspaceId,
}) {
  if (remainingTabs.isEmpty) {
    return WorkbenchClosedTabsPlan(
      layout: WorkbenchLayout.single(
        workspaceId: workspaceId,
        tabIds: const <String>[],
      ),
      activeWorkspaceId: activeWorkspaceId == workspaceId
          ? null
          : activeWorkspaceId,
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
    activeWorkspaceId: activeWorkspaceId,
    shouldForgetFocusHistory: false,
  );
}
