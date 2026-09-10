import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

final class WorkbenchFileTabPathMovePlan {
  const WorkbenchFileTabPathMovePlan({
    required this.tabs,
    required this.layoutToPersist,
  });

  final List<WorkspaceTabRecord> tabs;
  final WorkbenchLayout? layoutToPersist;
}

WorkbenchFileTabPathMovePlan planWorkbenchFileTabPathMove({
  required String workspaceId,
  required List<WorkspaceTabRecord> currentTabs,
  required WorkbenchLayout? currentLayout,
  required List<WorkspaceTabRecord> updatedTabs,
  required List<String> closedTabIds,
}) {
  final closedIds = closedTabIds.toSet();
  final updatedById = <String, WorkspaceTabRecord>{
    for (final tab in updatedTabs) tab.id: tab,
  };
  final tabs = <WorkspaceTabRecord>[
    for (final tab in currentTabs)
      if (!closedIds.contains(tab.id)) updatedById[tab.id] ?? tab,
  ];
  if (closedIds.isEmpty) {
    return WorkbenchFileTabPathMovePlan(tabs: tabs, layoutToPersist: null);
  }
  if (tabs.isEmpty) {
    return WorkbenchFileTabPathMovePlan(
      tabs: tabs,
      layoutToPersist: WorkbenchLayout.single(
        workspaceId: workspaceId,
        tabIds: const <String>[],
      ),
    );
  }

  var layout =
      (currentLayout ??
              WorkbenchLayout.single(
                workspaceId: workspaceId,
                tabIds: <String>[for (final tab in tabs) tab.id],
              ))
          .sanitize(tabs);
  for (final tabId in closedIds) {
    layout = layout.removeTab(tabId);
  }
  return WorkbenchFileTabPathMovePlan(
    tabs: tabs,
    layoutToPersist: layout.sanitize(tabs),
  );
}
