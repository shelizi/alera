import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';

final class WorkbenchWorkspaceTagUpdatePlan {
  const WorkbenchWorkspaceTagUpdatePlan({
    required this.tagIdsToRemove,
    required this.tagIdsToAdd,
  });

  final Set<String> tagIdsToRemove;
  final Set<String> tagIdsToAdd;
}

Set<String> normalizeWorkbenchWorkspaceTagIds(Iterable<String> tagIds) {
  return tagIds.map((id) => id.trim()).where((id) => id.isNotEmpty).toSet();
}

WorkbenchWorkspaceTagUpdatePlan planWorkbenchWorkspaceTagUpdate({
  required WorkbenchState state,
  required Workspace workspace,
  required Set<String> requestedTagIds,
}) {
  Workspace? latest;
  for (final candidate in state.workspacesFor(workspace.projectId)) {
    if (candidate.id == workspace.id) {
      latest = candidate;
      break;
    }
  }
  final current = (latest ?? workspace).tagIds.toSet();
  final requested = normalizeWorkbenchWorkspaceTagIds(requestedTagIds);
  return WorkbenchWorkspaceTagUpdatePlan(
    tagIdsToRemove: current.difference(requested),
    tagIdsToAdd: requested.difference(current),
  );
}
