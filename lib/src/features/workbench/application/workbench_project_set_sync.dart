import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

final class WorkbenchProjectSetSyncPlan {
  const WorkbenchProjectSetSyncPlan({
    required this.validProjectIds,
    required this.removedWorkspaces,
    required this.activeProjectId,
    required this.activeWorkspaceId,
    required this.workspacesByProject,
    required this.tabsByWorkspace,
    required this.layoutByWorkspace,
    required this.activeTabIdByWorkspace,
    required this.viewPrefs,
    required this.viewPrefsChanged,
  });

  final Set<String> validProjectIds;
  final List<Workspace> removedWorkspaces;
  final String? activeProjectId;
  final String? activeWorkspaceId;
  final Map<String, List<Workspace>> workspacesByProject;
  final Map<String, List<WorkspaceTabRecord>> tabsByWorkspace;
  final Map<String, WorkbenchLayout> layoutByWorkspace;
  final Map<String, String> activeTabIdByWorkspace;
  final WorkbenchViewPrefs viewPrefs;
  final bool viewPrefsChanged;
}

WorkbenchProjectSetSyncPlan planWorkbenchProjectSetSync({
  required WorkbenchState state,
  required List<Project> projects,
}) {
  final validProjectIds = <String>{for (final project in projects) project.id};
  final removedWorkspaces = <Workspace>[
    for (final entry in state.workspacesByProject.entries)
      if (!validProjectIds.contains(entry.key)) ...entry.value,
  ];
  final removedWorkspaceIds = <String>{
    for (final workspace in removedWorkspaces) workspace.id,
  };

  final prefs = state.viewPrefs;
  final collapsedProjectIds = prefs.collapsedProjectIds
      .where(validProjectIds.contains)
      .toSet();
  final selectedProjectIds = prefs.selectedProjectIds
      .where(validProjectIds.contains)
      .toSet();
  final sourceControlRoots = Map<String, String>.from(
    prefs.sourceControlRootByWorkspaceId,
  )..removeWhere((workspaceId, _) => removedWorkspaceIds.contains(workspaceId));
  final viewPrefsChanged =
      collapsedProjectIds.length != prefs.collapsedProjectIds.length ||
      selectedProjectIds.length != prefs.selectedProjectIds.length ||
      sourceControlRoots.length != prefs.sourceControlRootByWorkspaceId.length;
  final viewPrefs = viewPrefsChanged
      ? prefs.copyWith(
          collapsedProjectIds: collapsedProjectIds,
          selectedProjectIds: selectedProjectIds,
          sourceControlRootByWorkspaceId: sourceControlRoots,
        )
      : prefs;

  final workspacesByProject = <String, List<Workspace>>{
    for (final entry in state.workspacesByProject.entries)
      if (validProjectIds.contains(entry.key)) entry.key: entry.value,
  };
  final liveWorkspaceIds = <String>{
    for (final workspaces in workspacesByProject.values)
      for (final workspace in workspaces) workspace.id,
  };

  final activeProjectId =
      state.activeProjectId != null &&
          validProjectIds.contains(state.activeProjectId)
      ? state.activeProjectId
      : (projects.isNotEmpty ? projects.first.id : null);
  final activeProjectWorkspaces = activeProjectId == null
      ? const <Workspace>[]
      : workspacesByProject[activeProjectId] ?? const <Workspace>[];
  final activeWorkspaceId =
      state.activeWorkspaceId != null &&
          activeProjectWorkspaces.any(
            (workspace) => workspace.id == state.activeWorkspaceId,
          )
      ? state.activeWorkspaceId
      : null;

  return WorkbenchProjectSetSyncPlan(
    validProjectIds: validProjectIds,
    removedWorkspaces: removedWorkspaces,
    activeProjectId: activeProjectId,
    activeWorkspaceId: activeWorkspaceId,
    workspacesByProject: workspacesByProject,
    tabsByWorkspace: <String, List<WorkspaceTabRecord>>{
      for (final entry in state.tabsByWorkspace.entries)
        if (liveWorkspaceIds.contains(entry.key)) entry.key: entry.value,
    },
    layoutByWorkspace: <String, WorkbenchLayout>{
      for (final entry in state.layoutByWorkspace.entries)
        if (liveWorkspaceIds.contains(entry.key)) entry.key: entry.value,
    },
    activeTabIdByWorkspace: <String, String>{
      for (final entry in state.activeTabIdByWorkspace.entries)
        if (liveWorkspaceIds.contains(entry.key)) entry.key: entry.value,
    },
    viewPrefs: viewPrefs,
    viewPrefsChanged: viewPrefsChanged,
  );
}
