import 'package:alera/src/features/workbench/application/workspace_graph_repository.dart';
import 'package:alera/src/features/workbench/application/workspace_service.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';

final class WorkbenchWorkspaceParentUpdateService {
  const WorkbenchWorkspaceParentUpdateService(this._repository);

  final WorkspaceParentRepository _repository;

  Future<bool> update({
    required Workspace workspace,
    String? parentWorkspaceId,
  }) async {
    final currentParentId = normalizeWorkbenchWorkspaceParentId(
      workspace.parentWorkspaceId,
    );
    final nextParentId = normalizeWorkbenchWorkspaceParentId(parentWorkspaceId);
    if (currentParentId == nextParentId) {
      return false;
    }

    if (nextParentId != null) {
      if (nextParentId == workspace.id) {
        throw WorkspaceException('A workspace cannot be its own parent');
      }
      final relations = await _repository.listRelations();
      if (workspaceDescendantIds(
        workspace.id,
        relations,
      ).contains(nextParentId)) {
        throw WorkspaceException('Cannot set a descendant workspace as parent');
      }
    }

    if (currentParentId != null) {
      await _repository.unlinkWorkspaces(
        parentWorkspaceId: currentParentId,
        childWorkspaceId: workspace.id,
      );
    }

    if (nextParentId == null) {
      return true;
    }

    try {
      await _repository.linkWorkspaces(
        parentWorkspaceId: nextParentId,
        childWorkspaceId: workspace.id,
      );
    } catch (error) {
      if (currentParentId != null) {
        try {
          await _repository.linkWorkspaces(
            parentWorkspaceId: currentParentId,
            childWorkspaceId: workspace.id,
          );
        } catch (restoreError) {
          throw WorkspaceException(
            'Workspace parent update failed: $error. '
            'Previous parent restore failed: $restoreError',
          );
        }
      }
      rethrow;
    }
    return true;
  }
}

String? normalizeWorkbenchWorkspaceParentId(String? value) {
  final trimmed = value?.trim();
  return trimmed == null || trimmed.isEmpty ? null : trimmed;
}
