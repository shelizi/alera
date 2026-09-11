import 'package:alera/src/features/workbench/application/workbench_serial_mutation_queue.dart';

final class WorkbenchFileTabMutationQueue {
  final WorkbenchSerialMutationQueue _delegate = WorkbenchSerialMutationQueue();

  Future<T> run<T>(Future<T> Function() action) => _delegate.run(action);
}
