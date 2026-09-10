import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

final class WorkbenchPersistedTabRestorePlan {
  const WorkbenchPersistedTabRestorePlan({
    required this.tabs,
    required this.layoutToPersist,
  });

  final List<WorkspaceTabRecord> tabs;
  final WorkbenchLayout? layoutToPersist;
}

WorkbenchPersistedTabRestorePlan planWorkbenchPersistedTabRestore({
  required List<WorkspaceTabRecord> currentTabs,
  required WorkbenchLayout layout,
  required WorkspaceTabRecord restoredTab,
}) {
  final alreadyLocal = currentTabs.any((tab) => tab.id == restoredTab.id);
  final tabs = <WorkspaceTabRecord>[
    for (final tab in currentTabs)
      if (tab.id == restoredTab.id) restoredTab else tab,
    if (!alreadyLocal) restoredTab,
  ];
  if (layout.groupIdForTab(restoredTab.id) != null) {
    return WorkbenchPersistedTabRestorePlan(tabs: tabs, layoutToPersist: null);
  }
  final nextLayout = layout
      .addTabToGroup(groupId: layout.activeGroupId, tabId: restoredTab.id)
      .sanitize(tabs);
  return WorkbenchPersistedTabRestorePlan(
    tabs: tabs,
    layoutToPersist: nextLayout,
  );
}
