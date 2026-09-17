part of 'terminal_runtime.dart';

/// Restore-progress bookkeeping for a session handle.
///
/// A restored snapshot is drained over several frames, so the surface needs
/// to know how far along it is to show something better than an empty
/// terminal.
extension _TerminalRestoreProgressTracking on _XtermTerminalSessionHandle {
  void _beginRestore(int totalChars) {
    _restoreGeneration += 1;
    _restoreTotalChars = totalChars;
    _restoreWrittenChars = 0;
    _restoreProgress.value = totalChars <= 0
        ? null
        : TerminalRestoreProgress(writtenChars: 0, totalChars: totalChars);
  }

  void _advanceRestore(int chars) {
    if (_restoreTotalChars <= 0) {
      return;
    }
    _restoreWrittenChars += chars;
    if (_restoreWrittenChars >= _restoreTotalChars) {
      _finishRestore();
      return;
    }
    _restoreProgress.value = TerminalRestoreProgress(
      writtenChars: _restoreWrittenChars,
      totalChars: _restoreTotalChars,
    );
  }

  /// Takes the restore overlay down and refreshes the rebuilt view next frame.
  ///
  /// Any path that empties the backlog without going through [_advanceRestore]
  /// has to call this, or the overlay covers the terminal for good: nothing
  /// else clears it short of another snapshot or disposal.
  /// Restore and PTY generations cancel the pulse after replacement or exit.
  void _finishRestore() {
    if (_restoreTotalChars <= 0) {
      return;
    }
    final restoreGeneration = _restoreGeneration;
    final ptyGeneration = _activePtyGeneration;
    _restoreTotalChars = 0;
    _restoreWrittenChars = 0;
    _restoreProgress.value = null;
    if (ptyGeneration == null) {
      return;
    }
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_disposed ||
          !_outputVisible ||
          _restoreGeneration != restoreGeneration ||
          _activePtyGeneration != ptyGeneration ||
          _restoreProgress.value != null) {
        return;
      }
      unawaited(refreshRendering());
    });
  }

  void _rebuildTerminalFromSnapshot(
    List<int> data, {
    required bool resetInteractionModes,
  }) {
    if (_disposed) {
      return;
    }
    _rebuildTerminalFromSnapshotText(
      const Utf8Decoder(allowMalformed: true).convert(data),
      resetInteractionModes: resetInteractionModes,
    );
  }

  void _rebuildTerminalFromSnapshotText(
    String restored, {
    required bool resetInteractionModes,
  }) {
    if (_disposed) {
      return;
    }
    _clearPendingTerminalOutput();
    _resetParserWorkerBackend();
    _terminalController.clearSelection();
    final previousTerminal = _terminal;
    final viewWidth = previousTerminal.viewWidth;
    final viewHeight = previousTerminal.viewHeight;
    _detachTerminal(previousTerminal);

    final nextTerminal = _createTerminal(notificationsEnabled: _outputVisible)
      ..resize(viewWidth, viewHeight);
    _terminal = nextTerminal;
    _attachTerminal(nextTerminal);
    _attachSearchTerminal(nextTerminal);
    previousTerminal.dispose();
    // Scrollback can reach the host's 10 MB cap. The production socket path
    // decodes it to text before it reaches the UI isolate; the legacy byte path
    // above remains for fallback adapters. Parsing is still frame-budgeted.
    _beginRestore(restored.length);
    _queueTerminalOutput(restored, source: .restore);
    if (resetInteractionModes) {
      _queueTerminalOutput(terminalInteractionModeReset, source: .control);
    }
  }
}
