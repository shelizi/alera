part of 'terminal_runtime.dart';

/// Workspace/tab synchronization and terminal settings application.
extension _XtermTerminalSessionSync on _XtermTerminalSessionHandle {
  _XtermTerminalSessionHandle sync({
    required Workspace workspace,
    required WorkspaceTabRecord tab,
  }) {
    final metadataChanged =
        _workspace.id != workspace.id ||
        _workspace.path != workspace.path ||
        _tab.id != tab.id ||
        _tab.title != tab.title ||
        _tab.hasManualTitle != tab.hasManualTitle;
    _workspace = workspace;
    _tab = tab;
    _syncTerminalPulseConfiguration(tab);
    _titleNotifier.value = displayTitle;
    if (metadataChanged) {
      // sync() is invoked from build(); defer the notification so listening
      // AnimatedBuilders are not marked dirty during the build phase.
      scheduleMicrotask(() {
        if (!_disposed) {
          _notifySessionListeners();
        }
      });
    }
    return this;
  }

  void applySettings(TerminalSettings settings) {
    final outputRefreshFpsChanged =
        _settings.outputRefreshFps != settings.outputRefreshFps;
    _settings = settings;
    if (outputRefreshFpsChanged) {
      _pump.outputRefreshFpsChanged();
    }
    if (!settings.clipboardOnSelect) {
      _selectionCopyTimer?.cancel();
      _selectionCopyTimer = null;
    } else {
      _handleSelectionChanged();
    }
    _notifySessionListeners();
  }
}
