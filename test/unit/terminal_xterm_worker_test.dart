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
}

void _expectParity(TerminalXtermWorkerSnapshot snapshot, Terminal direct) {
  final first = direct.buffer.scrollBack;
  final expectedRows = <String>[
    for (var row = 0; row < direct.viewHeight; row++)
      direct.buffer.lines[first + row].toString(),
  ];
  expect(snapshot.viewportRows, expectedRows);
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
