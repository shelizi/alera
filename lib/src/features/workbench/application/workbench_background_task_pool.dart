import 'dart:async';
import 'dart:collection';

/// FIFO, instance-owned concurrency limit for best-effort background work.
/// Callers coalesce each owner's pending work before submitting it here.
/// Cancellation removes only queued work; a running callback keeps its slot
/// until it actually completes.
final class WorkbenchBackgroundTaskPool {
  WorkbenchBackgroundTaskPool({required this.maxConcurrent}) {
    if (maxConcurrent < 1) {
      throw ArgumentError.value(
        maxConcurrent,
        'maxConcurrent',
        'Must be positive',
      );
    }
  }

  final int maxConcurrent;
  final _queue = Queue<_BackgroundTask>();
  int _active = 0;
  bool _draining = false;

  Future<void> run(Object owner, Future<void> Function() action) {
    final task = _BackgroundTask(owner, action);
    _queue.addLast(task);
    _drain();
    return task.completer.future;
  }

  /// Complete queued submissions without starting their callbacks.
  /// The caller retains any dirty flag it needs to replay after suspension.
  bool cancelPending(Object owner) {
    var cancelled = false;
    _queue.removeWhere((task) {
      if (!identical(task.owner, owner)) return false;
      cancelled = true;
      task.completer.complete();
      return true;
    });
    return cancelled;
  }

  void _drain() {
    if (_draining) return;
    _draining = true;
    try {
      while (_active < maxConcurrent && _queue.isNotEmpty) {
        final task = _queue.removeFirst();
        _active += 1;
        unawaited(
          Future<void>.sync(task.action).then<void>(
            (_) => _finish(task),
            onError: (Object error, StackTrace stack) =>
                _finish(task, error, stack),
          ),
        );
      }
    } finally {
      _draining = false;
    }
  }

  void _finish(_BackgroundTask task, [Object? error, StackTrace? stack]) {
    _active -= 1;
    if (error == null) {
      task.completer.complete();
    } else {
      task.completer.completeError(error, stack);
    }
    _drain();
  }
}

final class _BackgroundTask {
  _BackgroundTask(this.owner, this.action);
  final Object owner;
  final Future<void> Function() action;
  final completer = Completer<void>();
}
