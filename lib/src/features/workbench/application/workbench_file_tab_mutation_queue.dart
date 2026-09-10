import 'dart:async';

final class WorkbenchFileTabMutationQueue {
  Future<void>? _tail;

  Future<T> run<T>(Future<T> Function() action) async {
    final previous = _tail;
    final gate = Completer<void>();
    final gateFuture = gate.future;
    _tail = gateFuture;
    if (previous != null) {
      await previous;
    }
    try {
      return await action();
    } finally {
      gate.complete();
      if (identical(_tail, gateFuture)) {
        _tail = null;
      }
    }
  }
}
