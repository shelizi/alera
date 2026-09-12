part of 'terminal_runtime.dart';

void _storeTerminalClipboard(_XtermTerminalSessionHandle handle, String text) {
  handle._launchInputOwner.storeClipboard(
    text: text,
    allowOsc52Clipboard: handle._settings.allowOsc52Clipboard,
    isDisposed: handle._disposed,
  );
}

void _pasteTerminalText(_XtermTerminalSessionHandle handle, String text) {
  if (handle._disposed || text.isEmpty) {
    return;
  }
  handle._terminal.paste(text);
}

Future<void> _pasteTerminalClipboard(_XtermTerminalSessionHandle handle) {
  return handle._launchInputOwner.pasteClipboard(
    isDisposed: handle._disposed,
    onPasteText: (text) => _pasteTerminalText(handle, text),
  );
}

void _handleTerminalSelectionChanged(_XtermTerminalSessionHandle handle) {
  handle._selectionCopyTimer?.cancel();
  handle._selectionCopyTimer = null;
  if (handle._disposed || !handle._settings.clipboardOnSelect) {
    return;
  }
  final selection = handle._terminalController.selection;
  if (selection == null) {
    return;
  }
  handle._selectionCopyTimer = Timer(const Duration(milliseconds: 100), () {
    if (handle._disposed || !handle._settings.clipboardOnSelect) {
      return;
    }
    final currentSelection = handle._terminalController.selection;
    if (currentSelection == null) {
      return;
    }
    final text = handle._terminal.buffer.getText(currentSelection);
    if (text.isEmpty) {
      return;
    }
    handle._launchInputOwner.storeClipboard(
      text: text,
      allowOsc52Clipboard: true,
      isDisposed: handle._disposed,
    );
  });
}

void _publishTerminalInteraction(
  _XtermTerminalSessionHandle handle,
  String message, {
  required bool error,
}) {
  handle._launchInputOwner.notifyInteraction(message, error: error);
}

xterm.Terminal _createSessionTerminal(_XtermTerminalSessionHandle handle) {
  return xterm.Terminal(
    reflowWithHiddenCursor: false,
    preserveOrphanCombiningMarks: true,
    allowITerm2ClipboardCapture: false,
    allowKittyClipboard: false,
    // An unset callback lets TerminalView install its system clipboard reader.
    onClipboardQuery: (_) => null,
    clipboardDecoder: decodeTerminalOsc52Payload,
    maxLines: handle._settings.scrollbackLines,
    platform: _xtermTargetPlatform,
    wordSeparators: _wordSeparatorsFromSettings(
      handle._settings.wordSeparators,
    ),
  );
}

void _attachSessionTerminal(
  _XtermTerminalSessionHandle handle,
  xterm.Terminal terminal,
) {
  terminal.onTitleChange = handle._handleTitleChanged;
  terminal.onOutput = handle._handleTerminalInput;
  terminal.onResize = handle._handleTerminalResize;
  terminal.onClipboardStore = (_, text) =>
      _storeTerminalClipboard(handle, text);
}

void _detachSessionTerminal(xterm.Terminal terminal) {
  terminal.onTitleChange = null;
  terminal.onOutput = null;
  terminal.onResize = null;
  terminal.onClipboardStore = null;
}
