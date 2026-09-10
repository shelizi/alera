import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

WorkbenchState applyWorkbenchTabsState({
  required WorkbenchState state,
  required String workspaceId,
  required List<WorkspaceTabRecord> tabs,
}) {
  final tabsByWorkspace = Map<String, List<WorkspaceTabRecord>>.from(
    state.tabsByWorkspace,
  )..[workspaceId] = tabs;
  return state.copyWith(tabsByWorkspace: tabsByWorkspace);
}

WorkbenchState applyWorkbenchTabUpdateState({
  required WorkbenchState state,
  required WorkspaceTabRecord tab,
}) {
  final current = state.tabsFor(tab.workspaceId);
  if (!current.any((candidate) => candidate.id == tab.id)) {
    return state;
  }
  return applyWorkbenchTabsState(
    state: state,
    workspaceId: tab.workspaceId,
    tabs: <WorkspaceTabRecord>[
      for (final candidate in current)
        if (candidate.id == tab.id) tab else candidate,
    ],
  );
}

final class WorkbenchTabSetSyncPlan {
  const WorkbenchTabSetSyncPlan({
    required this.removedTabs,
    required this.tabsByWorkspace,
    required this.layoutByWorkspace,
    required this.activeTabIdByWorkspace,
    required this.layoutToPersist,
    required this.shouldLoadLayout,
  });

  final List<WorkspaceTabRecord> removedTabs;
  final Map<String, List<WorkspaceTabRecord>> tabsByWorkspace;
  final Map<String, WorkbenchLayout> layoutByWorkspace;
  final Map<String, String> activeTabIdByWorkspace;
  final WorkbenchLayout? layoutToPersist;
  final bool shouldLoadLayout;
}

WorkbenchState applyWorkbenchTabSetSyncPlan({
  required WorkbenchState state,
  required WorkbenchTabSetSyncPlan plan,
}) {
  return state.copyWith(
    tabsByWorkspace: plan.tabsByWorkspace,
    layoutByWorkspace: plan.layoutByWorkspace,
    activeTabIdByWorkspace: plan.activeTabIdByWorkspace,
  );
}

WorkbenchTabSetSyncPlan planWorkbenchTabSetSync({
  required WorkbenchState state,
  required String workspaceId,
  required List<WorkspaceTabRecord> tabs,
  required bool layoutWasCleared,
}) {
  final liveTabIds = <String>{for (final tab in tabs) tab.id};
  final removedTabs = <WorkspaceTabRecord>[
    for (final tab in state.tabsFor(workspaceId))
      if (!liveTabIds.contains(tab.id)) tab,
  ];
  final tabsByWorkspace = applyWorkbenchTabsState(
    state: state,
    workspaceId: workspaceId,
    tabs: tabs,
  ).tabsByWorkspace;

  if (tabs.isEmpty && layoutWasCleared) {
    final layoutByWorkspace = Map<String, WorkbenchLayout>.from(
      state.layoutByWorkspace,
    )..remove(workspaceId);
    final activeTabIdByWorkspace = Map<String, String>.from(
      state.activeTabIdByWorkspace,
    )..remove(workspaceId);
    return WorkbenchTabSetSyncPlan(
      removedTabs: removedTabs,
      tabsByWorkspace: tabsByWorkspace,
      layoutByWorkspace: layoutByWorkspace,
      activeTabIdByWorkspace: activeTabIdByWorkspace,
      layoutToPersist: null,
      shouldLoadLayout: false,
    );
  }

  final currentLayout = state.layoutFor(workspaceId);
  if (currentLayout == null) {
    return WorkbenchTabSetSyncPlan(
      removedTabs: removedTabs,
      tabsByWorkspace: tabsByWorkspace,
      layoutByWorkspace: state.layoutByWorkspace,
      activeTabIdByWorkspace: state.activeTabIdByWorkspace,
      layoutToPersist: null,
      shouldLoadLayout: true,
    );
  }

  final layout = currentLayout.sanitize(tabs);
  final layoutByWorkspace = Map<String, WorkbenchLayout>.from(
    state.layoutByWorkspace,
  )..[workspaceId] = layout;
  final activeTabIdByWorkspace = Map<String, String>.from(
    state.activeTabIdByWorkspace,
  );
  final activeTabId = layout.activeTabId;
  if (activeTabId == null) {
    activeTabIdByWorkspace.remove(workspaceId);
  } else {
    activeTabIdByWorkspace[workspaceId] = activeTabId;
  }

  return WorkbenchTabSetSyncPlan(
    removedTabs: removedTabs,
    tabsByWorkspace: tabsByWorkspace,
    layoutByWorkspace: layoutByWorkspace,
    activeTabIdByWorkspace: activeTabIdByWorkspace,
    layoutToPersist: layout != currentLayout ? layout : null,
    shouldLoadLayout: false,
  );
}
