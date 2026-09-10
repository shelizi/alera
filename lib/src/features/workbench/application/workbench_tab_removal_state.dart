import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

WorkbenchState completeWorkbenchTabRemovalState({
  required WorkbenchState state,
  required String workspaceId,
  required List<WorkspaceTabRecord> remainingTabs,
}) {
  return state.copyWith(
    activeWorkspaceId:
        remainingTabs.isEmpty && state.activeWorkspaceId == workspaceId
        ? null
        : state.activeWorkspaceId,
    error: null,
  );
}
