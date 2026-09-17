import 'dart:convert';

import 'package:alera/src/features/workbench/domain/terminal_osc52_clipboard.dart';
import 'package:alera/src/features/workbench/presentation/terminal_xterm_worker.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:xterm2/xterm.dart';

void main() {
  test(
    'worker-owned xterm matches direct xterm through writes and resize',
    () async {
      final direct = _createDirectTerminal(
        cols: 10,
        rows: 4,
        platform: TerminalTargetPlatform.windows,
        wordSeparators: const <int>{0x2f, 0x5c},
      );
      final worker = await TerminalXtermWorker.start(
        cols: 10,
        rows: 4,
        maxLines: 256,
        platform: TerminalTargetPlatform.windows,
        wordSeparators: const <int>{0x2f, 0x5c},
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
      final directEffects = <TerminalXtermWorkerEffect>[];
      final direct = _createDirectTerminal(
        cols: 20,
        rows: 6,
        onTitleChange: (title) =>
            directEffects.add(TerminalXtermWorkerTitleChanged(title)),
        onBell: () => directEffects.add(const TerminalXtermWorkerBell()),
        onOutput: (value) =>
            directEffects.add(TerminalXtermWorkerPtyWrite(value)),
        onClipboardStore: (selector, text) => directEffects.add(
          TerminalXtermWorkerClipboardStore(selector: selector, text: text),
        ),
      );
      final worker = await TerminalXtermWorker.start(
        cols: 20,
        rows: 6,
        maxLines: 256,
      );
      addTearDown(worker.close);

      final clipboardPayload = base64.encode(utf8.encode('copied text'));
      final text =
          '\x1b]2;worker-title\x07\x07\x1b[6n'
          '\x1b[?1h\x1b=\x1b[?25l\x1b[?1000h\x1b[?1004h'
          '\x1b[?1006h\x1b[?1007h\x1b[?2004h'
          '\x1b]52;c;$clipboardPayload\x07';
      direct.write(text);
      final snapshot = await worker.write(text);

      _expectParity(snapshot, direct);
      _expectEffectsParity(snapshot.effects, directEffects);
    },
  );

  test(
    'worker preserves orphan combining marks like the Alera terminal',
    () async {
      final direct = _createDirectTerminal(cols: 8, rows: 3);
      final worker = await TerminalXtermWorker.start(
        cols: 8,
        rows: 3,
        maxLines: 256,
      );
      addTearDown(worker.close);

      const text = '\u0301A';
      direct.write(text);
      final snapshot = await worker.write(text);

      _expectParity(snapshot, direct);
    },
  );

  test('worker input encoding matches direct xterm state', () async {
    final directEffects = <TerminalXtermWorkerEffect>[];
    final direct = _createDirectTerminal(
      cols: 20,
      rows: 6,
      onOutput: (value) =>
          directEffects.add(TerminalXtermWorkerPtyWrite(value)),
    );
    final worker = await TerminalXtermWorker.start(
      cols: 20,
      rows: 6,
      maxLines: 256,
    );
    addTearDown(worker.close);

    final escape = String.fromCharCode(27);
    final modes =
        '$escape[?1h$escape[?1000h$escape[?1004h$escape[?1006h$escape[?2004h';
    direct.write(modes);
    await worker.writeDelta(modes);
    directEffects.clear();

    final directKeyHandled = direct.keyInput(TerminalKey.arrowUp);
    final key = await worker.keyInput(TerminalKey.arrowUp);
    expect(key.handled, directKeyHandled);
    expect(key.revision, 2);
    _expectEffectsParity(key.effects, directEffects);

    directEffects.clear();
    direct.textInput('typed');
    final text = await worker.textInput('typed');
    expect(text.handled, isNull);
    _expectEffectsParity(text.effects, directEffects);

    directEffects.clear();
    direct.paste('a\nb');
    final paste = await worker.paste('a\nb');
    expect(paste.handled, isNull);
    _expectEffectsParity(paste.effects, directEffects);

    directEffects.clear();
    direct.focusInput(true);
    final focus = await worker.focusInput(true);
    expect(focus.handled, isNull);
    _expectEffectsParity(focus.effects, directEffects);

    directEffects.clear();
    final directMouseHandled = direct.mouseInput(
      TerminalMouseButton.left,
      TerminalMouseButtonState.down,
      const CellOffset(2, 1),
      modifiers: const TerminalMouseModifiers(alt: true),
    );
    final mouse = await worker.mouseInput(
      TerminalMouseButton.left,
      TerminalMouseButtonState.down,
      const CellOffset(2, 1),
      modifiers: const TerminalMouseModifiers(alt: true),
    );
    expect(mouse.handled, directMouseHandled);
    _expectEffectsParity(mouse.effects, directEffects);
  });

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
    expect(first.revision, 1);
    expect(first.fullRepaint, isTrue);
    expect(first.rowDeltas, hasLength(4));
    _applyDelta(mirror, first);
    expect(mirror, _viewportRows(direct));

    direct.write('!');
    final second = await worker.writeDelta('!');
    expect(second.revision, 2);
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
    expect(delta.revision, 2);
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

Terminal _createDirectTerminal({
  required int cols,
  required int rows,
  TerminalTargetPlatform platform = TerminalTargetPlatform.unknown,
  Set<int>? wordSeparators,
  void Function(String title)? onTitleChange,
  void Function()? onBell,
  void Function(String data)? onOutput,
  void Function(String selector, String text)? onClipboardStore,
}) {
  return Terminal(
    maxLines: 256,
    reflowWithHiddenCursor: false,
    preserveOrphanCombiningMarks: true,
    allowITerm2ClipboardCapture: false,
    allowKittyClipboard: false,
    onClipboardQuery: (_) => null,
    clipboardDecoder: decodeTerminalOsc52Payload,
    platform: platform,
    wordSeparators: wordSeparators,
    onTitleChange: onTitleChange,
    onBell: onBell,
    onOutput: onOutput,
    onClipboardStore: onClipboardStore,
  )..resize(cols, rows);
}

void _expectEffectsParity(
  List<TerminalXtermWorkerEffect> actual,
  List<TerminalXtermWorkerEffect> expected,
) {
  expect(actual, hasLength(expected.length));
  for (var index = 0; index < expected.length; index++) {
    final actualEffect = actual[index];
    switch (expected[index]) {
      case TerminalXtermWorkerTitleChanged(:final title):
        expect(actualEffect, isA<TerminalXtermWorkerTitleChanged>());
        expect((actualEffect as TerminalXtermWorkerTitleChanged).title, title);
      case TerminalXtermWorkerBell():
        expect(actualEffect, isA<TerminalXtermWorkerBell>());
      case TerminalXtermWorkerPtyWrite(:final data):
        expect(actualEffect, isA<TerminalXtermWorkerPtyWrite>());
        expect((actualEffect as TerminalXtermWorkerPtyWrite).data, data);
      case TerminalXtermWorkerClipboardStore(:final selector, :final text):
        expect(actualEffect, isA<TerminalXtermWorkerClipboardStore>());
        final clipboard = actualEffect as TerminalXtermWorkerClipboardStore;
        expect(clipboard.selector, selector);
        expect(clipboard.text, text);
    }
  }
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
