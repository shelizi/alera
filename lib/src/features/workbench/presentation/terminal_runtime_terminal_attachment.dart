part of 'terminal_runtime.dart';

/// Terminal instantiation, event routing, and clipboard attachment.
extension _XtermTerminalAttachment on _XtermTerminalSessionHandle {
  Future<void> _pasteFromClipboard() => _pasteTerminalClipboard(this);

  xterm.Terminal _createTerminal({bool notificationsEnabled = false}) {
    if (_parserWorkerEnabled) {
      _parserWorkerFocused = _focusNode.hasFocus;
      return TerminalXtermReplicaTerminal(
        cols: 80,
        rows: 24,
        maxLines: _settings.scrollbackLines,
        platform: _xtermTargetPlatform,
        wordSeparators: _rendererAdapterOwner.resolveWordSeparators(
          _settings.wordSeparators,
        ),
        initialFocused: _parserWorkerFocused,
        onFocusStateChanged: (focused) => _parserWorkerFocused = focused,
        notificationsEnabled: notificationsEnabled,
      );
    }
    return _rendererAdapterOwner.createTerminal(
      settings: _settings,
      notificationsEnabled: notificationsEnabled,
    );
  }

  void _attachTerminal(xterm.Terminal terminal) {
    _rendererAdapterOwner.attachTerminal(
      terminal,
      onTitleChange: _handleTitleChanged,
      onOutput: _handleTerminalInput,
      onResize: _handleTerminalResize,
      onClipboardStore: (text) => _storeTerminalClipboard(this, text),
    );
  }

  void _handleTitleChanged(String title) {
    _title = title;
    _titleNotifier.value = displayTitle;
  }

  static const int _maxPendingTerminalInputLength = 16384;

  void _handleTerminalInput(String data) {
    traceTerminalInput(
      'pty units=${data.codeUnits} session=${_ptySession != null} pending=${_pendingTerminalInput.length}',
    );
    _pump.markInteractiveInput();
    final session = _ptySession;
    if (session == null) {
      _bufferPendingTerminalInput(data);
    } else if (data.isNotEmpty) {
      final accepted = session.writeBytes(utf8.encode(data));
      if (!accepted) {
        traceTerminalInput('ptyRejected units=${data.codeUnits}');
      }
    }
    // The emulator also answers terminal queries through this callback; an
    // unfocused terminal cannot be receiving the user's keys or paste.
    if (_focusNode.hasFocus) _onUserInput(_workspace.id);
  }

  void _bufferPendingTerminalInput(String data) {
    // Keystrokes that arrive while start() is still attaching used to die on
    // the null session, which is exactly how a detached window eats its first
    // typed character. Buffer them until the PTY session is assigned.
    if (data.isEmpty ||
        _pendingTerminalInputLength + data.length >
            _maxPendingTerminalInputLength) {
      return;
    }
    _pendingTerminalInputLength += data.length;
    _pendingTerminalInput.add(data);
  }

  void _flushPendingTerminalInput() {
    if (_pendingTerminalInput.isEmpty) {
      return;
    }
    final buffered = _pendingTerminalInput.join();
    _pendingTerminalInput.clear();
    _pendingTerminalInputLength = 0;
    final session = _ptySession;
    final accepted = session?.writeBytes(utf8.encode(buffered));
    traceTerminalInput(
      'ptyFlush units=${buffered.codeUnits} accepted=$accepted',
    );
  }

  void _handleTerminalOutput(String data) => _queueTerminalOutput(data);
}
