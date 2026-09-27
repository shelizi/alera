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

  void _handleTerminalInput(String data) {
    _pump.markInteractiveInput();
    _ptySession?.writeBytes(utf8.encode(data));
    // The emulator also answers terminal queries through this callback; an
    // unfocused terminal cannot be receiving the user's keys or paste.
    if (_focusNode.hasFocus) _onUserInput(_workspace.id);
  }

  void _handleTerminalOutput(String data) => _queueTerminalOutput(data);
}
