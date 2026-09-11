import 'package:alera/src/features/workbench/application/workbench_serial_mutation_queue.dart';
import 'package:alera/src/features/workbench/application/workbench_view_prefs_repository.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';

final class WorkbenchViewPrefsPersistenceQueue {
  final WorkbenchSerialMutationQueue _queue = WorkbenchSerialMutationQueue();

  Future<void> save({
    required WorkbenchViewPrefsRepository repository,
    required WorkbenchViewPrefs prefs,
  }) {
    return _queue.run(() => repository.save(prefs));
  }
}
