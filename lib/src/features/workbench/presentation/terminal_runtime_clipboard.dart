part of 'terminal_runtime.dart';

void _storeTerminalClipboard(_XtermTerminalSessionHandle handle, String text) {
  handle._launchInputOwner.storeClipboard(
    text: text,
    allowOsc52Clipboard: handle._settings.allowOsc52Clipboard,
    isDisposed: handle._disposed,
  );
}

Future<void> _pasteTerminalClipboard(_XtermTerminalSessionHandle handle) {
  return handle._launchInputOwner.pasteClipboard(
    isDisposed: handle._disposed,
    onPasteText: (text) => handle._launchInputOwner.pasteText(handle, text),
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
