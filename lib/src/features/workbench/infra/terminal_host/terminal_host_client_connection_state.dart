part of 'terminal_host_client.dart';

/// Original Host connection admission, reuse, and capability policy.
extension _SocketTerminalHostConnectionState on SocketTerminalHostClient {
  void _throwIfConnectRetryCoolingDown() {
    final notBefore = _connectRetryNotBefore;
    if (notBefore != null && DateTime.now().isBefore(notBefore)) {
      throw const TerminalHostConnectionClosedException();
    }
  }

  Future<_TerminalHostConnection> _trackConnectOutcome(
    Future<_TerminalHostConnection> future,
  ) async {
    try {
      final connection = await future;
      _connectFailureStreak = 0;
      _connectRetryNotBefore = null;
      return connection;
    } catch (error) {
      // Semantic rejections such as a missing orchestration capability are
      // not transport failures and must not pace later connects.
      if (error is! StateError) {
        _connectFailureStreak += 1;
        if (_connectFailureStreak >= 2) {
          final factor = (_connectFailureStreak - 1).clamp(1, 4);
          _connectRetryNotBefore = DateTime.now().add(
            _connectRetryCooldown * factor,
          );
        }
      }
      rethrow;
    }
  }

  Future<_TerminalHostConnection> _connectTerminal() {
    if (_disposed) {
      throw const TerminalHostConnectionClosedException();
    }
    if (_terminalConnection case final connection?) {
      return Future<_TerminalHostConnection>.value(connection);
    }
    if (_runtimeConnection case final connection?
        when connection.supportsRuntime) {
      _terminalConnection = connection;
      return Future<_TerminalHostConnection>.value(connection);
    }
    final future = _terminalConnectionFuture;
    if (future != null) {
      return future;
    }
    _throwIfConnectRetryCoolingDown();
    late final Future<_TerminalHostConnection> next;
    next = _trackConnectOutcome(_openTerminalConnection()).whenComplete(() {
      if (identical(_terminalConnectionFuture, next)) {
        _terminalConnectionFuture = null;
      }
    });
    _terminalConnectionFuture = next;
    _guardHostFuture(next);
    return next;
  }

  Future<_TerminalHostConnection> _connectRuntimeImpl({
    bool requireOrchestration = false,
    bool launchIfMissing = true,
  }) async {
    if (_disposed) {
      throw const TerminalHostConnectionClosedException();
    }
    if (_runtimeConnection case final connection?
        when _supportsRuntime(connection, requireOrchestration)) {
      return Future<_TerminalHostConnection>.value(connection);
    }
    if (_terminalConnection case final connection?) {
      if (_supportsRuntime(connection, requireOrchestration)) {
        _runtimeConnection = connection;
        return Future<_TerminalHostConnection>.value(connection);
      }
      _throwIfOrchestrationWouldSplitPtyHost(connection, requireOrchestration);
    }
    final future = _runtimeConnectionFuture;
    if (future != null) {
      final connection = await _waitForRuntimeConnectionFuture(
        future,
        requireOrchestration: requireOrchestration,
      );
      if (connection != null &&
          _supportsRuntime(connection, requireOrchestration)) {
        return connection;
      }
    }
    final terminalFuture = _terminalConnectionFuture;
    if (terminalFuture != null) {
      final connection = await terminalFuture;
      if (_supportsRuntime(connection, requireOrchestration)) {
        _runtimeConnection = connection;
        return connection;
      }
      _throwIfOrchestrationWouldSplitPtyHost(connection, requireOrchestration);
    }
    if (_runtimeConnection case final connection?
        when _supportsRuntime(connection, requireOrchestration)) {
      return connection;
    }
    final nextFuture = _runtimeConnectionFuture;
    if (nextFuture != null) {
      final connection = await _waitForRuntimeConnectionFuture(
        nextFuture,
        requireOrchestration: requireOrchestration,
      );
      if (connection != null &&
          _supportsRuntime(connection, requireOrchestration)) {
        return connection;
      }
    }
    _throwIfConnectRetryCoolingDown();
    late final Future<_TerminalHostConnection> next;
    next =
        _trackConnectOutcome(
          _openRuntimeConnection(
            requireOrchestration: requireOrchestration,
            launchIfMissing: launchIfMissing,
          ),
        ).whenComplete(() {
          if (identical(_runtimeConnectionFuture, next)) {
            _runtimeConnectionFuture = null;
          }
        });
    _runtimeConnectionFuture = next;
    _guardHostFuture(next);
    return next;
  }

  Future<_TerminalHostConnection?> _waitForRuntimeConnectionFuture(
    Future<_TerminalHostConnection> future, {
    required bool requireOrchestration,
  }) async {
    try {
      return await future;
    } catch (_) {
      if (_disposed) {
        throw const TerminalHostConnectionClosedException();
      }
      if (requireOrchestration) {
        rethrow;
      }
      // A strict orchestration probe can reject a live pre-orchestration host;
      // normal runtime callers should retry without inheriting that error.
      return null;
    }
  }

  Future<_TerminalHostPaths> _runtimePaths() async {
    final support = await _applicationSupportDirectory();
    final runtimeDir = Directory(p.join(support.path, 'terminal_host'));
    if (!await runtimeDir.exists()) {
      await runtimeDir.create(recursive: true);
    }
    return _TerminalHostPaths(
      runtimeDir: runtimeDir,
      controlFile: File(p.join(runtimeDir.path, 'host.json')),
      runtimeControlFile: File(p.join(runtimeDir.path, 'runtime-host.json')),
    );
  }

  String _newToken() {
    final random = Random.secure();
    final bytes = List<int>.generate(32, (_) => random.nextInt(256));
    return base64UrlEncode(bytes);
  }

  bool _supportsRuntime(
    _TerminalHostConnection connection,
    bool requireOrchestration,
  ) {
    return connection.supportsRuntime &&
        (!requireOrchestration || connection.supportsOrchestration);
  }

  bool _controlSupportsRuntime(
    _TerminalHostControl? control,
    bool requireOrchestration,
  ) {
    return control?.supportsRuntime == true &&
        (!requireOrchestration || control!.supportsOrchestration);
  }

  void _throwIfOrchestrationWouldSplitPtyHost(
    _TerminalHostConnection connection,
    bool requireOrchestration,
  ) {
    if (requireOrchestration && !_supportsRuntime(connection, true)) {
      throw StateError(_orchestrationHostRestartRequiredMessage);
    }
  }
}
