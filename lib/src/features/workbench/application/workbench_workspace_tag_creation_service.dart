import 'package:alera/src/features/workbench/application/workspace_graph_repository.dart';
import 'package:alera/src/features/workbench/application/workspace_service.dart';

final class WorkbenchWorkspaceTagCreationService {
  const WorkbenchWorkspaceTagCreationService(this._repository);

  final WorkspaceTagRepository _repository;

  Future<WorkspaceTag> create(String name) async {
    final trimmed = name.trim();
    if (trimmed.isEmpty) {
      throw WorkspaceException('Tag name is required.');
    }

    final lowered = trimmed.toLowerCase();
    for (final tag in await _repository.listTags()) {
      if (tag.name.toLowerCase() == lowered) {
        return tag;
      }
    }

    return _repository.upsertTag(WorkspaceTag.create(name: trimmed));
  }
}
