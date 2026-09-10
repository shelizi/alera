import 'package:alera/src/features/workbench/application/workbench_workspace_tag_update_plan.dart';
import 'package:alera/src/features/workbench/application/workspace_graph_repository.dart';

final class WorkbenchWorkspaceTagUpdateService {
  const WorkbenchWorkspaceTagUpdateService(this._repository);

  final WorkspaceTagAssignmentRepository _repository;

  Future<void> apply({
    required String workspaceId,
    required WorkbenchWorkspaceTagUpdatePlan plan,
  }) async {
    for (final tagId in plan.tagIdsToRemove) {
      await _repository.unassignTag(workspaceId: workspaceId, tagId: tagId);
    }
    for (final tagId in plan.tagIdsToAdd) {
      await _repository.assignTag(workspaceId: workspaceId, tagId: tagId);
    }
  }
}
