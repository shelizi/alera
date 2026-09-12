part of 'terminal_runtime.dart';

/// Delegates the app-foreground signal from [TerminalRuntimeViewBufferOwner]
/// to the per-session [_TerminalSessionVisibilityAccounting] object.
extension _TerminalBufferAccounting on _XtermTerminalSessionHandle {
  void setAppForeground(bool foreground) {
    _visibility.setAppForeground(foreground);
  }
}
