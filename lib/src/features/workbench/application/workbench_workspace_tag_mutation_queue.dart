import 'dart:async';

final class WorkbenchWorkspaceTagMutationQueue {
  final Map<String, Future<void>> _tails = <String, Future<void>>{};

  Future<T> run<T>({
    required String workspaceId,
    required Future<T> Function() action,
  }) async {
    final previous = _tails[workspaceId];
    final gate = Completer<void>();
    final gateFuture = gate.future;
    _tails[workspaceId] = gateFuture;
    if (previous != null) {
      await previous;
    }
    try {
      return await action();
    } finally {
      gate.complete();
      if (identical(_tails[workspaceId], gateFuture)) {
        _tails.remove(workspaceId);
      }
    }
  }
}
