import 'package:alera/src/features/workbench/application/workbench_layout_repository.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

final class WorkbenchLayoutResolver {
  const WorkbenchLayoutResolver(this._repository);

  final WorkbenchLayoutRepository _repository;

  Future<WorkbenchLayout> resolve({
    required String workspaceId,
    required List<WorkspaceTabRecord> tabs,
  }) async {
    final stored = await _repository.findWorkbenchLayout(workspaceId);
    final layout =
        stored ??
        WorkbenchLayout.single(
          workspaceId: workspaceId,
          tabIds: <String>[for (final tab in tabs) tab.id],
        );
    final sanitized = layout.sanitize(tabs);
    if (stored == null || sanitized != stored) {
      await _repository.upsertWorkbenchLayout(sanitized);
    }
    return sanitized;
  }
}
