import 'dart:typed_data';

import 'package:xterm2/xterm.dart';

typedef TerminalXtermWorkerCellSpan = ({int start, int end});

/// Worker-owned snapshot of an active row, independent of the mutable xterm
/// line. Unlike UI mirror rows, this cache never escapes its worker, so a
/// same-width update can reuse the storage and refresh only the changed span.
final class TerminalXtermWorkerRowCache {
  TerminalXtermWorkerRowCache._(this._cells, this.isWrapped);

  factory TerminalXtermWorkerRowCache.capture(BufferLine line) {
    return TerminalXtermWorkerRowCache._(
      Uint32List(line.length * _wordsPerCell),
      line.isWrapped,
    ).._refreshCells(line, 0, line.length);
  }

  static const int _wordsPerCell = 5;

  final Uint32List _cells;
  List<String?>? _combiningCharacters;
  bool isWrapped;

  TerminalXtermWorkerCellSpan? changedCellSpan(BufferLine line) {
    if (_cells.length != line.length * _wordsPerCell) {
      return (start: 0, end: line.length);
    }

    final hasCombining = line.hasCombiningCharacters;
    var start = 0;
    while (start < line.length && _cellMatches(line, start, hasCombining)) {
      start++;
    }
    if (start == line.length) return null;

    // Only the boundaries of the changed span are needed. In a large TUI
    // repaint the interior cells will be serialized/refreshed anyway.
    var end = line.length;
    while (end > start + 1 && _cellMatches(line, end - 1, hasCombining)) {
      end--;
    }
    return (start: start, end: end);
  }

  bool _cellMatches(BufferLine line, int column, bool hasCombining) {
    final offset = column * _wordsPerCell;
    if (_cells[offset] != line.getForeground(column) ||
        _cells[offset + 1] != line.getBackground(column) ||
        _cells[offset + 2] != line.getAttributes(column) ||
        _cells[offset + 3] != line.getContent(column) ||
        _cells[offset + 4] != line.getUnderlineColor(column)) {
      return false;
    }
    return _combiningCharacters?[column] ==
        (hasCombining ? line.getCombiningCharacters(column) : null);
  }

  /// Refresh after [changedCellSpan] against the same line state. A resize
  /// requires new storage; ordinary text, style and wrap updates do not.
  TerminalXtermWorkerRowCache refresh(
    BufferLine line,
    TerminalXtermWorkerCellSpan? span,
  ) {
    if (_cells.length != line.length * _wordsPerCell) {
      return TerminalXtermWorkerRowCache.capture(line);
    }
    isWrapped = line.isWrapped;
    if (span != null) _refreshCells(line, span.start, span.end);
    return this;
  }

  void _refreshCells(BufferLine line, int start, int end) {
    final combining = line.hasCombiningCharacters
        ? (_combiningCharacters ??= List<String?>.filled(line.length, null))
        : (_combiningCharacters = null);
    for (var column = start; column < end; column++) {
      final offset = column * _wordsPerCell;
      _cells[offset] = line.getForeground(column);
      _cells[offset + 1] = line.getBackground(column);
      // The packed attributes also include hyperlink and semantic bits.
      _cells[offset + 2] = line.getAttributes(column);
      _cells[offset + 3] = line.getContent(column);
      _cells[offset + 4] = line.getUnderlineColor(column);
      if (combining != null) {
        combining[column] = line.getCombiningCharacters(column);
      }
    }
  }
}
