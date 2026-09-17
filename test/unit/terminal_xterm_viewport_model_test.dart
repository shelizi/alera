import 'package:alera/src/features/workbench/presentation/terminal_xterm_viewport_model.dart';
import 'package:alera/src/features/workbench/presentation/terminal_xterm_worker.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'rebuilds xterm viewport while preserving untouched row identity',
    () async {
      final worker = await TerminalXtermWorker.start(
        cols: 12,
        rows: 4,
        maxLines: 64,
      );
      addTearDown(worker.close);
      final viewport = TerminalXtermViewportModel();

      final initial = await worker.writeDelta('one\r\ntwo');
      viewport.apply(initial);

      expect(viewport.cols, 12);
      expect(viewport.rows, 4);
      expect(viewport.rowText(0), 'one');
      expect(viewport.rowText(1), 'two');
      final untouchedFirstRow = viewport.renderRows[0];
      final changedSecondRow = viewport.renderRows[1];

      final partial = await worker.writeDelta('X');
      expect(partial.fullRepaint, isFalse);
      expect(partial.rowDeltas.map((row) => row.row), <int>[1]);
      viewport.apply(partial);

      expect(viewport.rowText(0), 'one');
      expect(viewport.rowText(1), 'twoX');
      expect(identical(viewport.renderRows[0], untouchedFirstRow), isTrue);
      expect(identical(viewport.renderRows[1], changedSecondRow), isFalse);
      expect(viewport.cursorX, partial.cursorX);
      expect(viewport.cursorY, partial.cursorY);
    },
  );

  test('applies mode-only deltas without replacing viewport rows', () async {
    final worker = await TerminalXtermWorker.start(
      cols: 12,
      rows: 4,
      maxLines: 64,
    );
    addTearDown(worker.close);
    final viewport = TerminalXtermViewportModel();

    viewport.apply(await worker.writeDelta('ready'));
    final rowIdentities = List<Object>.from(viewport.renderRows);

    final modes = await worker.writeDelta('\x1b[?2004h\x1b[?1004h');
    expect(modes.rowDeltas, isEmpty);
    viewport.apply(modes);

    expect(viewport.bracketedPaste, isTrue);
    expect(viewport.focusEvents, isTrue);
    for (var row = 0; row < viewport.rows; row++) {
      expect(identical(viewport.renderRows[row], rowIdentities[row]), isTrue);
    }
  });

  test('accepts resize only as a complete full repaint', () async {
    final worker = await TerminalXtermWorker.start(
      cols: 10,
      rows: 4,
      maxLines: 64,
    );
    addTearDown(worker.close);
    final viewport = TerminalXtermViewportModel();

    viewport.apply(await worker.writeDelta('abcdefghijABCDEFGHIJ\r\nend'));
    final resized = await worker.resizeDelta(cols: 6, rows: 6);
    expect(resized.fullRepaint, isTrue);
    viewport.apply(resized);

    expect(viewport.cols, 6);
    expect(viewport.rows, 6);
    expect(viewport.renderRows, hasLength(6));
    expect(viewport.rowTexts, hasLength(6));
    expect(viewport.cursorX, resized.cursorX);
    expect(viewport.cursorY, resized.cursorY);
    expect(viewport.scrollBack, resized.scrollBack);
  });
}
