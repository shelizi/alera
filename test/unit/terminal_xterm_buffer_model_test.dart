import 'package:alera/src/features/workbench/presentation/terminal_xterm_buffer_model.dart';
import 'package:alera/src/features/workbench/presentation/terminal_xterm_worker.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:xterm2/core.dart';
import 'package:xterm2/xterm.dart';

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

  test('partial updates keep top-level mirror storage in place', () async {
    final worker = await TerminalXtermWorker.start(
      cols: 10,
      rows: 3,
      maxLines: 64,
    );
    addTearDown(worker.close);
    final model = TerminalXtermBufferModel();

    model.apply(await worker.writeBufferDelta('one\r\ntwo\r\nthree'));
    final rowTexts = model.rowTexts;
    final renderRows = model.renderRows;

    model.apply(await worker.writeBufferDelta('!'));

    expect(identical(model.rowTexts, rowTexts), isTrue);
    expect(identical(model.renderRows, renderRows), isTrue);
    expect(model.rowTexts.last, 'three!');
  });

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

  test('word and logical-line boundaries match direct xterm', () async {
    final separators = <int>{'/'.codeUnitAt(0)};
    final direct = Terminal(
      maxLines: 64,
      reflowWithHiddenCursor: false,
      wordSeparators: separators,
    )..resize(6, 4);
    final worker = await TerminalXtermWorker.start(
      cols: 6,
      rows: 4,
      maxLines: 64,
      wordSeparators: separators,
    );
    addTearDown(worker.close);
    final model = TerminalXtermBufferModel(wordSeparators: separators);

    const wrapped = 'foo/barbaz';
    direct.write(wrapped);
    model.apply(await worker.writeBufferDelta(wrapped));

    expect(model.isWrapped(1), direct.buffer.lines[1].isWrapped);
    _expectRangeParity(
      model.getWordBoundary(const CellOffset(1, 1)),
      direct.buffer.getWordBoundary(const CellOffset(1, 1)),
    );
    _expectRangeParity(
      model.getLineBoundary(const CellOffset(1, 1)),
      direct.buffer.getLineBoundary(const CellOffset(1, 1)),
    );

    const wide = '\r\nab界cd';
    direct.write(wide);
    model.apply(await worker.writeBufferDelta(wide));
    _expectRangeParity(
      model.getWordBoundary(const CellOffset(3, 2)),
      direct.buffer.getWordBoundary(const CellOffset(3, 2)),
    );
  });

  test('semantic prompt navigation matches direct xterm', () async {
    final direct = Terminal(maxLines: 20, reflowWithHiddenCursor: false)
      ..resize(12, 4);
    final worker = await TerminalXtermWorker.start(
      cols: 12,
      rows: 4,
      maxLines: 20,
    );
    addTearDown(worker.close);
    final model = TerminalXtermBufferModel();
    final escape = String.fromCharCode(27);
    final stringTerminator = '$escape\\';
    final sequence =
        '$escape]133;A$stringTerminator'
        'first\r\n'
        '$escape]133;A;k=c$stringTerminator'
        'continuation\r\n'
        '$escape]133;A$stringTerminator'
        'second';

    direct.write(sequence);
    model.apply(await worker.writeBufferDelta(sequence));

    for (var line = 0; line < direct.buffer.lines.length; line++) {
      expect(
        model.isSemanticPromptLine(line),
        direct.isSemanticPromptLine(line),
      );
    }
    for (final line in <int>[-1, 0, 1, 2, 99]) {
      expect(
        model.semanticPromptLineBefore(line),
        direct.semanticPromptLineBefore(line),
      );
      expect(
        model.semanticPromptLineAfter(line),
        direct.semanticPromptLineAfter(line),
      );
    }
  });

  test(
    'semantic prompt indexes follow scrollback trims incrementally',
    () async {
      final direct = Terminal(maxLines: 25, reflowWithHiddenCursor: false)
        ..resize(12, 3);
      final worker = await TerminalXtermWorker.start(
        cols: 12,
        rows: 3,
        maxLines: 25,
      );
      addTearDown(worker.close);
      final model = TerminalXtermBufferModel();
      final escape = String.fromCharCode(27);
      final stringTerminator = '$escape\\';
      final prompt = '$escape]133;A$stringTerminator';

      final initial = <String>[
        '${prompt}first',
        ...List<String>.generate(23, (index) => 'seed-$index'),
      ].join('\r\n');
      direct.write(initial);
      model.apply(await worker.writeBufferDelta(initial));
      _expectSemanticPromptParity(model, direct);

      const overflow = '\r\nline-24\r\nline-25';
      direct.write(overflow);
      final trimmed = await worker.writeBufferDelta(overflow);
      expect(trimmed.trimStart, greaterThan(0));
      model.apply(trimmed);
      _expectSemanticPromptParity(model, direct);

      final nextPrompt = '\r\n${prompt}latest';
      direct.write(nextPrompt);
      model.apply(await worker.writeBufferDelta(nextPrompt));
      _expectSemanticPromptParity(model, direct);
    },
  );

  test('hyperlink ID lookup stays synchronous in the mirror', () async {
    final worker = await TerminalXtermWorker.start(
      cols: 16,
      rows: 4,
      maxLines: 64,
    );
    addTearDown(worker.close);
    final model = TerminalXtermBufferModel();
    final escape = String.fromCharCode(27);
    final stringTerminator = '$escape\\';

    model.apply(
      await worker.writeBufferDelta(
        '$escape]8;;https://example.com$stringTerminator'
        'link'
        '$escape]8;;$stringTerminator',
      ),
    );

    final hyperlinkId = model.hyperlinkIdAt(0, 0);
    expect(hyperlinkId, greaterThan(0));
    expect(model.hyperlinkAt(0, 0), 'https://example.com');
  });
}

void _expectRangeParity(BufferRangeLine? actual, BufferRangeLine? expected) {
  expect(actual == null, expected == null);
  if (actual == null || expected == null) return;
  expect(actual.begin.x, expected.begin.x);
  expect(actual.begin.y, expected.begin.y);
  expect(actual.end.x, expected.end.x);
  expect(actual.end.y, expected.end.y);
}

void _expectSemanticPromptParity(
  TerminalXtermBufferModel model,
  Terminal terminal,
) {
  for (var line = 0; line < terminal.buffer.lines.length; line++) {
    expect(
      model.isSemanticPromptLine(line),
      terminal.isSemanticPromptLine(line),
    );
  }
  for (final line in <int>[-1, 0, 1, 2, terminal.buffer.lines.length, 999]) {
    expect(
      model.semanticPromptLineBefore(line),
      terminal.semanticPromptLineBefore(line),
    );
    expect(
      model.semanticPromptLineAfter(line),
      terminal.semanticPromptLineAfter(line),
    );
  }
}
