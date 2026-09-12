part of 'terminal_runtime.dart';

extension _TerminalRecoveryState on _XtermTerminalSessionHandle {
  void _setTerminalHostError(Object error) {
    if (_disposed) {
      return;
    }
    final message = 'Terminal host unavailable: $error';
    if (_errorMessage == message) {
      return;
    }
    _errorMessage = message;
    _notifySessionListeners();
  }
}
