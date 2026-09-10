import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

WorkbenchState applyWorkbenchSleepWorkspaceState({
  required WorkbenchState state,
  required String workspaceId,
}) {
  final tabsByWorkspace = Map<String, List<WorkspaceTabRecord>>.from(
    state.tabsByWorkspace,
  )..[workspaceId] = const <WorkspaceTabRecord>[];
  final layoutByWorkspace = Map<String, WorkbenchLayout>.from(
    state.layoutByWorkspace,
  )..remove(workspaceId);
  final activeTabIdByWorkspace = Map<String, String>.from(
    state.activeTabIdByWorkspace,
  )..remove(workspaceId);

  return state.copyWith(
    tabsByWorkspace: tabsByWorkspace,
    layoutByWorkspace: layoutByWorkspace,
    activeTabIdByWorkspace: activeTabIdByWorkspace,
    activeWorkspaceId: state.activeWorkspaceId == workspaceId
        ? null
        : state.activeWorkspaceId,
    error: null,
  );
}
