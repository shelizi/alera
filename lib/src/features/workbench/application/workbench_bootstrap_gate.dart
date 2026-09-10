import 'dart:async';

final class WorkbenchBootstrapGate {
  Future<void>? _future;

  Future<void> run(Future<void> Function() action) {
    final existing = _future;
    if (existing != null) {
      return existing;
    }

    final completer = Completer<void>();
    _future = completer.future;
    Future<void>.sync(action)
        .then((_) => completer.complete(), onError: completer.completeError);
    return completer.future;
  }
}
