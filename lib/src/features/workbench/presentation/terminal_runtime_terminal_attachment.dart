part of 'terminal_runtime.dart';

/// Terminal instantiation, event routing, and clipboard attachment.
extension _XtermTerminalAttachment on _XtermTerminalSessionHandle {
  Future<void> _pasteFromClipboard() => _pasteTerminalClipboard(this);

  xterm.Terminal _createTerminal() =>
      _rendererAdapterOwner.createTerminal(settings: _settings);

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
    _ptySession?.writeBytes(utf8.encode(data));
  }

  void _handleTerminalOutput(String data) => _queueTerminalOutput(data);
}
