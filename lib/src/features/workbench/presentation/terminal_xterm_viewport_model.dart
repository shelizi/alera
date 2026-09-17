import 'terminal_xterm_worker.dart';

/// UI-isolate mirror of the xterm model owned by [TerminalXtermWorker].
///
/// The mirror never parses terminal output. Full repaints replace the complete
/// visible viewport while partial deltas replace only changed rows, preserving
/// row identity for renderers that want to skip untouched lines.
final class TerminalXtermViewportModel {
  int _cols = 0;
  int _rows = 0;
  int _cursorX = 0;
  int _cursorY = 0;
  bool _cursorVisible = true;
  bool _cursorKeys = false;
  bool _keypadKeys = false;
  bool _bracketedPaste = false;
  bool _focusEvents = false;
  bool _altScroll = false;
  int _mouseMode = 0;
  int _mouseReportMode = 0;
  int _scrollBack = 0;
  List<String> _effects = const <String>[];
  List<String> _rowTexts = const <String>[];
  List<List<TerminalXtermWorkerRenderCell>> _renderRows =
      const <List<TerminalXtermWorkerRenderCell>>[];

  int get cols => _cols;
  int get rows => _rows;
  int get cursorX => _cursorX;
  int get cursorY => _cursorY;
  bool get cursorVisible => _cursorVisible;
  bool get cursorKeys => _cursorKeys;
  bool get keypadKeys => _keypadKeys;
  bool get bracketedPaste => _bracketedPaste;
  bool get focusEvents => _focusEvents;
  bool get altScroll => _altScroll;
  int get mouseMode => _mouseMode;
  int get mouseReportMode => _mouseReportMode;
  int get scrollBack => _scrollBack;
  List<String> get effects => _effects;
  List<String> get rowTexts => _rowTexts;
  List<List<TerminalXtermWorkerRenderCell>> get renderRows => _renderRows;

  String rowText(int row) => _rowTexts[row];

  void apply(TerminalXtermWorkerDelta delta) {
    if (_renderRows.isEmpty || delta.fullRepaint) {
      if (!delta.fullRepaint) {
        throw StateError(
          'Terminal xterm viewport requires a full repaint for its first delta.',
        );
      }
      _rebuildFullViewport(delta);
    } else {
      if (delta.cols != _cols || delta.rows != _rows) {
        throw StateError(
          'Terminal xterm viewport dimensions changed without a full repaint.',
        );
      }
      _applyPartialRows(delta);
    }

    _cols = delta.cols;
    _rows = delta.rows;
    _cursorX = delta.cursorX;
    _cursorY = delta.cursorY;
    _cursorVisible = delta.cursorVisible;
    _cursorKeys = delta.cursorKeys;
    _keypadKeys = delta.keypadKeys;
    _bracketedPaste = delta.bracketedPaste;
    _focusEvents = delta.focusEvents;
    _altScroll = delta.altScroll;
    _mouseMode = delta.mouseMode;
    _mouseReportMode = delta.mouseReportMode;
    _scrollBack = delta.scrollBack;
    _effects = List<String>.unmodifiable(delta.effects);
  }

  void _rebuildFullViewport(TerminalXtermWorkerDelta delta) {
    final nextTexts = List<String?>.filled(delta.rows, null, growable: false);
    final nextRows = List<List<TerminalXtermWorkerRenderCell>?>.filled(
      delta.rows,
      null,
      growable: false,
    );
    for (final changed in delta.rowDeltas) {
      _validateRow(changed.row, delta.rows);
      nextTexts[changed.row] = changed.text;
      nextRows[changed.row] = List<TerminalXtermWorkerRenderCell>.unmodifiable(
        changed.cells,
      );
    }
    if (nextRows.any((row) => row == null) ||
        nextTexts.any((text) => text == null)) {
      throw StateError(
        'Terminal xterm full repaint did not include every viewport row.',
      );
    }
    _rowTexts = List<String>.unmodifiable(nextTexts.cast<String>());
    _renderRows = List<List<TerminalXtermWorkerRenderCell>>.unmodifiable(
      nextRows.cast<List<TerminalXtermWorkerRenderCell>>(),
    );
  }

  void _applyPartialRows(TerminalXtermWorkerDelta delta) {
    if (delta.rowDeltas.isEmpty) {
      return;
    }
    final nextTexts = List<String>.of(_rowTexts);
    final nextRows = List<List<TerminalXtermWorkerRenderCell>>.of(_renderRows);
    for (final changed in delta.rowDeltas) {
      _validateRow(changed.row, delta.rows);
      nextTexts[changed.row] = changed.text;
      nextRows[changed.row] = List<TerminalXtermWorkerRenderCell>.unmodifiable(
        changed.cells,
      );
    }
    _rowTexts = List<String>.unmodifiable(nextTexts);
    _renderRows = List<List<TerminalXtermWorkerRenderCell>>.unmodifiable(
      nextRows,
    );
  }

  void _validateRow(int row, int rowCount) {
    if (row < 0 || row >= rowCount) {
      throw StateError('Terminal xterm worker returned invalid row $row.');
    }
  }
}
