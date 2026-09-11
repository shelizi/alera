import 'package:alera/src/features/workbench/application/workbench_tab_subscription_registry.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

typedef WorkbenchRetiredWorkspaceCallback = void Function(String workspaceId);
typedef WorkbenchClearedLayoutsForgetter = void Function(
  Iterable<String> workspaceIds,
);
typedef WorkbenchWorkspaceLayoutBackgroundLoader = void Function(
  String workspaceId,
);
typedef WorkbenchWorkspaceTabsWatcher =
    Stream<List<WorkspaceTabRecord>> Function(String workspaceId);
typedef WorkbenchWorkspaceTabsChanged = void Function(
  String workspaceId,
  List<WorkspaceTabRecord> tabs,
);

final class WorkbenchWorkspaceTabSubscriptionCoordinator {
  const WorkbenchWorkspaceTabSubscriptionCoordinator(this._registry);

  final WorkbenchTabSubscriptionRegistry _registry;

  void sync({
    required String projectId,
    required List<Workspace> workspaces,
    required WorkbenchRetiredWorkspaceCallback cleanupRetiredWorkspace,
    required WorkbenchClearedLayoutsForgetter forgetClearedLayouts,
    required WorkbenchWorkspaceLayoutBackgroundLoader loadLayoutInBackground,
    required WorkbenchWorkspaceTabsWatcher watchTabs,
    required WorkbenchWorkspaceTabsChanged onTabsChanged,
  }) {
    final liveWorkspaceIds = <String>{
      for (final workspace in workspaces) workspace.id,
    };
    final removedWorkspaceIds = _registry
        .workspaceIdsForProject(projectId)
        .where((workspaceId) => !liveWorkspaceIds.contains(workspaceId))
        .toList(growable: false);

    for (final workspaceId in removedWorkspaceIds) {
      cleanupRetiredWorkspace(workspaceId);
    }
    for (final workspaceId in removedWorkspaceIds) {
      _registry.cancelWorkspace(workspaceId);
    }
    forgetClearedLayouts(removedWorkspaceIds);

    for (final workspace in workspaces) {
      if (_registry.contains(workspace.id)) {
        continue;
      }
      loadLayoutInBackground(workspace.id);
      _registry.watch(
        projectId: projectId,
        workspaceId: workspace.id,
        stream: watchTabs(workspace.id),
        onData: (tabs) => onTabsChanged(workspace.id, tabs),
      );
    }
  }
}
