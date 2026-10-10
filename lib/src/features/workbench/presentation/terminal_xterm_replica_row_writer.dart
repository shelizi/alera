import 'package:xterm2/xterm.dart';

import 'terminal_xterm_worker.dart';
import 'terminal_xterm_worker_packed_cells.dart';

/// Writes worker row deltas into the replica's xterm buffer lines.
final class TerminalXtermReplicaRowWriter {
  // setCellData copies the fields, so one scratch cell serves every row.
  final CellData _scratch = CellData.empty();

  BufferLine buildLine(TerminalXtermWorkerRowDelta row) {
    if (row.cellStart != 0 || row.cells.length != row.rowLength) {
      throw StateError(
        'Terminal xterm replica full row ${row.row} returned cell span '
        '${row.cellStart}+${row.cells.length}; expected 0+${row.rowLength}.',
      );
    }
    final line = BufferLine(row.rowLength, isWrapped: row.isWrapped);
    applyRowDelta(line, row);
    return line;
  }

  void applyRowDelta(BufferLine line, TerminalXtermWorkerRowDelta row) {
    if (line.length != row.rowLength) {
      if (row.cellStart != 0 || row.cells.length != row.rowLength) {
        throw StateError(
          'Terminal xterm replica cannot resize row ${row.row} from '
          '${line.length} to ${row.rowLength} with partial span '
          '${row.cellStart}+${row.cells.length}.',
        );
      }
      line.resize(row.rowLength, clearNewCells: true);
    }
    final start = row.cellStart;
    final end = start + row.cells.length;
    if (start < 0 || end > line.length) {
      throw StateError(
        'Terminal xterm replica cell span [$start, $end) exceeds row width '
        '${line.length}.',
      );
    }
    line.isWrapped = row.isWrapped;
    final cells = row.cells;
    final scratch = _scratch;
    if (cells is TerminalXtermWorkerPackedCells) {
      for (var offset = 0; offset < cells.length; offset++) {
        final column = start + offset;
        scratch
          ..foreground = cells.foregroundAt(offset)
          ..background = cells.backgroundAt(offset)
          ..underlineColor = cells.underlineColorAt(offset)
          ..flags = cells.attributesAt(offset)
          ..content = cells.contentAt(offset);
        line.setCellData(column, scratch);
        final combining = cells.combiningCharactersAt(offset);
        if (combining != null) {
          for (final codePoint in combining.runes) {
            line.addCombiningCharacter(column, codePoint);
          }
        }
      }
      return;
    }
    for (var offset = 0; offset < cells.length; offset++) {
      final column = start + offset;
      final cell = cells[offset];
      scratch
        ..foreground = cell.foreground
        ..background = cell.background
        ..underlineColor = cell.underlineColor
        ..flags = cell.attributes
        ..content = cell.content;
      line.setCellData(column, scratch);
      final combining = cell.combiningCharacters;
      if (combining != null) {
        for (final codePoint in combining.runes) {
          line.addCombiningCharacter(column, codePoint);
        }
      }
    }
  }
}
