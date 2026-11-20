part of 'workbench_listing.dart';

List<Workspace> _sortSidebarWorkspaces(
  List<Workspace> workspaces, {
  required WorkbenchSortBy sortBy,
  required List<String> manualOrder,
  required bool pinMainOnRecent,
  required AgentActivityRank? Function(Workspace) activityOf,
  required DateTime Function(Workspace) lastActivityOf,
}) {
  final sorted = List<Workspace>.from(workspaces);
  switch (sortBy) {
    case WorkbenchSortBy.manual:
      final positions = <String, int>{
        for (var i = 0; i < manualOrder.length; i++) manualOrder[i]: i,
      };
      sorted.sort(
        (a, b) =>
            _compareManualPositions(a.id, b.id, positions, a.name, b.name),
      );
    case WorkbenchSortBy.name:
      sorted.sort((a, b) {
        if (a.isMain != b.isMain) {
          return a.isMain ? -1 : 1;
        }
        return a.name.toLowerCase().compareTo(b.name.toLowerCase());
      });
    case WorkbenchSortBy.recent:
      sorted.sort((a, b) {
        if (pinMainOnRecent && a.isMain != b.isMain) {
          return a.isMain ? -1 : 1;
        }
        return lastActivityOf(b).compareTo(lastActivityOf(a));
      });
    case WorkbenchSortBy.activity:
      final subtreeActivity = aggregateAgentActivityBySubtree(
        workspaces: sorted.map(
          (workspace) =>
              (id: workspace.id, parentId: workspace.parentWorkspaceId),
        ),
        directActivityByWorkspaceId: <String, AgentActivityRank?>{
          for (final workspace in sorted) workspace.id: activityOf(workspace),
        },
      );
      sorted.sort(
        (a, b) => compareByAgentActivity(
          aActivity: subtreeActivity[a.id],
          aName: a.name,
          bActivity: subtreeActivity[b.id],
          bName: b.name,
        ),
      );
  }
  return sorted;
}

List<Project> _sortSidebarProjects(
  List<Project> projects, {
  required WorkbenchSortBy sortBy,
  required List<String> manualOrder,
  required Iterable<Workspace> Function(String projectId) workspacesFor,
  required bool Function(Project, Workspace) workspaceVisible,
  required AgentActivityRank? Function(Workspace) activityOf,
  required DateTime Function(Workspace) lastActivityOf,
}) {
  final sorted = List<Project>.from(projects);
  switch (sortBy) {
    case WorkbenchSortBy.manual:
      final positions = <String, int>{
        for (var i = 0; i < manualOrder.length; i++) manualOrder[i]: i,
      };
      sorted.sort(
        (a, b) =>
            _compareManualPositions(a.id, b.id, positions, a.name, b.name),
      );
    case WorkbenchSortBy.name:
      sorted.sort(
        (a, b) => a.name.toLowerCase().compareTo(b.name.toLowerCase()),
      );
    case WorkbenchSortBy.recent:
      final latest = <String, DateTime>{};
      for (final project in sorted) {
        var value = project.updatedAt;
        for (final workspace in workspacesFor(project.id)) {
          if (!workspaceVisible(project, workspace)) {
            continue;
          }
          final at = lastActivityOf(workspace);
          if (at.isAfter(value)) value = at;
        }
        latest[project.id] = value;
      }
      sorted.sort((a, b) => latest[b.id]!.compareTo(latest[a.id]!));
    case WorkbenchSortBy.activity:
      final rank = <String, AgentActivityRank?>{};
      for (final project in sorted) {
        AgentActivityRank? best;
        for (final workspace in workspacesFor(project.id)) {
          if (!workspaceVisible(project, workspace)) {
            continue;
          }
          best = bestAgentActivityRank(best, activityOf(workspace));
        }
        rank[project.id] = best;
      }
      sorted.sort(
        (a, b) => compareByAgentActivity(
          aActivity: rank[a.id],
          aName: a.name,
          bActivity: rank[b.id],
          bName: b.name,
        ),
      );
  }
  return sorted;
}

int _compareManualPositions(
  String leftId,
  String rightId,
  Map<String, int> positions,
  String leftName,
  String rightName,
) {
  final left = positions[leftId];
  final right = positions[rightId];
  if (left != null && right != null) return left.compareTo(right);
  if (left != null) return -1;
  if (right != null) return 1;
  final byName = leftName.toLowerCase().compareTo(rightName.toLowerCase());
  return byName != 0 ? byName : leftId.compareTo(rightId);
}
