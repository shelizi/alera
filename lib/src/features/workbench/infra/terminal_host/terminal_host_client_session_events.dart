part of 'terminal_host_client.dart';

/// Per-session event demultiplexing.
///
/// Every [TerminalHostPtySession] used to listen to the single global event
/// stream and discard everything addressed to another session, so one output
/// chunk cost a dispatch to every live terminal. Keeping one sink per session
/// makes that O(1).
mixin _TerminalHostClientSessionEvents {
  late final TerminalHostEventHub<TerminalHostEvent> _sessionEventHub =
      TerminalHostEventHub<TerminalHostEvent>(
        onGlobalEvent: (event) {
          if (!_globalEvents.isClosed) {
            _globalEvents.add(event);
          }
        },
        onConnectionError: (sessionId, error) =>
            TerminalHostErrorEvent(sessionId, error),
      );

  /// The connection-wide sink, kept alongside the per-session ones so a single
  /// call fans an event to both.
  StreamController<TerminalHostEvent> get _globalEvents;

  /// Runtime change events, which are not session-scoped.
  StreamController<RuntimeHostEvent> get _runtimeEventSink;

  Stream<TerminalHostEvent> eventsForSession(String sessionId) =>
      _sessionEventHub.eventsForSession(sessionId);

  void releaseSession(String sessionId) =>
      _sessionEventHub.releaseSession(sessionId);

  void _emitHostEvent(String sessionId, TerminalHostEvent event) =>
      _sessionEventHub.emit(sessionId, event);

  void _emitConnectionError(Object error) =>
      _sessionEventHub.emitConnectionError(error);

  void _closeSessionEvents() => _sessionEventHub.close();

  void _handleEvent(String event, Map<String, Object?> payload) {
    if (runtimeHostEventNames.contains(event) && !_runtimeEventSink.isClosed) {
      _runtimeEventSink.add(RuntimeHostEvent(event, payload));
    }
    final sessionId = payload['sessionId'];
    if (sessionId is! String || _globalEvents.isClosed) {
      return;
    }
    switch (event) {
      case 'output':
        // Only reached against a host without the binary capability.
        _emitHostEvent(
          sessionId,
          TerminalHostOutputEvent(
            sessionId,
            decodeTerminalHostBytes(payload['dataBase64']),
          ),
        );
      case 'outputResyncRequired':
        _emitHostEvent(
          sessionId,
          TerminalHostOutputResyncRequiredEvent(sessionId),
        );
      case 'exit':
        _emitHostEvent(
          sessionId,
          TerminalHostExitEvent(sessionId, (payload['exitCode'] as int?) ?? -1),
        );
      case 'error':
        _emitHostEvent(
          sessionId,
          TerminalHostErrorEvent(
            sessionId,
            payload['error'] ?? 'Unknown terminal host error.',
          ),
        );
      case 'terminalDriverChanged':
        _emitHostEvent(
          sessionId,
          TerminalHostDriverChangedEvent.fromPayload(sessionId, payload),
        );
      case 'terminalPulseChanged':
        _emitHostEvent(
          sessionId,
          TerminalHostPulseChangedEvent(sessionId, .fromJson(payload)),
        );
    }
  }
}
