part of 'terminal_host_client.dart';

/// Authenticated Host frames, pending request completion and socket close.
extension _SocketTerminalHostMessageDispatch on SocketTerminalHostClient {
  void _handleLine(_TerminalHostConnection connection, Object? line) {
    // The reader isolate parses; the fallback path hands over the raw line.
    final decoded = line is String ? jsonDecode(line) : line;
    final message = asTerminalHostMap(decoded, 'Terminal host message');
    if (message['event'] case final String event) {
      _handleEvent(event, asTerminalHostMap(message['payload'], 'event'));
      return;
    }
    final id = message['id'];
    if (id is! int) {
      return;
    }
    if (id == 0) {
      if (message['ok'] != true) {
        final error = StateError(
          (message['error'] as String?) ??
              'Terminal host authentication was rejected.',
        );
        connection.completeAuthenticationError(error);
        _handleConnectionClosed(connection, error, 'authentication_rejected');
      } else {
        connection.completeAuthentication();
      }
      return;
    }
    final pending = _pending.remove(id);
    final completer = pending?.completer;
    if (completer == null || completer.isCompleted) {
      return;
    }
    if (message['ok'] == true) {
      completer.complete(message['payload']);
    } else {
      completer.completeError(_terminalHostRequestError(message));
    }
  }

  void _handleConnectionClosedImpl(
    _TerminalHostConnection connection, [
    Object? error,
    String origin = 'unspecified',
  ]) {
    if (connection.isClosed) {
      return;
    }
    final wasTerminalConnection = identical(_terminalConnection, connection);
    final wasRuntimeConnection = identical(_runtimeConnection, connection);
    if ((wasTerminalConnection || wasRuntimeConnection) &&
        !_disposed &&
        !_appQuitInProgress) {
      // Never log a host frame, socket exception message, or RPC payload: the
      // runtime may contain user terminal output and authentication secrets.
      final detail = switch (error) {
        _TerminalHostSocketReaderFailure(:final stage) =>
          ' reader_stage=$stage',
        null => '',
        _ => ' error_type=${error.runtimeType}',
      };
      Logger(
        'SocketTerminalHostClient',
      ).warning('terminal host transport disconnected origin=$origin$detail');
    }
    final closedError = TerminalHostConnectionClosedException(error);
    _stopHeartbeatFor(connection);
    connection.completeAuthenticationError(closedError);
    if (wasTerminalConnection) {
      _terminalConnection = null;
      _terminalConnectionFuture = null;
      unawaited(_terminalLineSub?.cancel());
      _terminalLineSub = null;
    }
    if (wasTerminalConnection && !_disposed && !_appQuitInProgress) {
      _emitConnectionError(closedError);
    }
    if (wasRuntimeConnection) {
      CrashReporting.clearRuntimeContext();
      _runtimeConnection = null;
      _runtimeConnectionFuture = null;
      _runtimeSettingsRevision = null;
      unawaited(_runtimeLineSub?.cancel());
      _runtimeLineSub = null;
    }
    connection.dispose();
    final pendingIds = <int>[
      for (final entry in _pending.entries)
        if (identical(entry.value.connection, connection)) entry.key,
    ];
    for (final id in pendingIds) {
      final completer = _pending.remove(id)?.completer;
      if (completer != null && !completer.isCompleted) {
        final pendingError = _disposed || _appQuitInProgress
            ? const TerminalHostConnectionClosedException()
            : closedError;
        // Preserve the close site for reporters that receive this error
        // without an await; public callers observe the async wrapper stack.
        completer.completeError(pendingError, .current);
      }
    }
  }

  void _failPendingRequests(Object error) {
    final pending = _pending.values.toList(growable: false);
    _pending.clear();
    for (final request in pending) {
      if (!request.completer.isCompleted) {
        request.completer.completeError(error, .current);
      }
    }
  }
}
