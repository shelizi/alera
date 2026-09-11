import 'package:alera/src/features/workbench/application/workbench_serial_mutation_queue.dart';
import 'package:alera/src/features/workbench/application/workspace_activity_repository.dart';

final class WorkspaceActivityPersistenceQueue {
  final WorkbenchSerialMutationQueue _queue = WorkbenchSerialMutationQueue();

  Future<void> upsertAll({
    required WorkspaceActivityRepository repository,
    required Map<String, DateTime> entries,
  }) {
    return _queue.run(() => repository.upsertAll(entries));
  }

  Future<void> remove({
    required WorkspaceActivityRepository repository,
    required String workspaceId,
  }) {
    return _queue.run(() => repository.remove(workspaceId));
  }
}
