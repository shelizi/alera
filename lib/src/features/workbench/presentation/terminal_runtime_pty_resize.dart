part of 'terminal_runtime.dart';

/// PTY window resizing and debounce logic for a session handle.
extension _XtermTerminalPtyResize on _XtermTerminalSessionHandle {
  void _handleTerminalResize(
    int width,
    int height,
    int pixelWidth,
    int pixelHeight,
  ) {
    final size = _TerminalPtySize(
      cols: width,
      rows: height,
      cellWidthPx: pixelWidth,
      cellHeightPx: pixelHeight,
    );
    _pendingParserWorkerSize = size;
    _pendingPtySize = size;
    _pendingPtyResizeTimer ??= Timer(
      _ptyResizeDebounceDuration,
      _flushPendingPtyResize,
    );
  }

  void _flushPendingParserWorkerResize() {
    final size = _pendingParserWorkerSize;
    if (_disposed || size == null) {
      _pendingParserWorkerSize = null;
      return;
    }
    _pendingParserWorkerSize = null;
    _resizeParserWorker(
      cols: size.cols,
      rows: size.rows,
      pixelWidth: size.cellWidthPx,
      pixelHeight: size.cellHeightPx,
    );
  }

  void _flushPendingPtyResize() {
    _flushPendingParserWorkerResize();
    _pendingPtyResizeTimer?.cancel();
    _pendingPtyResizeTimer = null;
    final size = _pendingPtySize;
    final session = _ptySession;
    if (_disposed || size == null) {
      _pendingPtySize = null;
      return;
    }
    if (session == null) {
      return;
    }
    _pendingPtySize = null;
    session.resize(size.cols, size.rows, size.cellWidthPx, size.cellHeightPx);
  }
}
