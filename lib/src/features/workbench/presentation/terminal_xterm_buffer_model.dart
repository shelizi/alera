import 'terminal_xterm_worker.dart';

/// UI-isolate mirror of the complete active xterm buffer owned by
/// [TerminalXtermWorker].
///
/// Normal output applies exact changed rows plus an optional head trim. More
/// complex worker-side structural changes arrive as full repaints. Retained
/// row objects keep their identity so render/search layers can skip untouched
/// scrollback rows.
final class TerminalXtermBufferModel {
  int _revision = 0;
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
  List<TerminalXtermWorkerEffect> _effects =
      const <TerminalXtermWorkerEffect>[];
  List<String> _rowTexts = const <String>[];
  List<List<TerminalXtermWorkerRenderCell>> _renderRows =
      const <List<TerminalXtermWorkerRenderCell>>[];

  int get revision => _revision;
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
  int get bufferLength => _renderRows.length;
  List<TerminalXtermWorkerEffect> get effects => _effects;
  List<String> get rowTexts => _rowTexts;
  List<List<TerminalXtermWorkerRenderCell>> get renderRows => _renderRows;

  String rowText(int row) => _rowTexts[row];

  void apply(TerminalXtermWorkerBufferDelta delta) {
    if (delta.revision <= _revision) {
      throw StateError(
        'Terminal xterm buffer received stale revision ${delta.revision}; '
        'current revision is $_revision.',
      );
    }

    if (_renderRows.isEmpty || delta.fullRepaint) {
      if (!delta.fullRepaint) {
        throw StateError(
          'Terminal xterm buffer requires a full repaint for its first delta.',
        );
      }
      _rebuildFullBuffer(delta);
    } else {
      if (delta.cols != _cols || delta.rows != _rows) {
        throw StateError(
          'Terminal xterm buffer dimensions changed without a full repaint.',
        );
      }
      _applyPartialBuffer(delta);
    }

    _revision = delta.revision;
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
    _effects = List<TerminalXtermWorkerEffect>.unmodifiable(delta.effects);

    if (_renderRows.length != delta.bufferLength) {
      throw StateError(
        'Terminal xterm buffer length ${_renderRows.length} does not match '
        'worker length ${delta.bufferLength}.',
      );
    }
  }

  void _rebuildFullBuffer(TerminalXtermWorkerBufferDelta delta) {
    if (delta.trimStart != 0) {
      throw StateError(
        'Terminal xterm full repaint cannot trim existing rows.',
      );
    }
    final nextTexts = List<String?>.filled(
      delta.bufferLength,
      null,
      growable: false,
    );
    final nextRows = List<List<TerminalXtermWorkerRenderCell>?>.filled(
      delta.bufferLength,
      null,
      growable: false,
    );
    for (final changed in delta.rowDeltas) {
      _validateRow(changed.row, delta.bufferLength);
      nextTexts[changed.row] = changed.text;
      nextRows[changed.row] = _freezeCells(changed.cells);
    }
    if (nextRows.any((row) => row == null) ||
        nextTexts.any((text) => text == null)) {
      throw StateError(
        'Terminal xterm full buffer repaint did not include every row.',
      );
    }
    _rowTexts = List<String>.unmodifiable(nextTexts.cast<String>());
    _renderRows = List<List<TerminalXtermWorkerRenderCell>>.unmodifiable(
      nextRows.cast<List<TerminalXtermWorkerRenderCell>>(),
    );
  }

  void _applyPartialBuffer(TerminalXtermWorkerBufferDelta delta) {
    if (delta.trimStart < 0 || delta.trimStart > _renderRows.length) {
      throw StateError(
        'Terminal xterm buffer returned invalid head trim ${delta.trimStart}.',
      );
    }

    final nextTexts = List<String>.of(_rowTexts);
    final nextRows = List<List<TerminalXtermWorkerRenderCell>>.of(_renderRows);
    if (delta.trimStart > 0) {
      nextTexts.removeRange(0, delta.trimStart);
      nextRows.removeRange(0, delta.trimStart);
    }

    for (final changed in delta.rowDeltas) {
      _validateRow(changed.row, delta.bufferLength);
      if (changed.row > nextRows.length) {
        throw StateError(
          'Terminal xterm buffer delta skipped appended row ${nextRows.length}.',
        );
      }
      final frozenCells = _freezeCells(changed.cells);
      if (changed.row == nextRows.length) {
        nextTexts.add(changed.text);
        nextRows.add(frozenCells);
      } else {
        nextTexts[changed.row] = changed.text;
        nextRows[changed.row] = frozenCells;
      }
    }

    if (nextRows.length != delta.bufferLength) {
      throw StateError(
        'Terminal xterm partial delta produced ${nextRows.length} rows; '
        'expected ${delta.bufferLength}.',
      );
    }
    _rowTexts = List<String>.unmodifiable(nextTexts);
    _renderRows = List<List<TerminalXtermWorkerRenderCell>>.unmodifiable(
      nextRows,
    );
  }

  List<TerminalXtermWorkerRenderCell> _freezeCells(
    List<TerminalXtermWorkerRenderCell> cells,
  ) {
    return List<TerminalXtermWorkerRenderCell>.unmodifiable(cells);
  }

  void _validateRow(int row, int rowCount) {
    if (row < 0 || row >= rowCount) {
      throw StateError(
        'Terminal xterm worker returned invalid buffer row $row.',
      );
    }
  }
}
