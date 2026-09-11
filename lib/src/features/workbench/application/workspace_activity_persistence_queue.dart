import 'dart:async';

import 'package:alera/src/features/workbench/application/workspace_activity_repository.dart';

final class WorkspaceActivityPersistenceQueue {
  Future<void>? _tail;

  Future<void> upsertAll({
    required WorkspaceActivityRepository repository,
    required Map<String, DateTime> entries,
  }) {
    return _run(() => repository.upsertAll(entries));
  }

  Future<void> remove({
    required WorkspaceActivityRepository repository,
    required String workspaceId,
  }) {
    return _run(() => repository.remove(workspaceId));
  }

  Future<void> _run(Future<void> Function() action) async {
    final previous = _tail;
    final gate = Completer<void>();
    final gateFuture = gate.future;
    _tail = gateFuture;
    if (previous != null) {
      await previous;
    }
    try {
      await action();
    } finally {
      gate.complete();
      if (identical(_tail, gateFuture)) {
        _tail = null;
      }
    }
  }
}
