import 'package:alera/src/features/workbench/application/workbench_layout_repository.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_mutation_queue.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';

final class WorkbenchSequencedLayoutRepository
    implements WorkbenchLayoutRepository {
  WorkbenchSequencedLayoutRepository(this._delegate);

  final WorkbenchLayoutRepository _delegate;
  final WorkbenchWorkspaceMutationQueue _writes =
      WorkbenchWorkspaceMutationQueue();

  @override
  Future<WorkbenchLayout?> findWorkbenchLayout(String workspaceId) {
    return _delegate.findWorkbenchLayout(workspaceId);
  }

  @override
  Future<WorkbenchLayout> upsertWorkbenchLayout(WorkbenchLayout layout) {
    return _writes.run(
      workspaceId: layout.workspaceId,
      action: () => _delegate.upsertWorkbenchLayout(layout),
    );
  }
}
