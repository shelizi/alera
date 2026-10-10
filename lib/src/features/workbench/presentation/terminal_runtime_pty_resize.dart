part of 'terminal_runtime.dart';

/// PTY window resizing and debounce logic for a session handle.
extension _XtermTerminalPtyResize on _XtermTerminalSessionHandle {
  /// The size the view asked for. A worker-backed replica adopts it only once
  /// the worker's repaint arrives, so its committed size can lag behind.
  int get _requestedCols => switch (_terminal) {
    final TerminalXtermReplicaTerminal replica => replica.requestedViewWidth,
    final terminal => terminal.viewWidth,
  };

  int get _requestedRows => switch (_terminal) {
    final TerminalXtermReplicaTerminal replica => replica.requestedViewHeight,
    final terminal => terminal.viewHeight,
  };

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

  /// Sends the pending view size to the parser worker.
  ///
  /// A stopped worker is only started for it when [startWorker] is set, which
  /// the paths that are about to start it anyway (output, reveal) pass. The
  /// PTY throttle does not, so resizing a hard-evicted tab stays cheap; the
  /// size is kept for whichever of those paths runs next.
  void _flushPendingParserWorkerResize({bool startWorker = false}) {
    final size = _pendingParserWorkerSize;
    if (_disposed || size == null) {
      _pendingParserWorkerSize = null;
      return;
    }
    if (_parserWorkerFuture == null && !startWorker) {
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
