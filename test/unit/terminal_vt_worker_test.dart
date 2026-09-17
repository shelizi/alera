import 'dart:convert';
import 'dart:typed_data';

import 'package:alera/src/features/workbench/presentation/terminal_vt_worker.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'parses VT bytes off-isolate and returns a primitive snapshot',
    () async {
      final worker = await TerminalVtWorker.start(
        cols: 16,
        rows: 4,
        maxScrollback: 32,
      );
      addTearDown(worker.close);

      final snapshot = await worker.writeBytes(
        Uint8List.fromList(utf8.encode('hello\r\n\x1b[31mred\x1b[0m')),
      );

      expect(snapshot.cols, 16);
      expect(snapshot.rows, 4);
      expect(snapshot.text, contains('hello'));
      expect(snapshot.text, contains('red'));
      expect(snapshot.viewportRows.any((row) => row.contains('red')), isTrue);
      expect(snapshot.revision, 1);
    },
  );

  test(
    'resizes the worker-owned terminal without losing parsed state',
    () async {
      final worker = await TerminalVtWorker.start(
        cols: 12,
        rows: 3,
        maxScrollback: 32,
      );
      addTearDown(worker.close);

      await worker.writeBytes(Uint8List.fromList(utf8.encode('persist')));
      final snapshot = await worker.resize(cols: 24, rows: 6);

      expect(snapshot.cols, 24);
      expect(snapshot.rows, 6);
      expect(snapshot.text, contains('persist'));
      expect(snapshot.revision, 2);
    },
  );

  test('returns only dirty viewport rows after the initial repaint', () async {
    final worker = await TerminalVtWorker.start(
      cols: 16,
      rows: 6,
      maxScrollback: 32,
    );
    addTearDown(worker.close);

    final first = await worker.writeDelta(
      Uint8List.fromList(utf8.encode('first')),
    );
    expect(first.fullRepaint, isTrue);
    expect(first.changedRows, isNotEmpty);

    final second = await worker.writeDelta(
      Uint8List.fromList(utf8.encode('B')),
    );

    expect(second.fullRepaint, isFalse);
    expect(second.changedRows, isNotEmpty);
    expect(second.changedRows.length, lessThan(second.rows));
    expect(
      second.changedRows.expand((row) => row.cells).join(),
      contains('firstB'),
    );
    expect(second.revision, 2);
  });

  test('preserves VT writeback, title, and bell effects in order', () async {
    final worker = await TerminalVtWorker.start(
      cols: 16,
      rows: 6,
      maxScrollback: 32,
    );
    addTearDown(worker.close);

    final delta = await worker.writeDelta(
      Uint8List.fromList(utf8.encode('\x1b]2;worker-title\x07\x07\x1b[6n')),
    );

    expect(delta.effects, hasLength(3));
    expect(delta.effects[0], isA<TerminalVtWorkerTitleChanged>());
    expect(
      (delta.effects[0] as TerminalVtWorkerTitleChanged).title,
      'worker-title',
    );
    expect(delta.effects[1], isA<TerminalVtWorkerBell>());
    expect(delta.effects[2], isA<TerminalVtWorkerPtyWrite>());
    expect(
      utf8.decode((delta.effects[2] as TerminalVtWorkerPtyWrite).bytes),
      '\x1b[1;1R',
    );
  });

  test(
    'reports interaction modes needed by terminal input and pointer routing',
    () async {
      final worker = await TerminalVtWorker.start(
        cols: 16,
        rows: 6,
        maxScrollback: 32,
      );
      addTearDown(worker.close);

      final enabled = await worker.writeDelta(
        Uint8List.fromList(
          utf8.encode(
            '\x1b[?1h'
            '\x1b[?25l'
            '\x1b[?1000h'
            '\x1b[?1004h'
            '\x1b[?1006h'
            '\x1b[?1007h'
            '\x1b[?2004h',
          ),
        ),
      );

      expect(enabled.modes.bracketedPaste, isTrue);
      expect(enabled.modes.cursorKeys, isTrue);
      expect(enabled.modes.cursorVisible, isFalse);
      expect(enabled.modes.mouseEnabled, isTrue);
      expect(enabled.modes.mouseTrackingMode, 2);
      expect(enabled.modes.mouseFormat, 2);
      expect(enabled.modes.focusEvents, isTrue);
      expect(enabled.modes.altScroll, isTrue);

      final disabled = await worker.writeDelta(
        Uint8List.fromList(
          utf8.encode(
            '\x1b[?1l'
            '\x1b[?25h'
            '\x1b[?1000l'
            '\x1b[?1004l'
            '\x1b[?1006l'
            '\x1b[?1007l'
            '\x1b[?2004l',
          ),
        ),
      );

      expect(disabled.modes.bracketedPaste, isFalse);
      expect(disabled.modes.cursorKeys, isFalse);
      expect(disabled.modes.cursorVisible, isTrue);
      expect(disabled.modes.mouseEnabled, isFalse);
      expect(disabled.modes.mouseTrackingMode, isNull);
      expect(disabled.modes.mouseFormat, isNull);
      expect(disabled.modes.focusEvents, isFalse);
      expect(disabled.modes.altScroll, isFalse);
    },
  );

  test(
    'serializes styled and wide dirty cells through a style table',
    () async {
      final worker = await TerminalVtWorker.start(
        cols: 16,
        rows: 4,
        maxScrollback: 32,
      );
      addTearDown(worker.close);

      final delta = await worker.writeDelta(
        Uint8List.fromList(utf8.encode('A\x1b[1;3;31mB\x1b[0m界')),
      );

      final row = delta.changedRows.firstWhere(
        (candidate) => candidate.cells.join().contains('AB界'),
      );
      final plain = row.renderCells.firstWhere((cell) => cell.text == 'A');
      final styled = row.renderCells.firstWhere((cell) => cell.text == 'B');
      final wide = row.renderCells.firstWhere((cell) => cell.text == '界');
      final wideIndex = row.renderCells.indexOf(wide);

      expect(plain.width, 1);
      expect(plain.style.bold, isFalse);
      expect(styled.style.bold, isTrue);
      expect(styled.style.italic, isTrue);
      expect(styled.style.foreground.tag, 1);
      expect(styled.style.foreground.paletteIndex, 1);
      expect(wide.width, 2);
      expect(wide.wide, 1);
      expect(row.renderCells[wideIndex + 1].width, 0);
      expect(row.renderCells[wideIndex + 1].wide, 2);
      expect(delta.styles.length, lessThan(row.renderCells.length));
    },
  );
}
