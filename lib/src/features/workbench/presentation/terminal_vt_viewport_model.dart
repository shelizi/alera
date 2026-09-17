import 'terminal_vt_worker.dart';

/// UI-isolate mirror of the worker-owned visible VT viewport.
///
/// This model contains only immutable primitive-backed render cells. It does
/// not parse terminal bytes and does not own native handles. Partial updates
/// replace only the rows marked dirty by Ghostty, preserving object identity
/// for untouched rows so a renderer can cheaply skip them.
final class TerminalVtViewportModel {
  int _revision = 0;
  int _cols = 0;
  int _rows = 0;
  bool _cursorVisible = true;
  int? _cursorX;
  int? _cursorY;
  TerminalVtWorkerModes? _modes;
  List<List<TerminalVtWorkerRenderCell>> _renderRows =
      const <List<TerminalVtWorkerRenderCell>>[];

  int get revision => _revision;
  int get cols => _cols;
  int get rows => _rows;
  bool get cursorVisible => _cursorVisible;
  int? get cursorX => _cursorX;
  int? get cursorY => _cursorY;

  TerminalVtWorkerModes get modes {
    final value = _modes;
    if (value == null) {
      throw StateError('Terminal VT viewport has not received a delta yet.');
    }
    return value;
  }

  List<List<TerminalVtWorkerRenderCell>> get renderRows => _renderRows;

  String rowText(int row) {
    return _renderRows[row].map((cell) => cell.text).join();
  }

  void apply(TerminalVtWorkerDelta delta) {
    if (delta.revision <= _revision) {
      throw StateError(
        'Terminal VT viewport received stale revision ${delta.revision}; '
        'current revision is $_revision.',
      );
    }

    if (_renderRows.isEmpty || delta.fullRepaint) {
      if (!delta.fullRepaint) {
        throw StateError(
          'Terminal VT viewport requires a full repaint for its first delta.',
        );
      }
      _renderRows = _rebuildFullViewport(delta);
    } else {
      if (delta.cols != _cols || delta.rows != _rows) {
        throw StateError(
          'Terminal VT viewport dimensions changed without a full repaint.',
        );
      }
      final next = List<List<TerminalVtWorkerRenderCell>>.of(_renderRows);
      for (final changed in delta.changedRows) {
        _validateRow(changed.row, delta.rows);
        next[changed.row] = changed.renderCells;
      }
      _renderRows = List<List<TerminalVtWorkerRenderCell>>.unmodifiable(next);
    }

    _revision = delta.revision;
    _cols = delta.cols;
    _rows = delta.rows;
    _cursorVisible = delta.cursorVisible;
    _cursorX = delta.cursorX;
    _cursorY = delta.cursorY;
    _modes = delta.modes;
  }

  List<List<TerminalVtWorkerRenderCell>> _rebuildFullViewport(
    TerminalVtWorkerDelta delta,
  ) {
    final next = List<List<TerminalVtWorkerRenderCell>?>.filled(
      delta.rows,
      null,
      growable: false,
    );
    for (final changed in delta.changedRows) {
      _validateRow(changed.row, delta.rows);
      next[changed.row] = changed.renderCells;
    }
    if (next.any((row) => row == null)) {
      throw StateError(
        'Terminal VT full repaint did not include every viewport row.',
      );
    }
    return List<List<TerminalVtWorkerRenderCell>>.unmodifiable(
      next.cast<List<TerminalVtWorkerRenderCell>>(),
    );
  }

  void _validateRow(int row, int rowCount) {
    if (row < 0 || row >= rowCount) {
      throw StateError('Terminal VT worker returned invalid row $row.');
    }
  }
}
