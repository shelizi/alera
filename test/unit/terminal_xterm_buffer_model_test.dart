import 'package:alera/src/features/workbench/presentation/terminal_xterm_buffer_model.dart';
import 'package:alera/src/features/workbench/presentation/terminal_xterm_worker.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'mirrors full scrollback and preserves unchanged row identity',
    () async {
      final worker = await TerminalXtermWorker.start(
        cols: 10,
        rows: 3,
        maxLines: 64,
      );
      addTearDown(worker.close);
      final model = TerminalXtermBufferModel();

      final initial = await worker.writeBufferDelta(
        'one\r\ntwo\r\nthree\r\nfour',
      );
      model.apply(initial);
      expect(model.bufferLength, initial.bufferLength);
      expect(model.scrollBack, initial.scrollBack);
      expect(model.rowTexts, <String>['one', 'two', 'three', 'four']);
      final firstRow = model.renderRows[0];
      final secondRow = model.renderRows[1];

      final partial = await worker.writeBufferDelta('!');
      model.apply(partial);
      expect(model.rowTexts, <String>['one', 'two', 'three', 'four!']);
      expect(identical(model.renderRows[0], firstRow), isTrue);
      expect(identical(model.renderRows[1], secondRow), isTrue);
      expect(model.revision, partial.revision);
      expect(() => model.apply(partial), throwsStateError);
    },
  );

  test('applies circular head trim without rebuilding retained rows', () async {
    final worker = await TerminalXtermWorker.start(
      cols: 8,
      rows: 3,
      maxLines: 25,
    );
    addTearDown(worker.close);
    final model = TerminalXtermBufferModel();

    final initialText = List<String>.generate(
      25,
      (index) => '${index + 1}',
    ).join('\r\n');
    model.apply(await worker.writeBufferDelta(initialText));
    final oldSecondRow = model.renderRows[1];

    final trimmed = await worker.writeBufferDelta('\r\n26');
    expect(trimmed.fullRepaint, isFalse);
    expect(trimmed.trimStart, 1);
    model.apply(trimmed);

    expect(model.bufferLength, 25);
    expect(model.rowTexts.first, '2');
    expect(model.rowTexts.last, '26');
    expect(identical(model.renderRows.first, oldSecondRow), isTrue);
  });

  test(
    'resize buffer delta rebuilds reflowed scrollback immediately',
    () async {
      final worker = await TerminalXtermWorker.start(
        cols: 10,
        rows: 4,
        maxLines: 64,
      );
      addTearDown(worker.close);
      final model = TerminalXtermBufferModel();

      model.apply(await worker.writeBufferDelta('abcdefghijABCDEFGHIJ\r\nend'));
      final resized = await worker.resizeBufferDelta(cols: 6, rows: 6);
      expect(resized.fullRepaint, isTrue);
      model.apply(resized);

      expect(model.cols, 6);
      expect(model.rows, 6);
      expect(model.bufferLength, resized.bufferLength);
      expect(model.rowTexts, hasLength(resized.bufferLength));
      expect(model.scrollBack, resized.scrollBack);
      expect(model.cursorX, resized.cursorX);
      expect(model.cursorY, resized.cursorY);
    },
  );

  test('mode-only buffer delta updates state without replacing rows', () async {
    final worker = await TerminalXtermWorker.start(
      cols: 10,
      rows: 3,
      maxLines: 64,
    );
    addTearDown(worker.close);
    final model = TerminalXtermBufferModel();

    model.apply(await worker.writeBufferDelta('ready'));
    final identities = List<Object>.from(model.renderRows);
    final escape = String.fromCharCode(27);
    final modes = await worker.writeBufferDelta('$escape[?2004h$escape[?1004h');
    expect(modes.rowDeltas, isEmpty);
    model.apply(modes);

    expect(model.bracketedPaste, isTrue);
    expect(model.focusEvents, isTrue);
    for (var row = 0; row < model.bufferLength; row++) {
      expect(identical(model.renderRows[row], identities[row]), isTrue);
    }
  });

  test('mirrors global render state and resolves OSC8 hyperlinks', () async {
    final worker = await TerminalXtermWorker.start(
      cols: 16,
      rows: 4,
      maxLines: 64,
    );
    addTearDown(worker.close);
    final model = TerminalXtermBufferModel();

    final escape = String.fromCharCode(27);
    final bell = String.fromCharCode(7);
    final stringTerminator = '$escape\\';
    final delta = await worker.writeBufferDelta(
      '$escape[?1049h'
      '$escape[?5h'
      '$escape[?1036l$escape[?1039h'
      '$escape[5 q'
      '$escape]4;1;#112233$bell'
      '$escape]8;;https://example.com$stringTerminator'
      'link'
      '$escape]8;;$stringTerminator',
    );
    model.apply(delta);

    expect(model.isUsingAltBuffer, isTrue);
    expect(model.reverseDisplay, isTrue);
    expect(model.altEscPrefix, isFalse);
    expect(model.altSendsEscape, isTrue);
    expect(model.cursorType, isNotNull);
    expect(model.cursorBlink, isTrue);
    expect(model.indexedColorOverrides[1], isNotNull);

    String? hyperlink;
    for (var row = 0; row < model.bufferLength && hyperlink == null; row++) {
      for (var column = 0; column < model.renderRows[row].length; column++) {
        if (model.renderRows[row][column].hyperlinkId == 0) continue;
        hyperlink = model.hyperlinkAt(row, column);
        break;
      }
    }
    expect(hyperlink, 'https://example.com');
  });
}
