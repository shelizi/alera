import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';

final class WorkbenchAddedProjectActivationPlan {
  const WorkbenchAddedProjectActivationPlan({
    required this.state,
    required this.viewPrefsChanged,
  });

  final WorkbenchState state;
  final bool viewPrefsChanged;
}

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

WorkbenchAddedProjectActivationPlan planWorkbenchAddedProjectActivation({
  required WorkbenchState state,
  required Project project,
}) {
  final prefs = state.viewPrefs;
  final wasCollapsed = prefs.collapsedProjectIds.contains(project.id);
  final nextPrefs = wasCollapsed
      ? prefs.copyWith(
          collapsedProjectIds: Set<String>.from(prefs.collapsedProjectIds)
            ..remove(project.id),
        )
      : prefs;
  return WorkbenchAddedProjectActivationPlan(
    state: state.copyWith(
      viewPrefs: nextPrefs,
      activeProjectId: project.id,
      activeWorkspaceId: null,
      error: null,
    ),
    viewPrefsChanged: wasCollapsed,
  );
}
