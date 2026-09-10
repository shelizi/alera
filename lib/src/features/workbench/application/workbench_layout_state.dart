import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';

WorkbenchState applyWorkbenchLayoutState({
  required WorkbenchState state,
  required WorkbenchLayout layout,
}) {
  final layouts = Map<String, WorkbenchLayout>.from(state.layoutByWorkspace)
    ..[layout.workspaceId] = layout;
  final activeTabs = Map<String, String>.from(state.activeTabIdByWorkspace);
  final activeTabId = layout.activeTabId;
  if (activeTabId == null) {
    activeTabs.remove(layout.workspaceId);
  } else {
    activeTabs[layout.workspaceId] = activeTabId;
  }
  return state.copyWith(
    layoutByWorkspace: layouts,
    activeTabIdByWorkspace: activeTabs,
  );
}
