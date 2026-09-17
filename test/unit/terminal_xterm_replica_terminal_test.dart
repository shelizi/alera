import 'package:alera/src/features/workbench/domain/terminal_osc52_clipboard.dart';
import 'package:alera/src/features/workbench/presentation/terminal_xterm_replica_terminal.dart';
import 'package:alera/src/features/workbench/presentation/terminal_xterm_worker.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:xterm2/core.dart';
import 'package:xterm2/xterm.dart';

void main() {
  test('replica reconstructs direct xterm buffer and renderer state', () async {
    final direct = _createDirect(cols: 16, rows: 5, maxLines: 64);
    final worker = await TerminalXtermWorker.start(
      cols: 16,
      rows: 5,
      maxLines: 64,
    );
    addTearDown(worker.close);
    final replica = TerminalXtermReplicaTerminal(
      cols: 16,
      rows: 5,
      maxLines: 64,
    );

    final escape = String.fromCharCode(27);
    final bell = String.fromCharCode(7);
    final st = '$escape\\';
    final sequence =
        '$escape[1;3;31mstyled$escape[0m 界 e\u0301\r\n'
        '$escape]8;;https://example.com$st'
        'link'
        '$escape]8;;$st'
        '$escape]4;2;#123456$bell'
        '$escape[?5h'
        '$escape[5 q';

    direct.write(sequence);
    final delta = await worker.writeBufferDelta(sequence);
    replica.applyBufferDelta(delta);

    _expectReplicaParity(replica, direct);
  });

  test(
    'replica applies partial tail changes without replacing retained rows',
    () async {
      final direct = _createDirect(cols: 12, rows: 3, maxLines: 25);
      final worker = await TerminalXtermWorker.start(
        cols: 12,
        rows: 3,
        maxLines: 25,
      );
      addTearDown(worker.close);
      final replica = TerminalXtermReplicaTerminal(
        cols: 12,
        rows: 3,
        maxLines: 25,
      );

      final initial = List<String>.generate(
        25,
        (index) => 'line-$index',
      ).join('\r\n');
      direct.write(initial);
      replica.applyBufferDelta(await worker.writeBufferDelta(initial));
      final retained = replica.buffer.lines[1];

      const tail = '\r\nnew-tail';
      direct.write(tail);
      final delta = await worker.writeBufferDelta(tail);
      expect(delta.fullRepaint, isFalse);
      expect(delta.trimStart, 1);
      replica.applyBufferDelta(delta);

      _expectReplicaParity(replica, direct);
      expect(identical(replica.buffer.lines[0], retained), isTrue);
    },
  );

  test('replica rebuilds direct xterm resize reflow immediately', () async {
    final direct = _createDirect(cols: 10, rows: 4, maxLines: 64);
    final worker = await TerminalXtermWorker.start(
      cols: 10,
      rows: 4,
      maxLines: 64,
    );
    addTearDown(worker.close);
    final replica = TerminalXtermReplicaTerminal(
      cols: 10,
      rows: 4,
      maxLines: 64,
    );

    const initial = 'abcdefghijABCDEFGHIJ\r\nend';
    direct.write(initial);
    replica.applyBufferDelta(await worker.writeBufferDelta(initial));

    direct.resize(6, 6);
    replica.applyBufferDelta(await worker.resizeBufferDelta(cols: 6, rows: 6));

    _expectReplicaParity(replica, direct);
  });

  test(
    'replica exposes alternate-buffer contents through normal buffer getter',
    () async {
      final direct = _createDirect(cols: 12, rows: 4, maxLines: 64);
      final worker = await TerminalXtermWorker.start(
        cols: 12,
        rows: 4,
        maxLines: 64,
      );
      addTearDown(worker.close);
      final replica = TerminalXtermReplicaTerminal(
        cols: 12,
        rows: 4,
        maxLines: 64,
      );
      final escape = String.fromCharCode(27);

      direct.write('main-content');
      replica.applyBufferDelta(await worker.writeBufferDelta('main-content'));

      final alt = '$escape[?1049halt-content';
      direct.write(alt);
      replica.applyBufferDelta(await worker.writeBufferDelta(alt));
      expect(replica.isUsingAltBuffer, isTrue);
      _expectReplicaParity(replica, direct);

      final back = '$escape[?1049l';
      direct.write(back);
      replica.applyBufferDelta(await worker.writeBufferDelta(back));
      expect(replica.isUsingAltBuffer, isFalse);
      _expectReplicaParity(replica, direct);
    },
  );

  test('replica input encoding matches direct xterm parser state', () async {
    final directOutput = <String>[];
    final replicaOutput = <String>[];
    final direct = _createDirect(
      cols: 20,
      rows: 6,
      maxLines: 64,
      platform: TerminalTargetPlatform.windows,
      onOutput: directOutput.add,
    );
    final worker = await TerminalXtermWorker.start(
      cols: 20,
      rows: 6,
      maxLines: 64,
      platform: TerminalTargetPlatform.windows,
    );
    addTearDown(worker.close);
    final replica = TerminalXtermReplicaTerminal(
      cols: 20,
      rows: 6,
      maxLines: 64,
      platform: TerminalTargetPlatform.windows,
      onOutput: replicaOutput.add,
    );
    final escape = String.fromCharCode(27);
    final modes =
        '$escape[20h'
        '$escape[?1h'
        '$escape='
        '$escape[?1035l'
        '$escape[?67h'
        '$escape[?1000h'
        '$escape[?1004h'
        '$escape[?1006h'
        '$escape[?2004h'
        '$escape[>4;2m';

    direct.write(modes);
    replica.applyBufferDelta(await worker.writeBufferDelta(modes));
    directOutput.clear();
    replicaOutput.clear();

    _expectInputCallParity(
      directOutput,
      replicaOutput,
      () => direct.keyInput(TerminalKey.arrowUp),
      () => replica.keyInput(TerminalKey.arrowUp),
    );
    _expectInputCallParity(
      directOutput,
      replicaOutput,
      () => direct.keyInput(TerminalKey.backspace),
      () => replica.keyInput(TerminalKey.backspace),
    );
    _expectInputCallParity(
      directOutput,
      replicaOutput,
      () => direct.keyInput(TerminalKey.numpad1, numLock: true),
      () => replica.keyInput(TerminalKey.numpad1, numLock: true),
    );
    _expectInputCallParity(
      directOutput,
      replicaOutput,
      () => direct.keyInput(TerminalKey.enter),
      () => replica.keyInput(TerminalKey.enter),
    );
    _expectInputCallParity(
      directOutput,
      replicaOutput,
      () =>
          direct.keyInput(TerminalKey.keyH, ctrl: true, shift: true, text: 'H'),
      () => replica.keyInput(
        TerminalKey.keyH,
        ctrl: true,
        shift: true,
        text: 'H',
      ),
    );

    direct.paste('a\nb');
    replica.paste('a\nb');
    expect(replicaOutput, directOutput);
    directOutput.clear();
    replicaOutput.clear();

    direct.focusInput(false);
    replica.focusInput(false);
    expect(replicaOutput, directOutput);
    directOutput.clear();
    replicaOutput.clear();

    final directMouseHandled = direct.mouseInput(
      TerminalMouseButton.left,
      TerminalMouseButtonState.down,
      const CellOffset(2, 1),
      modifiers: const TerminalMouseModifiers(alt: true),
    );
    final replicaMouseHandled = replica.mouseInput(
      TerminalMouseButton.left,
      TerminalMouseButtonState.down,
      const CellOffset(2, 1),
      modifiers: const TerminalMouseModifiers(alt: true),
    );
    expect(replicaMouseHandled, directMouseHandled);
    expect(replicaOutput, directOutput);

    directOutput.clear();
    replicaOutput.clear();
    final kitty = '$escape[=3u';
    direct.write(kitty);
    replica.applyBufferDelta(await worker.writeBufferDelta(kitty));
    _expectInputCallParity(
      directOutput,
      replicaOutput,
      () => direct.keyInput(
        TerminalKey.keyA,
        ctrl: true,
        type: TerminalKeyEventType.repeat,
        text: 'a',
      ),
      () => replica.keyInput(
        TerminalKey.keyA,
        ctrl: true,
        type: TerminalKeyEventType.repeat,
        text: 'a',
      ),
    );
  });

  test(
    'replica suppresses keyboard input while worker KAM is active',
    () async {
      final directOutput = <String>[];
      final replicaOutput = <String>[];
      final direct = _createDirect(
        cols: 12,
        rows: 4,
        maxLines: 64,
        onOutput: directOutput.add,
      );
      final worker = await TerminalXtermWorker.start(
        cols: 12,
        rows: 4,
        maxLines: 64,
      );
      addTearDown(worker.close);
      final replica = TerminalXtermReplicaTerminal(
        cols: 12,
        rows: 4,
        maxLines: 64,
        onOutput: replicaOutput.add,
      );
      final escape = String.fromCharCode(27);
      final keyboardActionMode = '$escape[2h';

      direct.write(keyboardActionMode);
      replica.applyBufferDelta(
        await worker.writeBufferDelta(keyboardActionMode),
      );
      directOutput.clear();
      replicaOutput.clear();

      expect(direct.keyInput(TerminalKey.keyA, text: 'a'), isFalse);
      expect(replica.keyInput(TerminalKey.keyA, text: 'a'), isFalse);
      direct.textInput('text');
      replica.textInput('text');
      direct.paste('paste');
      replica.paste('paste');
      expect(replicaOutput, directOutput);
      expect(replicaOutput, isEmpty);
    },
  );

  test('replica coalesces listener notifications while hidden', () async {
    final worker = await TerminalXtermWorker.start(
      cols: 12,
      rows: 4,
      maxLines: 64,
    );
    addTearDown(worker.close);
    final replica = TerminalXtermReplicaTerminal(
      cols: 12,
      rows: 4,
      maxLines: 64,
      notificationsEnabled: false,
    );
    var notifications = 0;
    replica.addListener(() => notifications += 1);

    replica.applyBufferDelta(await worker.writeBufferDelta('one'));
    replica.applyBufferDelta(await worker.writeBufferDelta('\r\ntwo'));
    expect(notifications, 0);

    replica.setNotificationsEnabled(true, flushPending: false);
    expect(notifications, 0);
    replica.flushPendingNotification();
    expect(notifications, 1);
    replica.flushPendingNotification();
    expect(notifications, 1);
  });
}

Terminal _createDirect({
  required int cols,
  required int rows,
  required int maxLines,
  TerminalTargetPlatform platform = TerminalTargetPlatform.unknown,
  void Function(String)? onOutput,
}) {
  return Terminal(
    maxLines: maxLines,
    platform: platform,
    onOutput: onOutput,
    reflowWithHiddenCursor: false,
    preserveOrphanCombiningMarks: true,
    allowITerm2ClipboardCapture: false,
    allowKittyClipboard: false,
    onClipboardQuery: (_) => null,
    clipboardDecoder: decodeTerminalOsc52Payload,
  )..resize(cols, rows);
}

void _expectInputCallParity(
  List<String> directOutput,
  List<String> replicaOutput,
  bool Function() directCall,
  bool Function() replicaCall,
) {
  directOutput.clear();
  replicaOutput.clear();
  expect(replicaCall(), directCall());
  expect(replicaOutput, directOutput);
}

void _expectReplicaParity(
  TerminalXtermReplicaTerminal replica,
  Terminal direct,
) {
  expect(replica.viewWidth, direct.viewWidth);
  expect(replica.viewHeight, direct.viewHeight);
  expect(replica.buffer.lines.length, direct.buffer.lines.length);
  expect(replica.buffer.scrollBack, direct.buffer.scrollBack);
  expect(replica.buffer.cursorX, direct.buffer.cursorX);
  expect(replica.buffer.cursorY, direct.buffer.cursorY);
  expect(replica.isUsingAltBuffer, direct.isUsingAltBuffer);
  expect(replica.reverseDisplayMode, direct.reverseDisplayMode);
  expect(replica.applicationCursorType, direct.applicationCursorType);
  expect(replica.cursorBlinkMode, direct.cursorBlinkMode);
  expect(replica.colorRevision, direct.colorRevision);
  expect(
    Map<int, int>.fromEntries(replica.indexedColorOverrides),
    Map<int, int>.fromEntries(direct.indexedColorOverrides),
  );

  for (var row = 0; row < direct.buffer.lines.length; row++) {
    final actual = replica.buffer.lines[row];
    final expected = direct.buffer.lines[row];
    expect(actual.length, expected.length, reason: 'row $row length');
    expect(actual.isWrapped, expected.isWrapped, reason: 'row $row wrap');
    for (var column = 0; column < expected.length; column++) {
      expect(actual.getForeground(column), expected.getForeground(column));
      expect(actual.getBackground(column), expected.getBackground(column));
      expect(actual.getAttributes(column), expected.getAttributes(column));
      expect(actual.getContent(column), expected.getContent(column));
      expect(
        actual.getUnderlineColor(column),
        expected.getUnderlineColor(column),
      );
      expect(
        actual.getCombiningCharacters(column),
        expected.getCombiningCharacters(column),
        reason: 'row $row column $column combining',
      );
      final position = CellOffset(column, row);
      expect(replica.hyperlinkIdAt(position), direct.hyperlinkIdAt(position));
      expect(replica.hyperlinkAt(position), direct.hyperlinkAt(position));
    }
    expect(replica.isSemanticPromptLine(row), direct.isSemanticPromptLine(row));
  }
}
