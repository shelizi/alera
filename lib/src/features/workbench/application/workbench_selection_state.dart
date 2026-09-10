import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';

WorkbenchState selectWorkbenchWorkspace({
  required WorkbenchState state,
  required Project project,
  required Workspace workspace,
}) {
  return state.copyWith(
    activeProjectId: project.id,
    activeWorkspaceId: workspace.id,
    error: null,
  );
}

WorkbenchState activateWorkbenchProject({
  required WorkbenchState state,
  required Project project,
}) {
  return state.copyWith(
    activeProjectId: project.id,
    activeWorkspaceId: null,
    error: null,
  );
}
