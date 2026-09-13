part of 'terminal_runtime.dart';

/// View construction, link handling, and focus requests for a session handle.
extension _XtermTerminalSessionView on _XtermTerminalSessionHandle {
  xterm.TerminalView _buildTerminalView({
    required bool autofocus,
    FocusOnKeyEventCallback? onKeyEvent,
    required MouseCursor mouseCursor,
    void Function(TapUpDetails details, xterm.CellOffset offset)? onTapUp,
  }) {
    return _rendererAdapterOwner.buildTerminalView(
      terminal: _terminal,
      terminalViewKey: _terminalViewKey,
      controller: _terminalController,
      scrollController: _scrollController,
      focusNode: _focusNode,
      settings: _settings,
      autofocus: autofocus,
      onKeyEvent: onKeyEvent,
      mouseCursor: mouseCursor,
      onTapUp: onTapUp,
      onPaste: _pasteFromClipboard,
      onCopy: _launchInputOwner.clipboard.writeText,
    );
  }

  TerminalLinkRange? _linkAt(xterm.CellOffset offset) {
    return _rendererAdapterOwner.linkAt(terminal: _terminal, offset: offset);
  }

  Future<void> _openLink(Uri uri) {
    return _rendererAdapterOwner.openLink(uri);
  }

  void _requestFocusNow() {
    _rendererAdapterOwner.requestFocusNow(_focusNode, isDisposed: _disposed);
  }
}
