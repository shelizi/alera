import 'package:alera/src/features/workbench/presentation/terminal_xterm_worker.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:xterm2/xterm.dart';

void main() {
  test(
    'worker-owned xterm matches direct xterm through writes and resize',
    () async {
      final direct = Terminal(maxLines: 256, reflowWithHiddenCursor: false)
        ..resize(10, 4);
      final worker = await TerminalXtermWorker.start(
        cols: 10,
        rows: 4,
        maxLines: 256,
      );
      addTearDown(worker.close);

      const text = 'abcdefghijABCDEFGHIJ\r\nend';
      direct.write(text);
      var snapshot = await worker.write(text);
      _expectParity(snapshot, direct);

      direct.resize(6, 6);
      snapshot = await worker.resize(cols: 6, rows: 6);
      _expectParity(snapshot, direct);
    },
  );

  test(
    'worker-owned xterm preserves parser effects and interaction modes',
    () async {
      final directEffects = <String>[];
      final direct = Terminal(
        maxLines: 256,
        reflowWithHiddenCursor: false,
        onTitleChange: (title) => directEffects.add('title:$title'),
        onBell: () => directEffects.add('bell'),
        onOutput: (value) => directEffects.add('pty:$value'),
      )..resize(20, 6);
      final worker = await TerminalXtermWorker.start(
        cols: 20,
        rows: 6,
        maxLines: 256,
      );
      addTearDown(worker.close);

      const text =
          '\x1b]2;worker-title\x07\x07\x1b[6n'
          '\x1b[?1h\x1b=\x1b[?25l\x1b[?1000h\x1b[?1004h'
          '\x1b[?1006h\x1b[?1007h\x1b[?2004h';
      direct.write(text);
      final snapshot = await worker.write(text);

      _expectParity(snapshot, direct);
      expect(snapshot.effects, directEffects);
    },
  );

  test('worker delta sends only changed viewport rows', () async {
    final direct = Terminal(maxLines: 256, reflowWithHiddenCursor: false)
      ..resize(12, 4);
    final worker = await TerminalXtermWorker.start(
      cols: 12,
      rows: 4,
      maxLines: 256,
    );
    addTearDown(worker.close);
    final mirror = List<String>.filled(4, '');

    direct.write('first');
    final first = await worker.writeDelta('first');
    expect(first.fullRepaint, isTrue);
    expect(first.rowDeltas, hasLength(4));
    _applyDelta(mirror, first);
    expect(mirror, _viewportRows(direct));

    direct.write('!');
    final second = await worker.writeDelta('!');
    expect(second.fullRepaint, isFalse);
    expect(second.rowDeltas.map((row) => row.row), <int>[0]);
    _applyDelta(mirror, second);
    expect(mirror, _viewportRows(direct));
  });

  test('worker delta detects style-only and combining-mark changes', () async {
    final direct = Terminal(maxLines: 256, reflowWithHiddenCursor: false)
      ..resize(12, 4);
    final worker = await TerminalXtermWorker.start(
      cols: 12,
      rows: 4,
      maxLines: 256,
    );
    addTearDown(worker.close);

    direct.write('\x1b[31mA');
    final first = await worker.writeDelta('\x1b[31mA');
    final originalAttributes = first.rowDeltas.first.cells.first.attributes;

    direct.write('\b\x1b[1mA');
    final styled = await worker.writeDelta('\b\x1b[1mA');
    expect(styled.rowDeltas.map((row) => row.row), <int>[0]);
    expect(styled.rowDeltas.single.text, first.rowDeltas.first.text);
    expect(
      styled.rowDeltas.single.cells.first.attributes,
      isNot(originalAttributes),
    );

    direct.write('e');
    await worker.writeDelta('e');
    direct.write('\u0301');
    final combined = await worker.writeDelta('\u0301');
    expect(combined.rowDeltas.map((row) => row.row), <int>[0]);
    expect(combined.rowDeltas.single.text, _viewportRows(direct).first);
  });

  test('worker resize delta forces a full viewport repaint', () async {
    final direct = Terminal(maxLines: 256, reflowWithHiddenCursor: false)
      ..resize(10, 4)
      ..write('abcdefghijABCDEFGHIJ\r\nend');
    final worker = await TerminalXtermWorker.start(
      cols: 10,
      rows: 4,
      maxLines: 256,
    );
    addTearDown(worker.close);
    await worker.writeDelta('abcdefghijABCDEFGHIJ\r\nend');

    direct.resize(6, 6);
    final delta = await worker.resizeDelta(cols: 6, rows: 6);
    expect(delta.fullRepaint, isTrue);
    expect(delta.rowDeltas, hasLength(6));

    final mirror = List<String>.filled(6, '');
    _applyDelta(mirror, delta);
    expect(mirror, _viewportRows(direct));
    expect(delta.cursorX, direct.buffer.cursorX);
    expect(delta.cursorY, direct.buffer.cursorY);
    expect(delta.scrollBack, direct.buffer.scrollBack);
  });
}

void _expectParity(TerminalXtermWorkerSnapshot snapshot, Terminal direct) {
  expect(snapshot.viewportRows, _viewportRows(direct));
  expect(snapshot.cursorX, direct.buffer.cursorX);
  expect(snapshot.cursorY, direct.buffer.cursorY);
  expect(snapshot.cursorVisible, direct.cursorVisibleMode);
  expect(snapshot.cursorKeys, direct.cursorKeysMode);
  expect(snapshot.keypadKeys, direct.appKeypadMode);
  expect(snapshot.bracketedPaste, direct.bracketedPasteMode);
  expect(snapshot.focusEvents, direct.reportFocusMode);
  expect(snapshot.altScroll, direct.altBufferMouseScrollMode);
  expect(snapshot.mouseMode, direct.mouseMode.index);
  expect(snapshot.mouseReportMode, direct.mouseReportMode.index);
  expect(snapshot.scrollBack, direct.buffer.scrollBack);
}

List<String> _viewportRows(Terminal terminal) {
  final first = terminal.buffer.scrollBack;
  return <String>[
    for (var row = 0; row < terminal.viewHeight; row++)
      terminal.buffer.lines[first + row].toString(),
  ];
}

void _applyDelta(List<String> mirror, TerminalXtermWorkerDelta delta) {
  for (final row in delta.rowDeltas) {
    mirror[row.row] = row.text;
  }
}
