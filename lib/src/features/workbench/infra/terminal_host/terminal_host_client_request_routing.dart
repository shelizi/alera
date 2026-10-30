part of 'terminal_host_client.dart';

/// Host request routing retains its exact connection, deadline, and idempotency contracts.
extension _SocketTerminalHostClientRequestRouting on SocketTerminalHostClient {
  Future<void> _terminateWithView(
    String sessionId, {
    String? viewLeaseId,
    String? expectedWorkspaceId,
    String? expectedTabId,
    String? expectedTransferMutationId,
  }) async {
    final clientMutationId = _pendingTerminateMutationIds.putIfAbsent(
      sessionId,
      () => const Uuid().v4(),
    );
    await _terminalRequest('terminate', <String, Object?>{
      'sessionId': sessionId,
      'clientMutationId': clientMutationId,
      if (expectedWorkspaceId != null)
        'expectedWorkspaceId': expectedWorkspaceId,
      if (expectedTabId != null) 'expectedTabId': expectedTabId,
      if (expectedTransferMutationId != null)
        'expectedTransferMutationId': expectedTransferMutationId,
      if (viewLeaseId != null) 'expectedViewLeaseId': viewLeaseId,
    });
    _pendingTerminateMutationIds.remove(sessionId);
  }

  Future<Object?> _terminalRequest(
    String type,
    Map<String, Object?> payload, {
    Duration? timeout,
  }) async {
    if (_disposed) {
      throw StateError('Terminal host client is disposed.');
    }
    _throwIfAppQuitInProgress();
    final connection = await _connectTerminal();
    _throwIfAppQuitInProgress();
    return _requestOnConnection(connection, type, payload, timeout: timeout);
  }

  Future<Object?> _runtimeRequest(
    String type,
    Map<String, Object?> payload,
    Duration? timeout,
  ) async {
    if (_disposed) {
      throw StateError('Terminal host client is disposed.');
    }
    _throwIfAppQuitInProgress();
    final connection = await _connectRuntime(
      requireOrchestration: type.startsWith('orchestration.'),
    );
    _throwIfAppQuitInProgress();
    final requestPayload =
        type == 'runtimeSettings.update' &&
            _runtimeSettingsRevision != null &&
            !payload.containsKey('expectedRevision')
        ? <String, Object?>{
            ...payload,
            'expectedRevision': _runtimeSettingsRevision,
          }
        : payload;
    try {
      final response = await _requestOnConnection(
        connection,
        type,
        requestPayload,
        timeout: timeout,
      );
      if (type == 'runtimeSettings.get' || type == 'runtimeSettings.update') {
        _rememberRuntimeSettingsRevision(response);
      }
      return response;
    } on TerminalHostConflictException catch (error, stackTrace) {
      if (type == 'runtimeSettings.update' &&
          error.code == _runtimeSettingsRevisionConflictCode) {
        await _refreshRuntimeSettingsRevision(
          connection,
          timeout: timeout,
          conflict: error,
          conflictStackTrace: stackTrace,
        );
      }
      rethrow;
    }
  }

  void _rememberRuntimeSettingsRevision(Object? response) {
    final revision = response is Map ? response['revision'] : null;
    _runtimeSettingsRevision = revision is num ? revision.toInt() : null;
  }

  Future<void> _refreshRuntimeSettingsRevision(
    _TerminalHostConnection connection, {
    required Duration? timeout,
    required TerminalHostConflictException conflict,
    required StackTrace conflictStackTrace,
  }) async {
    try {
      final response = await _requestOnConnection(
        connection,
        'runtimeSettings.get',
        const <String, Object?>{},
        timeout: timeout,
      );
      _rememberRuntimeSettingsRevision(response);
    } on Object catch (error, stackTrace) {
      Logger('SocketTerminalHostClient').warning(
        'could not refresh runtime settings after a revision conflict',
        error,
        stackTrace,
      );
      Logger('SocketTerminalHostClient').fine(
        'the original runtime settings conflict remains authoritative',
        conflict,
        conflictStackTrace,
      );
    }
  }

  Future<Object?> _requestOnConnection(
    _TerminalHostConnection connection,
    String type,
    Map<String, Object?> payload, {
    Duration? timeout,
    bool allowDuringAppQuit = false,
  }) {
    if (_disposed) {
      throw StateError('Terminal host client is disposed.');
    }
    if (_appQuitInProgress && !allowDuringAppQuit) {
      throw const TerminalHostConnectionClosedException();
    }
    return _sendTerminalHostRequestWithMutationRetry(
      this,
      connection,
      type,
      payload,
      timeout: timeout,
      allowDuringAppQuit: allowDuringAppQuit,
    );
  }
}
