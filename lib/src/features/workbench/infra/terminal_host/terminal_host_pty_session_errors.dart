part of 'terminal_host_pty_session.dart';

bool _shouldRecoverFromTerminalHostError(Object error) {
  final message = _terminalHostErrorMessage(error);
  return message.contains('Terminal session is not attached') ||
      message.contains('Terminal host connection closed');
}

String _terminalHostErrorMessage(Object error) {
  if (error is StateError) {
    return error.message;
  }
  return error.toString();
}

extension _TerminalHostPtySessionErrors on TerminalHostPtySession {
  bool _shouldRecoverFromHostError(Object error) {
    return _shouldRecoverFromTerminalHostError(error);
  }

  bool _isDefinitivelyNotAttached(Object error) {
    return _terminalHostErrorMessage(error)
        .contains('Terminal session is not attached');
  }

  void _emitHostError(Object error) {
    if (!_disposed && !_events.isClosed && !_isInputBackpressure(error)) {
      _events.add(TerminalPtyErrorEvent(error));
    }
  }

  bool _isInputBackpressure(Object error) {
    return _terminalHostErrorMessage(error)
        .contains('terminal_input_backpressure');
  }

  bool _isTransportClosed(Object error) {
    return _terminalHostErrorMessage(error)
        .contains('Terminal host connection closed');
  }

  void _onHostSessionError(Object error) {
    if (_isInputBackpressure(error)) {
      return;
    }
    if (_isTransportClosed(error)) {
      unawaited(_recoverClosedTransport(error));
      return;
    }
    _emitHostError(error);
  }

  /// Fire-and-forget input. A dropped socket must not replay the bytes: the
  /// host may already have applied them, and a second write would duplicate
  /// the keystroke. "Not attached" still replays, because that session never
  /// received the write.
  Future<void> _sendBytesAcrossTransportLoss(
    Future<void> Function() write,
  ) async {
    try {
      await _withReattach(write, shouldRecover: _isDefinitivelyNotAttached);
    } catch (error) {
      if (!_isTransportClosed(error)) {
        rethrow;
      }
      await _recoverClosedTransport(error);
    }
  }

  Future<void> _recoverClosedTransport(Object error) {
    final pending = _transportRecovery;
    if (pending != null) {
      return pending;
    }
    if (_transportRecoveryFailures >= _transportRecoveryLimit) {
      _emitHostError(error);
      return Future<void>.value();
    }
    late final Future<void> recovery;
    recovery = _attemptTransportRecovery(error).whenComplete(() {
      if (identical(_transportRecovery, recovery)) {
        _transportRecovery = null;
      }
    });
    _transportRecovery = recovery;
    return recovery;
  }

  /// Same-session attach clears the failure count inside `_reattach`. A
  /// replacement shell means the old PTY is gone, so further createOrAttach
  /// calls would keep spawning shells.
  Future<void> _attemptTransportRecovery(Object error) async {
    while (_transportRecoveryFailures < _transportRecoveryLimit) {
      if (_disposed) {
        return;
      }
      final failure = _transportRecoveryFailures;
      if (failure > 0) {
        await Future.pause(Duration(milliseconds: 40 * failure));
        if (_disposed) {
          return;
        }
      }
      _transportRecoveryFailures = failure + 1;
      try {
        final attachment = await _reattach();
        if (attachment.created) {
          _transportRecoveryFailures = _transportRecoveryLimit;
          _emitHostError(error);
        }
        return;
      } catch (reattachError) {
        error = reattachError;
      }
    }
    _emitHostError(error);
  }
}

const _transportRecoveryLimit = 3;
