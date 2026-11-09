import 'dart:async';

/// Session-scoped fan-out for Host events, independent of Flutter and sockets.
///
/// One incoming event is delivered to one local session stream and the
/// optional global observer. Session teardown cannot leave a stale listener
/// attached when the Host later reuses the same opaque session identifier.
///
/// The transport owns this hub; it does not own or terminate Host PTYs.
final class TerminalHostEventHub<T extends Object> {
  TerminalHostEventHub({
    required void Function(T) onGlobalEvent,
    required T Function(String sessionId, Object error) onConnectionError,
  }) : _onGlobalEvent = onGlobalEvent,
       _onConnectionError = onConnectionError;

  final void Function(T) _onGlobalEvent;
  final T Function(String sessionId, Object error) _onConnectionError;
  final Map<String, StreamController<T>> _sessions =
      <String, StreamController<T>>{};
  bool _closed = false;

  Stream<T> eventsForSession(String sessionId) {
    if (_closed) {
      return Stream<T>.empty();
    }
    return _sessions
        .putIfAbsent(sessionId, StreamController<T>.broadcast)
        .stream;
  }

  void releaseSession(String sessionId) {
    final controller = _sessions.remove(sessionId);
    if (controller != null && !controller.isClosed) {
      unawaited(controller.close());
    }
  }

  void emit(String sessionId, T event) {
    if (_closed) {
      return;
    }
    _onGlobalEvent(event);
    final controller = _sessions[sessionId];
    if (controller != null && !controller.isClosed) {
      controller.add(event);
    }
  }

  /// Report only to sessions still registered at the disconnect boundary.
  void emitConnectionError(Object error) {
    if (_closed) {
      return;
    }
    for (final sessionId in _sessions.keys.toList(growable: false)) {
      emit(sessionId, _onConnectionError(sessionId, error));
    }
  }

  void close() {
    if (_closed) {
      return;
    }
    _closed = true;
    for (final controller in _sessions.values) {
      if (!controller.isClosed) {
        unawaited(controller.close());
      }
    }
    _sessions.clear();
  }
}
