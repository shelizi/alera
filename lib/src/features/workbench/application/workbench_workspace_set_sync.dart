import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

WorkbenchState reconcileWorkbenchWorkspaceState({
  required WorkbenchState state,
  required String projectId,
  required Workspace workspace,
}) {
  final workspaces = List<Workspace>.from(state.workspacesFor(projectId));
  final index = workspaces.indexWhere((entry) => entry.id == workspace.id);
  if (index == -1) {
    workspaces.add(workspace);
  } else {
    workspaces[index] = workspace;
  }
  final workspacesByProject = Map<String, List<Workspace>>.from(
    state.workspacesByProject,
  )..[projectId] = workspaces;
  return state.copyWith(workspacesByProject: workspacesByProject);
}

final class WorkbenchWorkspaceSetSyncPlan {
  const WorkbenchWorkspaceSetSyncPlan({
    required this.liveWorkspaceIds,
    required this.removedWorkspaceIds,
    required this.workspacesByProject,
    required this.layoutByWorkspace,
    required this.tabsByWorkspace,
    required this.activeTabIdByWorkspace,
    required this.viewPrefs,
    required this.viewPrefsChanged,
    required this.activeProjectId,
    required this.activeWorkspaceId,
  });

  final Set<String> liveWorkspaceIds;
  final Set<String> removedWorkspaceIds;
  final Map<String, List<Workspace>> workspacesByProject;
  final Map<String, WorkbenchLayout> layoutByWorkspace;
  final Map<String, List<WorkspaceTabRecord>> tabsByWorkspace;
  final Map<String, String> activeTabIdByWorkspace;
  final WorkbenchViewPrefs viewPrefs;
  final bool viewPrefsChanged;
  final String activeProjectId;
  final String? activeWorkspaceId;
}

WorkbenchState applyWorkbenchWorkspaceSetSyncPlan({
  required WorkbenchState state,
  required WorkbenchWorkspaceSetSyncPlan plan,
}) {
  return state.copyWith(
    workspacesByProject: plan.workspacesByProject,
    viewPrefs: plan.viewPrefs,
    activeProjectId: plan.activeProjectId,
    activeWorkspaceId: plan.activeWorkspaceId,
    layoutByWorkspace: plan.layoutByWorkspace,
    tabsByWorkspace: plan.tabsByWorkspace,
    activeTabIdByWorkspace: plan.activeTabIdByWorkspace,
  );
}

WorkbenchWorkspaceSetSyncPlan planWorkbenchWorkspaceSetSync({
  required WorkbenchState state,
  required Project project,
  required List<Workspace> workspaces,
}) {
  final workspacesByProject = Map<String, List<Workspace>>.from(
    state.workspacesByProject,
  )..[project.id] = workspaces;
  final liveWorkspaceIds = <String>{
    for (final workspace in workspaces) workspace.id,
  };
  final previousWorkspaceIds = <String>{
    for (final workspace
        in state.workspacesByProject[project.id] ?? const <Workspace>[])
      workspace.id,
  };
  final removedWorkspaceIds = previousWorkspaceIds.difference(liveWorkspaceIds);

  final activeProjectId =
      state.activeProjectId != null &&
          state.projects.any(
            (candidate) => candidate.id == state.activeProjectId,
          )
      ? state.activeProjectId!
      : project.id;
  final activeProjectWorkspaces =
      workspacesByProject[activeProjectId] ?? const <Workspace>[];
  final activeWorkspaceId =
      state.activeWorkspaceId != null &&
          activeProjectWorkspaces.any(
            (workspace) => workspace.id == state.activeWorkspaceId,
          )
      ? state.activeWorkspaceId
      : null;

  final prefs = state.viewPrefs;
  final expandedWorkspaceIds = prefs.expandedWorkspaceIds
      .where((id) => !removedWorkspaceIds.contains(id))
      .toSet();
  final sourceControlRoots = Map<String, String>.from(
    prefs.sourceControlRootByWorkspaceId,
  )..removeWhere((workspaceId, _) => removedWorkspaceIds.contains(workspaceId));
  final viewPrefsChanged =
      expandedWorkspaceIds.length != prefs.expandedWorkspaceIds.length ||
      sourceControlRoots.length != prefs.sourceControlRootByWorkspaceId.length;
  final viewPrefs = viewPrefsChanged
      ? prefs.copyWith(
          expandedWorkspaceIds: expandedWorkspaceIds,
          sourceControlRootByWorkspaceId: sourceControlRoots,
        )
      : prefs;

  return WorkbenchWorkspaceSetSyncPlan(
    liveWorkspaceIds: liveWorkspaceIds,
    removedWorkspaceIds: removedWorkspaceIds,
    workspacesByProject: workspacesByProject,
    layoutByWorkspace: <String, WorkbenchLayout>{
      for (final entry in state.layoutByWorkspace.entries)
        if (!removedWorkspaceIds.contains(entry.key)) entry.key: entry.value,
    },
    tabsByWorkspace: <String, List<WorkspaceTabRecord>>{
      for (final entry in state.tabsByWorkspace.entries)
        if (!removedWorkspaceIds.contains(entry.key)) entry.key: entry.value,
    },
    activeTabIdByWorkspace: <String, String>{
      for (final entry in state.activeTabIdByWorkspace.entries)
        if (!removedWorkspaceIds.contains(entry.key)) entry.key: entry.value,
    },
    viewPrefs: viewPrefs,
    viewPrefsChanged: viewPrefsChanged,
    activeProjectId: activeProjectId,
    activeWorkspaceId: activeWorkspaceId,
  );
}
