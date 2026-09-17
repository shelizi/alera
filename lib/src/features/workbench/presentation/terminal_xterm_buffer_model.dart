import 'package:xterm2/core.dart';

import 'terminal_search_source.dart';
import 'terminal_xterm_worker.dart';

final class _TerminalXtermBufferSearchLineId {
  const _TerminalXtermBufferSearchLineId({
    required this.generation,
    required this.absoluteIndex,
  });

  final Object generation;
  final int absoluteIndex;
}

/// UI-isolate mirror of the complete active xterm buffer owned by
/// [TerminalXtermWorker].
///
/// Normal output applies exact changed rows plus an optional head trim. More
/// complex worker-side structural changes arrive as full repaints. Retained
/// row objects keep their identity so render/search layers can skip untouched
/// scrollback rows.
final class TerminalXtermBufferModel implements TerminalSearchSource {
  TerminalXtermBufferModel({Set<int>? wordSeparators})
    : _wordSeparators = Set<int>.unmodifiable(
        wordSeparators ?? Buffer.defaultWordSeparators,
      );

  final Set<int> _wordSeparators;
  final Set<TerminalSearchSourceListener> _searchListeners =
      <TerminalSearchSourceListener>{};
  Object _searchGeneration = Object();
  int _searchLineBase = 0;
  List<_TerminalXtermBufferSearchLineId> _searchLineIds =
      const <_TerminalXtermBufferSearchLineId>[];
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
  TerminalXtermWorkerGlobalState? _globalState;
  final Map<int, String> _hyperlinks = <int, String>{};
  List<TerminalXtermWorkerEffect> _effects =
      const <TerminalXtermWorkerEffect>[];
  List<String> _rowTexts = const <String>[];
  List<List<TerminalXtermWorkerRenderCell>> _renderRows =
      const <List<TerminalXtermWorkerRenderCell>>[];
  List<bool> _wrappedRows = const <bool>[];
  List<int> _semanticPromptLines = <int>[];

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
  @override
  int get height => bufferLength;
  @override
  int get viewWidth => _cols;
  @override
  int get viewHeight => _rows;
  @override
  Object get bufferIdentity => _searchGeneration;
  TerminalXtermWorkerGlobalState get globalState =>
      _globalState ??
      (throw StateError('Terminal xterm global state is not initialized.'));
  bool get isUsingAltBuffer => globalState.isUsingAltBuffer;
  bool get reverseDisplay => globalState.reverseDisplay;
  int? get cursorType => globalState.cursorType;
  bool get cursorBlink => globalState.cursorBlink;
  bool get cursorLineHighlight => globalState.cursorLineHighlight;
  bool get mouseShiftCapture => globalState.mouseShiftCapture;
  bool get altEscPrefix => globalState.altEscPrefix;
  bool get altSendsEscape => globalState.altSendsEscape;
  bool get lineFeedMode => globalState.lineFeedMode;
  bool get ignoreKeypadWithNumLockMode =>
      globalState.ignoreKeypadWithNumLockMode;
  bool get backarrowKeyMode => globalState.backarrowKeyMode;
  int get kittyKeyboardMode => globalState.kittyKeyboardMode;
  int get modifyOtherKeysMode => globalState.modifyOtherKeysMode;
  bool get keyboardActionMode => globalState.keyboardActionMode;
  int get colorRevision => globalState.colorRevision;
  Map<int, int> get indexedColorOverrides => globalState.indexedColorOverrides;
  Map<int, int> get specialColorOverrides => globalState.specialColorOverrides;
  int? get foregroundColorOverride => globalState.foregroundColorOverride;
  int? get backgroundColorOverride => globalState.backgroundColorOverride;
  int? get cursorColorOverride => globalState.cursorColorOverride;
  int? get selectionColorOverride => globalState.selectionColorOverride;
  int? get selectionForegroundColorOverride =>
      globalState.selectionForegroundColorOverride;
  List<TerminalXtermWorkerEffect> get effects => _effects;
  List<String> get rowTexts => _rowTexts;
  List<List<TerminalXtermWorkerRenderCell>> get renderRows => _renderRows;

  String rowText(int row) => _rowTexts[row];
  bool isWrapped(int row) => _wrappedRows[row];

  @override
  Object lineIdAt(int index) => _searchLineIds[index];

  @override
  String lineTextAt(int index) => _rowTexts[index];

  @override
  int? lineIndexOf(Object lineId) {
    if (lineId is! _TerminalXtermBufferSearchLineId ||
        !identical(lineId.generation, _searchGeneration)) {
      return null;
    }
    final index = lineId.absoluteIndex - _searchLineBase;
    if (index < 0 || index >= _searchLineIds.length) return null;
    return identical(_searchLineIds[index], lineId) ? index : null;
  }

  @override
  void addListener(TerminalSearchSourceListener listener) {
    _searchListeners.add(listener);
  }

  @override
  void removeListener(TerminalSearchSourceListener listener) {
    _searchListeners.remove(listener);
  }

  int hyperlinkIdAt(int row, int column) {
    if (row < 0 || row >= _renderRows.length) return 0;
    final cells = _renderRows[row];
    if (column < 0 || column >= cells.length) return 0;
    return cells[column].hyperlinkId;
  }

  String? hyperlinkAt(int row, int column) {
    final hyperlinkId = hyperlinkIdAt(row, column);
    if (hyperlinkId == 0) return null;
    return _hyperlinks[hyperlinkId];
  }

  BufferRangeLine? getWordBoundary(CellOffset position) {
    if (position.y < 0 || position.y >= _renderRows.length) return null;

    var startLine = position.y;
    var start = position.x;
    var endLine = position.y;
    var end = position.x;

    do {
      if (start == 0) {
        if (!_lineContinuesFromPrevious(startLine)) break;
        startLine--;
        start = _cols;
      }
      var previous = start - 1;
      if (previous > 0 &&
          _cellWidth(startLine, previous) == 0 &&
          _cellWidth(startLine, previous - 1) == 2) {
        previous--;
      }
      final char = _codePoint(startLine, previous);
      if (_wordSeparators.contains(char)) break;
      start = previous;
    } while (true);

    do {
      if (end >= _cols) {
        if (!_lineContinuesToNext(endLine)) break;
        endLine++;
        end = 0;
      }
      final width = _cellWidth(endLine, end);
      if (width == 0 && end > 0 && _cellWidth(endLine, end - 1) == 2) {
        end++;
        continue;
      }
      final char = _codePoint(endLine, end);
      if (_wordSeparators.contains(char)) break;
      end += switch (width) {
        2 => 2,
        _ => 1,
      };
    } while (true);

    return BufferRangeLine(
      CellOffset(start, startLine),
      CellOffset(end, endLine),
    );
  }

  BufferRangeLine? getLineBoundary(CellOffset position) {
    if (position.y < 0 || position.y >= _renderRows.length) return null;

    var startLine = position.y;
    while (_lineContinuesFromPrevious(startLine)) {
      startLine--;
    }

    var endLine = position.y;
    while (_lineContinuesToNext(endLine)) {
      endLine++;
    }

    while (startLine < endLine && _lineHasOnlyWhitespace(startLine)) {
      startLine++;
    }
    while (endLine > startLine && _lineHasOnlyWhitespace(endLine)) {
      endLine--;
    }

    final startColumn = _firstNonWhitespaceColumn(startLine);
    final endColumn = _lastNonWhitespaceColumnEnd(endLine);
    if (startColumn == _cols && endColumn == 0) {
      return BufferRangeLine(
        CellOffset(0, startLine),
        CellOffset(_cols, endLine),
      );
    }
    return BufferRangeLine(
      CellOffset(startColumn, startLine),
      CellOffset(endColumn, endLine),
    );
  }

  bool isSemanticPromptLine(int line) {
    final index = _lowerBound(_semanticPromptLines, line);
    return index < _semanticPromptLines.length &&
        _semanticPromptLines[index] == line;
  }

  int? semanticPromptLineBefore(int line) {
    final index = _lowerBound(_semanticPromptLines, line) - 1;
    return index >= 0 ? _semanticPromptLines[index] : null;
  }

  int? semanticPromptLineAfter(int line) {
    var index = _lowerBound(_semanticPromptLines, line);
    if (index < _semanticPromptLines.length &&
        _semanticPromptLines[index] == line) {
      index++;
    }
    return index < _semanticPromptLines.length
        ? _semanticPromptLines[index]
        : null;
  }

  void applyState(TerminalXtermWorkerStateDelta delta) {
    if (delta.revision <= _revision) {
      throw StateError(
        'Terminal xterm buffer received stale revision ${delta.revision}; '
        'current revision is $_revision.',
      );
    }
    if (_renderRows.isNotEmpty &&
        (delta.cols != _cols || delta.rows != _rows)) {
      throw StateError(
        'Terminal xterm hidden state changed dimensions without a buffer repaint.',
      );
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
    _globalState = delta.globalState;
  }

  void apply(TerminalXtermWorkerBufferDelta delta) {
    if (delta.revision <= _revision) {
      throw StateError(
        'Terminal xterm buffer received stale revision ${delta.revision}; '
        'current revision is $_revision.',
      );
    }

    final searchChanged = _searchChangedBy(delta);
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

    if (delta.fullRepaint) {
      _hyperlinks.clear();
    }
    _hyperlinks.addAll(delta.hyperlinkUpdates);
    _globalState = delta.globalState;

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
    if (searchChanged && _searchListeners.isNotEmpty) {
      for (final listener in _searchListeners.toList(growable: false)) {
        listener();
      }
    }
  }

  bool _searchChangedBy(TerminalXtermWorkerBufferDelta delta) {
    if (delta.fullRepaint || delta.trimStart > 0) return true;
    for (final changed in delta.rowDeltas) {
      if (changed.row >= _rowTexts.length ||
          _rowTexts[changed.row] != changed.text) {
        return true;
      }
    }
    return false;
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
    final nextWrapped = List<bool?>.filled(
      delta.bufferLength,
      null,
      growable: false,
    );
    for (final changed in delta.rowDeltas) {
      _validateRow(changed.row, delta.bufferLength);
      nextTexts[changed.row] = changed.text;
      nextRows[changed.row] = _freezeCells(changed.cells);
      nextWrapped[changed.row] = changed.isWrapped;
    }
    if (nextRows.any((row) => row == null) ||
        nextTexts.any((text) => text == null) ||
        nextWrapped.any((wrapped) => wrapped == null)) {
      throw StateError(
        'Terminal xterm full buffer repaint did not include every row.',
      );
    }
    _rowTexts = List<String>.unmodifiable(nextTexts.cast<String>());
    _renderRows = List<List<TerminalXtermWorkerRenderCell>>.unmodifiable(
      nextRows.cast<List<TerminalXtermWorkerRenderCell>>(),
    );
    _wrappedRows = List<bool>.unmodifiable(nextWrapped.cast<bool>());
    _semanticPromptLines = <int>[
      for (final changed in delta.rowDeltas)
        if (changed.isSemanticPromptLine) changed.row,
    ];
    _searchGeneration = Object();
    _searchLineBase = 0;
    _searchLineIds = <_TerminalXtermBufferSearchLineId>[
      for (var row = 0; row < delta.bufferLength; row++)
        _TerminalXtermBufferSearchLineId(
          generation: _searchGeneration,
          absoluteIndex: row,
        ),
    ];
  }

  void _applyPartialBuffer(TerminalXtermWorkerBufferDelta delta) {
    if (delta.trimStart < 0 || delta.trimStart > _renderRows.length) {
      throw StateError(
        'Terminal xterm buffer returned invalid head trim ${delta.trimStart}.',
      );
    }

    final nextTexts = List<String>.of(_rowTexts);
    final nextRows = List<List<TerminalXtermWorkerRenderCell>>.of(_renderRows);
    final nextWrapped = List<bool>.of(_wrappedRows);
    final nextSearchLineIds = List<_TerminalXtermBufferSearchLineId>.of(
      _searchLineIds,
    );
    if (delta.trimStart > 0) {
      nextTexts.removeRange(0, delta.trimStart);
      nextRows.removeRange(0, delta.trimStart);
      nextWrapped.removeRange(0, delta.trimStart);
      nextSearchLineIds.removeRange(0, delta.trimStart);
      _searchLineBase += delta.trimStart;
      _semanticPromptLines = <int>[
        for (final line in _semanticPromptLines)
          if (line >= delta.trimStart) line - delta.trimStart,
      ];
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
        nextWrapped.add(changed.isWrapped);
        nextSearchLineIds.add(
          _TerminalXtermBufferSearchLineId(
            generation: _searchGeneration,
            absoluteIndex: _searchLineBase + changed.row,
          ),
        );
      } else {
        nextTexts[changed.row] = changed.text;
        nextRows[changed.row] = frozenCells;
        nextWrapped[changed.row] = changed.isWrapped;
      }
      _setSemanticPromptLine(changed.row, changed.isSemanticPromptLine);
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
    _wrappedRows = List<bool>.unmodifiable(nextWrapped);
    _searchLineIds = List<_TerminalXtermBufferSearchLineId>.unmodifiable(
      nextSearchLineIds,
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

  bool _lineContinuesFromPrevious(int line) {
    return line > 0 && _wrappedRows[line];
  }

  bool _lineContinuesToNext(int line) {
    final nextLine = line + 1;
    return nextLine < _wrappedRows.length && _wrappedRows[nextLine];
  }

  int _codePoint(int row, int column) {
    if (row < 0 || row >= _renderRows.length || column < 0) return 0;
    final cells = _renderRows[row];
    if (column >= cells.length) return 0;
    return cells[column].content & CellContent.codepointMask;
  }

  int _cellWidth(int row, int column) {
    if (row < 0 || row >= _renderRows.length || column < 0) return 0;
    final cells = _renderRows[row];
    if (column >= cells.length) return 0;
    return cells[column].width;
  }

  bool _lineHasOnlyWhitespace(int line) {
    return _firstNonWhitespaceColumn(line) == _cols;
  }

  int _firstNonWhitespaceColumn(int line) {
    for (var column = 0; column < _cols; column++) {
      if (!_isLineBoundaryWhitespace(_codePoint(line, column))) {
        return column;
      }
    }
    return _cols;
  }

  int _lastNonWhitespaceColumnEnd(int line) {
    for (var column = _cols - 1; column >= 0; column--) {
      if (_isLineBoundaryWhitespace(_codePoint(line, column))) continue;
      return column +
          switch (_cellWidth(line, column)) {
            2 => 2,
            _ => 1,
          };
    }
    return 0;
  }

  bool _isLineBoundaryWhitespace(int codePoint) {
    return switch (codePoint) {
      0 || 0x09 || 0x20 => true,
      _ => false,
    };
  }

  void _setSemanticPromptLine(int line, bool present) {
    final index = _lowerBound(_semanticPromptLines, line);
    final exists =
        index < _semanticPromptLines.length &&
        _semanticPromptLines[index] == line;
    if (present == exists) return;
    if (present) {
      _semanticPromptLines.insert(index, line);
    } else {
      _semanticPromptLines.removeAt(index);
    }
  }

  int _lowerBound(List<int> values, int target) {
    var low = 0;
    var high = values.length;
    while (low < high) {
      final middle = low + ((high - low) >> 1);
      if (values[middle] < target) {
        low = middle + 1;
      } else {
        high = middle;
      }
    }
    return low;
  }
}
