import 'package:alera/src/features/workbench/application/workbench_workspace_mutation_queue.dart';

final class WorkbenchWorkspaceTagMutationQueue {
  final WorkbenchWorkspaceMutationQueue _delegate =
      WorkbenchWorkspaceMutationQueue();

  Future<T> run<T>({
    required String workspaceId,
    required Future<T> Function() action,
  }) {
    return _delegate.run(workspaceId: workspaceId, action: action);
  }
}
