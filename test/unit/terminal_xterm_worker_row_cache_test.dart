import 'package:alera/src/features/workbench/presentation/terminal_xterm_worker_row_cache.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:xterm2/xterm.dart';

void main() {
  test('reuses cache storage for repeated same-width changes', () {
    final line = BufferLine(160);
    final cache = TerminalXtermWorkerRowCache.capture(line);
    expect(cache.changedCellSpan(line), isNull);
    for (var round = 0; round < 512; round++) {
      final column = round % line.length;
      line.setCodePoint(column, 0x61 + round % 26);
      final span = cache.changedCellSpan(line);
      expect(span, (start: column, end: column + 1));
      expect(identical(cache.refresh(line, span), cache), isTrue);
      expect(cache.changedCellSpan(line), isNull);
    }
  });

  test('finds the exact outer boundaries of a wide repaint', () {
    final line = BufferLine(160);
    final cache = TerminalXtermWorkerRowCache.capture(line);
    for (var column = 12; column < 147; column++) {
      line.setCodePoint(column, 0x41);
    }
    final span = cache.changedCellSpan(line);
    expect(span, (start: 12, end: 147));
    expect(cache.refresh(line, span), same(cache));
    expect(cache.changedCellSpan(line), isNull);
  });

  test('style, combining additions and removals, and wrapping stay exact', () {
    final line = BufferLine(20);
    line.setCodePoint(5, 0x65);
    line.setCodePoint(16, 0x65);
    final cache = TerminalXtermWorkerRowCache.capture(line);

    line.addCombiningCharacter(5, 0x0301);
    line.addCombiningCharacter(16, 0x0308);
    var span = cache.changedCellSpan(line);
    expect(span, (start: 5, end: 17));
    expect(cache.refresh(line, span), same(cache));
    expect(cache.changedCellSpan(line), isNull);

    // Writing the same code point removes a combining mark without changing
    // the content word. Another mark outside the changed span must survive.
    line.setCodePoint(5, 0x65);
    span = cache.changedCellSpan(line);
    expect(span, (start: 5, end: 6));
    cache.refresh(line, span);
    expect(cache.changedCellSpan(line), isNull);
    line.setCodePoint(16, 0x65);
    span = cache.changedCellSpan(line);
    expect(span, (start: 16, end: 17));
    cache.refresh(line, span);
    expect(cache.changedCellSpan(line), isNull);

    final cell = CellData.empty();
    line.getCellData(7, cell);
    cell.underlineColor = 12345;
    line.setCellData(7, cell);
    span = cache.changedCellSpan(line);
    expect(span, (start: 7, end: 8));
    cache.refresh(line, span);
    expect(cache.changedCellSpan(line), isNull);
    line.setForeground(1, 123);
    line.setBackground(3, 456);
    line.setAttributes(11, 32);
    span = cache.changedCellSpan(line);
    expect(span, (start: 1, end: 12));
    cache.refresh(line, span);
    expect(cache.changedCellSpan(line), isNull);

    line.isWrapped = true;
    expect(cache.isWrapped, isFalse);
    expect(cache.changedCellSpan(line), isNull);
    expect(cache.refresh(line, null), same(cache));
    expect(cache.isWrapped, isTrue);
  });

  test('resizing or compacting a line requires a fresh cache', () {
    final line = BufferLine(160);
    line.setCodePoint(0, 0x4e2d);
    line.addCombiningCharacter(0, 0x0301);
    final original = TerminalXtermWorkerRowCache.capture(line);
    line.resize(40);
    final span = original.changedCellSpan(line);
    expect(span, (start: 0, end: 40));
    final resized = original.refresh(line, span);
    expect(resized, isNot(same(original)));
    expect(resized.changedCellSpan(line), isNull);
    line.resize(80, clearNewCells: true);
    final expanded = resized.refresh(line, resized.changedCellSpan(line));
    expect(expanded, isNot(same(resized)));
    expect(expanded.changedCellSpan(line), isNull);
  });
}
