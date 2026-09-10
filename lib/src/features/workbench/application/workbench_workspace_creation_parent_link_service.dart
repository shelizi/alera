import 'package:alera/src/features/workbench/application/workspace_graph_repository.dart';
import 'package:alera/src/features/workbench/domain/workspace_creation_result.dart';

final class WorkbenchWorkspaceCreationParentLinkService {
  const WorkbenchWorkspaceCreationParentLinkService(this._repository);

  final WorkspaceParentRepository _repository;

  Future<WorkspaceCreationResult> attach({
    required WorkspaceCreationResult result,
    String? parentWorkspaceId,
  }) async {
    final parentId = parentWorkspaceId?.trim();
    if (parentId == null || parentId.isEmpty) {
      return result;
    }
    try {
      await _repository.linkWorkspaces(
        parentWorkspaceId: parentId,
        childWorkspaceId: result.workspace.id,
      );
      return result;
    } catch (error) {
      return WorkspaceCreationResult(
        workspace: result.workspace,
        setupReport: result.setupReport,
        parentLinkError: error.toString(),
        deferredSetupCommand: result.deferredSetupCommand,
      );
    }
  }
}
