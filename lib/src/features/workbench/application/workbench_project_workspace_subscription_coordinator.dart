import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_tab_subscription_registry.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_subscription_registry.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';

typedef WorkbenchProjectMetadataWatcherSync = void Function(Project project);
typedef WorkbenchProjectWorkspacesWatcher = Stream<List<Workspace>> Function(
  String projectId,
);
typedef WorkbenchProjectWorkspacesChanged = void Function(
  Project project,
  List<Workspace> workspaces,
);
typedef WorkbenchProjectMainWorkspaceBackgroundEnsurer = void Function(
  Project project,
);
typedef WorkbenchProjectClearedLayoutsForgetter = void Function(
  Iterable<String> workspaceIds,
);
typedef WorkbenchProjectMetadataWatchersPruner = void Function(
  Set<String> validProjectIds,
);

final class WorkbenchProjectWorkspaceSubscriptionCoordinator {
  const WorkbenchProjectWorkspaceSubscriptionCoordinator({
    required WorkbenchWorkspaceSubscriptionRegistry workspaceSubscriptions,
    required WorkbenchTabSubscriptionRegistry tabSubscriptions,
  }) : _workspaceSubscriptions = workspaceSubscriptions,
       _tabSubscriptions = tabSubscriptions;

  final WorkbenchWorkspaceSubscriptionRegistry _workspaceSubscriptions;
  final WorkbenchTabSubscriptionRegistry _tabSubscriptions;

  void sync({
    required List<Project> projects,
    required Set<String> validProjectIds,
    required WorkbenchProjectMetadataWatcherSync syncMetadataWatcher,
    required WorkbenchProjectWorkspacesWatcher watchWorkspaces,
    required WorkbenchProjectWorkspacesChanged onWorkspacesChanged,
    required WorkbenchProjectMainWorkspaceBackgroundEnsurer
    ensureMainWorkspaceInBackground,
    required WorkbenchProjectClearedLayoutsForgetter forgetClearedLayouts,
    required WorkbenchProjectMetadataWatchersPruner pruneMetadataWatchers,
  }) {
    for (final project in projects) {
      syncMetadataWatcher(project);
      if (_workspaceSubscriptions.contains(project.id)) {
        continue;
      }
      _workspaceSubscriptions.watch(
        projectId: project.id,
        stream: watchWorkspaces(project.id),
        onData: (workspaces) => onWorkspacesChanged(project, workspaces),
      );
      ensureMainWorkspaceInBackground(project);
    }

    final removedProjectIds = _workspaceSubscriptions.projectIds
        .where((projectId) => !validProjectIds.contains(projectId))
        .toList(growable: false);
    for (final projectId in removedProjectIds) {
      _workspaceSubscriptions.cancelProject(projectId);
      final removedWorkspaceIds = _tabSubscriptions.workspaceIdsForProject(
        projectId,
      );
      for (final workspaceId in removedWorkspaceIds) {
        _tabSubscriptions.cancelWorkspace(workspaceId);
      }
      forgetClearedLayouts(removedWorkspaceIds);
    }
    pruneMetadataWatchers(validProjectIds);
  }
}
