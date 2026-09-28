import 'dart:collection';
import 'dart:typed_data';

import 'package:xterm2/core.dart';

import 'terminal_search_source.dart';
import 'terminal_xterm_worker.dart';

final class _TerminalXtermHeadList<T extends Object> extends ListBase<T> {
  static const int _minCompactionHead = 1024;

  final List<T?> _storage = <T?>[];
  int _head = 0;

  int get storageHead => _head;

  @override
  int get length => _storage.length - _head;

  @override
  set length(int value) {
    if (value < 0) {
      throw RangeError.range(value, 0, null, 'value');
    }
    final current = length;
    if (value > current) {
      throw UnsupportedError('Grow the terminal head list with add/addAll.');
    }
    if (value == 0) {
      clear();
      return;
    }
    if (value == current) return;
    _storage.length = _head + value;
  }

  @override
  T operator [](int index) {
    RangeError.checkValidIndex(index, this);
    return _storage[_head + index] as T;
  }

  @override
  void operator []=(int index, T value) {
    RangeError.checkValidIndex(index, this);
    _storage[_head + index] = value;
  }

  @override
  void add(T value) => _storage.add(value);

  @override
  void addAll(Iterable<T> iterable) => _storage.addAll(iterable);

  @override
  void clear() {
    _storage.clear();
    _head = 0;
  }

  void trimStart(int count) {
    RangeError.checkValueInInterval(count, 0, length, 'count');
    if (count == 0) return;
    final end = _head + count;
    for (var index = _head; index < end; index++) {
      _storage[index] = null;
    }
    _head = end;
    _compactIfNeeded();
  }

  void _compactIfNeeded() {
    if (_head < _minCompactionHead || _head * 2 < _storage.length) return;
    final retained = _storage.sublist(_head);
    _storage
      ..clear()
      ..addAll(retained);
    _head = 0;
  }
}

const int _packedCellWords = 3;
const int _retainedRowOverheadBytes = 96;
const int _packedWidthWord = 0;
const int _packedContentWord = 1;
const int _packedHyperlinkWord = 2;

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
  final _TerminalXtermHeadList<_TerminalXtermBufferSearchLineId>
  _searchLineIds = _TerminalXtermHeadList<_TerminalXtermBufferSearchLineId>();
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
  final _TerminalXtermHeadList<String> _rowTexts =
      _TerminalXtermHeadList<String>();
  // One packed row per buffer line rather than one object per cell: a full
  // scrollback held ~10k x cols cell objects on the UI isolate, and that heap
  // is what every collection in the isolate group had to walk.
  final _TerminalXtermHeadList<Uint32List> _renderRows =
      _TerminalXtermHeadList<Uint32List>();
  final _TerminalXtermHeadList<bool> _wrappedRows =
      _TerminalXtermHeadList<bool>();
  List<int> _semanticPromptLines = <int>[];
  late final UnmodifiableListView<String> _rowTextsView = UnmodifiableListView(
    _rowTexts,
  );
  late final UnmodifiableListView<Uint32List> _renderRowsView =
      UnmodifiableListView(_renderRows);

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
  List<String> get rowTexts => _rowTextsView;

  /// Packed rows, [_packedCellWords] words per cell (width, content,
  /// hyperlink id). Read-only; exposed for tests.
  List<Uint32List> get packedRows => _renderRowsView;

  int rowLength(int row) => _renderRows[row].length ~/ _packedCellWords;

  /// Estimated bytes this mirror holds on top of the replica's own cells: the
  /// packed rows, the row text search reads and a fixed per-row overhead for
  /// headers and list slots. The eviction budget used to see only the
  /// replica's cells, which is less than half of what a mirrored tab retains.
  int get retainedBytes {
    var bytes = 0;
    for (var row = 0; row < _renderRows.length; row++) {
      bytes +=
          _renderRows[row].lengthInBytes +
          _rowTexts[row].length * 2 +
          _retainedRowOverheadBytes;
    }
    return bytes;
  }

  /// Exposes the logical backing head only to make amortized trim behavior
  /// deterministic in unit tests; UI/runtime code must not depend on it.
  int get storageHeadForTesting => _rowTexts.storageHead;

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
    if (row < 0 || row >= _renderRows.length || column < 0) return 0;
    final cells = _renderRows[row];
    final offset = column * _packedCellWords;
    if (offset >= cells.length) return 0;
    return cells[offset + _packedHyperlinkWord];
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
    final nextRows = List<Uint32List?>.filled(
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
      _validateFullRowCells(changed);
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
    _rowTexts
      ..clear()
      ..addAll(nextTexts.cast<String>());
    _renderRows
      ..clear()
      ..addAll(nextRows.cast<Uint32List>());
    _wrappedRows
      ..clear()
      ..addAll(nextWrapped.cast<bool>());
    _semanticPromptLines = <int>[
      for (final changed in delta.rowDeltas)
        if (changed.isSemanticPromptLine) changed.row,
    ];
    _searchGeneration = Object();
    _searchLineBase = 0;
    _searchLineIds
      ..clear()
      ..addAll(<_TerminalXtermBufferSearchLineId>[
        for (var row = 0; row < delta.bufferLength; row++)
          _TerminalXtermBufferSearchLineId(
            generation: _searchGeneration,
            absoluteIndex: row,
          ),
      ]);
  }

  void _applyPartialBuffer(TerminalXtermWorkerBufferDelta delta) {
    if (delta.trimStart < 0 || delta.trimStart > _renderRows.length) {
      throw StateError(
        'Terminal xterm buffer returned invalid head trim ${delta.trimStart}.',
      );
    }

    _validatePartialBuffer(delta);
    if (delta.trimStart > 0) {
      _rowTexts.trimStart(delta.trimStart);
      _renderRows.trimStart(delta.trimStart);
      _wrappedRows.trimStart(delta.trimStart);
      _searchLineIds.trimStart(delta.trimStart);
      _searchLineBase += delta.trimStart;
      _semanticPromptLines = <int>[
        for (final line in _semanticPromptLines)
          if (line >= delta.trimStart) line - delta.trimStart,
      ];
    }

    for (final changed in delta.rowDeltas) {
      _validateRow(changed.row, delta.bufferLength);
      if (changed.row == _renderRows.length) {
        _validateFullRowCells(changed);
        _rowTexts.add(changed.text);
        _renderRows.add(_freezeCells(changed.cells));
        _wrappedRows.add(changed.isWrapped);
        _searchLineIds.add(
          _TerminalXtermBufferSearchLineId(
            generation: _searchGeneration,
            absoluteIndex: _searchLineBase + changed.row,
          ),
        );
      } else {
        _rowTexts[changed.row] = changed.text;
        _renderRows[changed.row] = _mergeCellSpan(
          _renderRows[changed.row],
          changed,
        );
        _wrappedRows[changed.row] = changed.isWrapped;
      }
      _setSemanticPromptLine(changed.row, changed.isSemanticPromptLine);
    }

    if (_renderRows.length != delta.bufferLength) {
      throw StateError(
        'Terminal xterm partial delta produced ${_renderRows.length} rows; '
        'expected ${delta.bufferLength}.',
      );
    }
  }

  void _validatePartialBuffer(TerminalXtermWorkerBufferDelta delta) {
    final retainedLength = _renderRows.length - delta.trimStart;
    var predictedLength = retainedLength;
    for (final changed in delta.rowDeltas) {
      _validateRow(changed.row, delta.bufferLength);
      if (changed.row > predictedLength) {
        throw StateError(
          'Terminal xterm buffer delta skipped appended row $predictedLength.',
        );
      }
      if (changed.row == predictedLength) {
        _validateFullRowCells(changed);
        predictedLength += 1;
        continue;
      }

      if (changed.row >= retainedLength) {
        throw StateError(
          'Terminal xterm buffer delta rewrote appended row ${changed.row}.',
        );
      }
      final existing = _renderRows[delta.trimStart + changed.row];
      final existingLength = existing.length ~/ _packedCellWords;
      if (existingLength != changed.rowLength) {
        _validateFullRowCells(changed);
        continue;
      }
      final start = changed.cellStart;
      final end = start + changed.cells.length;
      if (start < 0 || end > existingLength) {
        throw StateError(
          'Terminal xterm cell span [$start, $end) exceeds row width '
          '$existingLength.',
        );
      }
    }
    if (predictedLength != delta.bufferLength) {
      throw StateError(
        'Terminal xterm partial delta would produce $predictedLength rows; '
        'expected ${delta.bufferLength}.',
      );
    }
  }

  Uint32List _freezeCells(List<TerminalXtermWorkerRenderCell> cells) {
    final packed = Uint32List(cells.length * _packedCellWords);
    for (var column = 0; column < cells.length; column++) {
      _packCell(packed, column, cells[column]);
    }
    return packed;
  }

  void _packCell(
    Uint32List packed,
    int column,
    TerminalXtermWorkerRenderCell cell,
  ) {
    final offset = column * _packedCellWords;
    packed[offset + _packedWidthWord] = cell.width;
    packed[offset + _packedContentWord] = cell.content;
    packed[offset + _packedHyperlinkWord] = cell.hyperlinkId;
  }

  Uint32List _mergeCellSpan(
    Uint32List existing,
    TerminalXtermWorkerRowDelta changed,
  ) {
    final existingLength = existing.length ~/ _packedCellWords;
    if (existingLength != changed.rowLength) {
      _validateFullRowCells(changed);
      return _freezeCells(changed.cells);
    }
    final start = changed.cellStart;
    final end = start + changed.cells.length;
    if (start < 0 || end > existingLength) {
      throw StateError(
        'Terminal xterm cell span [$start, $end) exceeds row width '
        '$existingLength.',
      );
    }
    if (changed.cells.isEmpty) return existing;
    if (start == 0 && end == existingLength) {
      return _freezeCells(changed.cells);
    }
    // A new row rather than an in-place write keeps the identity contract:
    // an untouched row is the same object, a changed one is not.
    final next = Uint32List.fromList(existing);
    for (var offset = 0; offset < changed.cells.length; offset++) {
      _packCell(next, start + offset, changed.cells[offset]);
    }
    return next;
  }

  void _validateFullRowCells(TerminalXtermWorkerRowDelta row) {
    if (row.cellStart != 0 || row.cells.length != row.rowLength) {
      throw StateError(
        'Terminal xterm full row ${row.row} returned cell span '
        '${row.cellStart}+${row.cells.length}; expected 0+${row.rowLength}.',
      );
    }
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
    final offset = column * _packedCellWords;
    if (offset >= cells.length) return 0;
    return cells[offset + _packedContentWord] & CellContent.codepointMask;
  }

  int _cellWidth(int row, int column) {
    if (row < 0 || row >= _renderRows.length || column < 0) return 0;
    final cells = _renderRows[row];
    final offset = column * _packedCellWords;
    if (offset >= cells.length) return 0;
    return cells[offset + _packedWidthWord];
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
