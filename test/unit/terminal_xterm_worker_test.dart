import 'dart:convert';

import 'package:alera/src/features/workbench/domain/terminal_osc52_clipboard.dart';
import 'package:alera/src/features/workbench/presentation/terminal_xterm_worker.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:xterm2/xterm.dart';

void main() {
  test(
    'direct snapshot hydration returns a reset packed full buffer',
    () async {
      final worker = await TerminalXtermWorker.start(cols: 20, rows: 6);
      addTearDown(worker.close);

      final snapshot =
          '${List<String>.filled(128 * 1024, '\x1b[0m').join()}'
          '\r\nsnapshot-marker\x1b[?2004h\x1b[?1000h';
      final profile = await worker.profileHydrateSnapshotBufferDelta(
        snapshot,
        resetInteractionModes: true,
      );
      final delta = profile.delta;

      expect(profile.parseMicros, greaterThanOrEqualTo(0));
      expect(profile.materializeMicros, greaterThanOrEqualTo(0));
      expect(profile.rawRoundtripMicros, greaterThanOrEqualTo(0));
      expect(profile.decodeMicros, greaterThanOrEqualTo(0));
      expect(delta.fullRepaint, isTrue);
      expect(delta.rowDeltas, isNotEmpty);
      expect(delta.bracketedPaste, isFalse);
      expect(delta.mouseMode, MouseMode.none.index);
      expect(
        delta.rowDeltas.map((row) => row.text).join('\n'),
        contains('snapshot-marker'),
      );
    },
  );

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

  test('worker buffer write applies supplied focus before parsing', () async {
    final worker = await TerminalXtermWorker.start(cols: 20, rows: 6);
    addTearDown(worker.close);
    final escape = String.fromCharCode(27);

    final delta = await worker.writeBufferDelta(
      '$escape[?1004h',
      focused: false,
    );

    expect(delta.effects, hasLength(1));
    expect(delta.effects.single, isA<TerminalXtermWorkerPtyWrite>());
    expect(
      (delta.effects.single as TerminalXtermWorkerPtyWrite).data,
      '$escape[O',
    );
  });

  test('worker mirrors synchronized update mode and generation', () async {
    final worker = await TerminalXtermWorker.start(cols: 20, rows: 6);
    addTearDown(worker.close);
    final escape = String.fromCharCode(27);

    final enabled = await worker.writeBufferDelta('$escape[?2026hframe-a');
    expect(enabled.globalState.synchronizedUpdate, isTrue);
    final generation = enabled.globalState.synchronizedUpdateGeneration;

    final middle = await worker.writeBufferDelta('frame-b');
    expect(middle.globalState.synchronizedUpdate, isTrue);
    expect(middle.globalState.synchronizedUpdateGeneration, generation);

    final disabled = await worker.writeBufferDelta('$escape[?2026lframe-c');
    expect(disabled.globalState.synchronizedUpdate, isFalse);
    expect(
      disabled.globalState.synchronizedUpdateGeneration,
      greaterThan(generation),
    );
  });

  test(
    'worker hidden parse defers buffer serialization until reveal',
    () async {
      final worker = await TerminalXtermWorker.start(
        cols: 12,
        rows: 4,
        maxLines: 64,
      );
      addTearDown(worker.close);

      final escape = String.fromCharCode(27);
      final initial = await worker.writeBufferDelta('visible\r\n');
      expect(initial.fullRepaint, isTrue);

      final hidden = await worker.parseHidden(
        'hidden-marker$escape[?2004h$escape]2;hidden-title\x07',
        focused: false,
      );
      expect(hidden.bracketedPaste, isTrue);
      expect(hidden.focusEvents, isFalse);
      expect(
        hidden.effects
            .whereType<TerminalXtermWorkerTitleChanged>()
            .single
            .title,
        'hidden-title',
      );

      final reveal = await worker.snapshotBufferDelta();
      expect(reveal.fullRepaint, isTrue);
      expect(reveal.trimStart, 0);
      expect(reveal.rowDeltas, hasLength(reveal.bufferLength));
      expect(reveal.revision, greaterThan(hidden.revision));
      expect(
        reveal.rowDeltas.map((row) => row.text).join(),
        contains('hidden-marker'),
      );
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

  test(
    'worker buffer delta preserves scrollback and streams tail changes',
    () async {
      final direct = _createDirectTerminal(cols: 8, rows: 3);
      final worker = await TerminalXtermWorker.start(
        cols: 8,
        rows: 3,
        maxLines: 64,
      );
      addTearDown(worker.close);
      final mirror = <String>[];

      const initial = 'one\r\ntwo\r\nthree\r\nfour';
      direct.write(initial);
      final first = await worker.writeBufferDelta(initial);
      expect(first.fullRepaint, isTrue);
      expect(first.trimStart, 0);
      _applyBufferDelta(mirror, first);
      expect(mirror, _bufferRows(direct));
      expect(first.bufferLength, direct.buffer.lines.length);
      expect(first.scrollBack, direct.buffer.scrollBack);

      direct.write('\r\nfive');
      final second = await worker.writeBufferDelta('\r\nfive');
      expect(second.fullRepaint, isFalse);
      expect(second.trimStart, 0);
      expect(second.rowDeltas.length, lessThan(second.bufferLength));
      _applyBufferDelta(mirror, second);
      expect(mirror, _bufferRows(direct));
    },
  );

  test(
    'worker buffer delta serializes only changed cells in retained row',
    () async {
      final worker = await TerminalXtermWorker.start(
        cols: 16,
        rows: 4,
        maxLines: 64,
      );
      addTearDown(worker.close);

      await worker.writeBufferDelta('abcdefghijkl');
      final delta = await worker.writeBufferDelta('\rZ');

      expect(delta.fullRepaint, isFalse);
      expect(delta.rowDeltas, hasLength(1));
      expect(delta.rowDeltas.single.row, 0);
      expect(delta.rowDeltas.single.cellStart, 0);
      expect(delta.rowDeltas.single.cells, hasLength(1));
      expect(delta.rowDeltas.single.text, startsWith('Zbcdef'));
    },
  );

  test('worker buffer delta preserves nonzero cell span offsets', () async {
    final worker = await TerminalXtermWorker.start(
      cols: 16,
      rows: 4,
      maxLines: 64,
    );
    addTearDown(worker.close);

    await worker.writeBufferDelta('abcdefghijkl');
    final delta = await worker.writeBufferDelta('\r\x1b[5CX');

    expect(delta.fullRepaint, isFalse);
    expect(delta.rowDeltas, hasLength(1));
    expect(delta.rowDeltas.single.cellStart, 5);
    expect(delta.rowDeltas.single.cells, hasLength(1));
    expect(delta.rowDeltas.single.text, 'abcdeXghijkl');
  });

  test('worker buffer delta reports circular scrollback trimming', () async {
    final direct = _createDirectTerminal(cols: 8, rows: 3, maxLines: 25);
    final worker = await TerminalXtermWorker.start(
      cols: 8,
      rows: 3,
      maxLines: 25,
    );
    addTearDown(worker.close);
    final mirror = <String>[];

    final initial = List<String>.generate(
      25,
      (index) => '${index + 1}',
    ).join('\r\n');
    direct.write(initial);
    _applyBufferDelta(mirror, await worker.writeBufferDelta(initial));
    expect(mirror, _bufferRows(direct));
    expect(direct.buffer.lines.length, 25);

    direct.write('\r\n26');
    final trimmed = await worker.writeBufferDelta('\r\n26');
    expect(trimmed.fullRepaint, isFalse);
    expect(trimmed.trimStart, 1);
    _applyBufferDelta(mirror, trimmed);
    expect(mirror, _bufferRows(direct));
    expect(trimmed.bufferLength, 25);
  });

  test('worker only compares rows that were active before the write', () async {
    final worker = await TerminalXtermWorker.start(
      cols: 8,
      rows: 3,
      maxLines: 25,
    );
    addTearDown(worker.close);
    final initial = List<String>.generate(
      25,
      (index) => '${index + 1}',
    ).join('\r\n');
    await worker.writeBufferDelta(initial);

    final delta = await worker.writeBufferDelta('\r\n26');

    expect(delta.fullRepaint, isFalse);
    expect(delta.trimStart, 1);
    expect(delta.comparedRowCount, lessThanOrEqualTo(3));
    expect(delta.cachedRowCount, lessThanOrEqualTo(3));
  });

  test(
    'worker compares an active row before it scrolls into history',
    () async {
      final direct = _createDirectTerminal(cols: 8, rows: 3, maxLines: 25);
      final worker = await TerminalXtermWorker.start(
        cols: 8,
        rows: 3,
        maxLines: 25,
      );
      addTearDown(worker.close);
      final mirror = <String>[];
      final initial = List<String>.generate(
        25,
        (index) => '${index + 1}',
      ).join('\r\n');
      direct.write(initial);
      _applyBufferDelta(mirror, await worker.writeBufferDelta(initial));

      const updateAndScroll = '\rX\r\n26';
      direct.write(updateAndScroll);
      final delta = await worker.writeBufferDelta(updateAndScroll);
      _applyBufferDelta(mirror, delta);

      expect(mirror, _bufferRows(direct));
      expect(mirror, contains('X5'));
    },
  );

  test(
    'worker keeps full scrollback in sync without revisiting it per echo',
    () async {
      final direct = _createDirectTerminal(cols: 8, rows: 4, maxLines: 40);
      final worker = await TerminalXtermWorker.start(
        cols: 8,
        rows: 4,
        maxLines: 40,
      );
      addTearDown(worker.close);
      final mirror = <String>[];
      final initial = List<String>.generate(
        60,
        (index) => '${index + 1}',
      ).join('\r\n');
      direct.write(initial);
      _applyBufferDelta(mirror, await worker.writeBufferDelta(initial));

      final steps = <String>[
        'a',
        'b',
        '\r\nnext',
        // Insert and delete lines inside a scroll region in the viewport.
        '\x1b[2;3r\x1b[2H\x1b[L\x1b[3H\x1b[M\x1b[r',
        '\r\nmore\r\nlines',
        // Clear the screen and then the scrollback.
        '\x1b[2J\x1b[Hfresh',
        '\x1b[3J',
        '\r\nafter',
        // The alternate buffer and back.
        '\x1b[?1049halt\x1b[?1049l',
        'c',
      ];
      for (final step in steps) {
        direct.write(step);
        final delta = await worker.writeBufferDelta(step);
        _applyBufferDelta(mirror, delta);
        expect(
          mirror,
          _bufferRows(direct),
          reason: 'after ${jsonEncode(step)}',
        );
        if (step.length == 1 && !delta.fullRepaint) {
          expect(delta.comparedRowCount, lessThanOrEqualTo(4));
        }
      }
    },
  );

  test('worker invalidates cached scrollback when CSI 3 J clears it', () async {
    final direct = _createDirectTerminal(cols: 8, rows: 3, maxLines: 25);
    final worker = await TerminalXtermWorker.start(
      cols: 8,
      rows: 3,
      maxLines: 25,
    );
    addTearDown(worker.close);
    final mirror = <String>[];
    final initial = List<String>.generate(
      12,
      (index) => '${index + 1}',
    ).join('\r\n');
    direct.write(initial);
    _applyBufferDelta(mirror, await worker.writeBufferDelta(initial));

    direct.write('\x1b[3J');
    final delta = await worker.writeBufferDelta('\x1b[3J');
    _applyBufferDelta(mirror, delta);

    expect(mirror, _bufferRows(direct));
    expect(delta.scrollBack, 0);
  });

  test(
    'worker buffer delta carries global render state and hyperlinks',
    () async {
      final direct = _createDirectTerminal(cols: 16, rows: 4);
      final worker = await TerminalXtermWorker.start(
        cols: 16,
        rows: 4,
        maxLines: 64,
      );
      addTearDown(worker.close);

      final escape = String.fromCharCode(27);
      final bell = String.fromCharCode(7);
      final stringTerminator = '$escape\\';
      final sequence =
          '$escape[?1049h'
          '$escape[?5h'
          '$escape[?1036l$escape[?1039h'
          '$escape[>1s'
          '$escape[5 q'
          '$escape]4;1;#112233$bell'
          '$escape]10;#010203$bell'
          '$escape]8;;https://example.com$stringTerminator'
          'link'
          '$escape]8;;$stringTerminator';
      direct.write(sequence);
      final delta = await worker.writeBufferDelta(sequence);

      expect(delta.globalState.isUsingAltBuffer, direct.isUsingAltBuffer);
      expect(delta.globalState.reverseDisplay, direct.reverseDisplayMode);
      expect(delta.globalState.cursorType, direct.applicationCursorType?.index);
      expect(delta.globalState.cursorBlink, direct.cursorBlinkMode);
      expect(
        delta.globalState.cursorLineHighlight,
        direct.cursorLineHighlightMode,
      );
      expect(delta.globalState.mouseShiftCapture, direct.mouseShiftCaptureMode);
      expect(delta.globalState.altEscPrefix, direct.altEscPrefixMode);
      expect(delta.globalState.altSendsEscape, direct.altSendsEscapeMode);
      expect(delta.globalState.colorRevision, direct.colorRevision);
      expect(
        delta.globalState.indexedColorOverrides,
        Map<int, int>.fromEntries(direct.indexedColorOverrides),
      );
      expect(
        delta.globalState.specialColorOverrides,
        Map<int, int>.fromEntries(direct.specialColorOverrides),
      );
      expect(
        delta.globalState.foregroundColorOverride,
        direct.foregroundColorOverride,
      );
      expect(
        delta.globalState.backgroundColorOverride,
        direct.backgroundColorOverride,
      );
      expect(delta.globalState.cursorColorOverride, direct.cursorColorOverride);
      expect(
        delta.globalState.selectionColorOverride,
        direct.selectionColorOverride,
      );
      expect(
        delta.globalState.selectionForegroundColorOverride,
        direct.selectionForegroundColorOverride,
      );

      var foundHyperlink = false;
      for (var row = 0; row < direct.buffer.lines.length; row++) {
        final line = direct.buffer.lines[row];
        for (var column = 0; column < line.length; column++) {
          final hyperlinkId = line.getHyperlinkId(column);
          if (hyperlinkId == 0) continue;
          foundHyperlink = true;
          expect(
            delta.hyperlinkUpdates[hyperlinkId],
            direct.hyperlinkAt(CellOffset(column, row)),
          );
          final changedRow = delta.rowDeltas.singleWhere(
            (item) => item.row == row,
          );
          expect(changedRow.cells[column].hyperlinkId, hyperlinkId);
          expect(
            changedRow.cells[column].semanticAttributes,
            line.getSemanticContent(column),
          );
        }
      }
      expect(foundHyperlink, isTrue);
    },
  );

  test(
    'retained worker state round-trips and preserves subsequent parsing',
    () async {
      final control = await TerminalXtermWorker.start(cols: 20, rows: 6);
      addTearDown(control.close);
      var original = await TerminalXtermWorker.start(cols: 20, rows: 6);
      addTearDown(() async {
        await original.close();
      });
      final prefix =
          '\x1b[31;1mmain-state'
          '\x1b7'
          '\x1b[?1h'
          '\x1b[?2004h'
          '\x1b[2;5r'
          '\x1b]1337;HighlightCursorLine=yes\x07'
          '\x1b[?1049h'
          '\x1b[32malt-state';

      await control.writeBufferDelta(prefix);
      await original.writeBufferDelta(prefix);
      final exported = await original.exportRetainedState();
      expect(exported.eligible, isTrue, reason: exported.blockers.join(', '));
      expect(exported.blockers, isEmpty);

      await original.close();
      original = await TerminalXtermWorker.start(
        cols: 20,
        rows: 6,
        retainedState: exported.state,
      );

      final beforeControl = await control.snapshotBufferDelta();
      final beforeRestored = await original.snapshotBufferDelta();
      _expectBufferDeltaParity(beforeRestored, beforeControl);

      final suffix =
          '\x1b[0m!'
          '\x1b[3b'
          '\x1b[?1049l'
          'R'
          '\x1b8'
          '\x1b]1337;HighlightCursorLine=no\x07'
          '\x1b[?1l'
          '\x1b[?2004l';
      final controlAfter = await control.writeBufferDelta(suffix);
      final restoredAfter = await original.writeBufferDelta(suffix);
      _expectBufferDeltaParity(restoredAfter, controlAfter);
    },
  );

  test('retained worker state rejects a non-ground parser', () async {
    final worker = await TerminalXtermWorker.start(cols: 20, rows: 6);
    addTearDown(worker.close);

    await worker.writeBufferDelta('\x1b[31');
    final blocked = await worker.exportRetainedState();
    expect(blocked.eligible, isFalse);
    expect(blocked.blockers, contains('parser-not-ground'));

    await worker.writeBufferDelta('mred\x1b[0m');
    final safe = await worker.exportRetainedState();
    expect(safe.blockers, isNot(contains('parser-not-ground')));
  });

  test('retained worker state rejects semantic shell state', () async {
    final worker = await TerminalXtermWorker.start(cols: 20, rows: 6);
    addTearDown(worker.close);

    await worker.writeBufferDelta('\x1b]133;D;2\x07');
    final exported = await worker.exportRetainedState();

    expect(exported.eligible, isFalse);
    expect(exported.blockers, contains('semantic-shell-state'));
  });

  test('retained worker state rejects auxiliary dynamic colors', () async {
    final worker = await TerminalXtermWorker.start(cols: 20, rows: 6);
    addTearDown(worker.close);

    await worker.writeBufferDelta('\x1b]13;#02468a\x07');
    final exported = await worker.exportRetainedState();

    expect(exported.eligible, isFalse);
    expect(exported.blockers, contains('custom-colors'));
  });

  test('retained worker state rejects unsupported hyperlink state', () async {
    final worker = await TerminalXtermWorker.start(cols: 20, rows: 6);
    addTearDown(worker.close);

    await worker.writeBufferDelta(
      '\x1b]8;;https://example.test\x07link\x1b]8;;\x07',
    );
    final exported = await worker.exportRetainedState();

    expect(exported.eligible, isFalse);
    expect(exported.blockers, contains('hyperlinks'));
  });

  test('worker resize preserves in-band pixel size reports', () async {
    final directOutput = <String>[];
    final direct = _createDirectTerminal(
      cols: 10,
      rows: 4,
      onOutput: directOutput.add,
    );
    final worker = await TerminalXtermWorker.start(cols: 10, rows: 4);
    addTearDown(worker.close);
    final escape = String.fromCharCode(27);

    direct.write('$escape[?2048h');
    await worker.writeBufferDelta('$escape[?2048h');
    directOutput.clear();

    direct.resize(12, 5, 8, 16);
    final delta = await worker.resizeBufferDelta(
      cols: 12,
      rows: 5,
      pixelWidth: 8,
      pixelHeight: 16,
    );

    _expectEffectsParity(delta.effects, <TerminalXtermWorkerEffect>[
      for (final output in directOutput) TerminalXtermWorkerPtyWrite(output),
    ]);
    expect(directOutput, <String>['$escape[48;5;12;80;96t']);
  });
}

Terminal _createDirectTerminal({
  required int cols,
  required int rows,
  int maxLines = 256,
  TerminalTargetPlatform platform = TerminalTargetPlatform.unknown,
  Set<int>? wordSeparators,
  void Function(String title)? onTitleChange,
  void Function()? onBell,
  void Function(String data)? onOutput,
  void Function(String selector, String text)? onClipboardStore,
}) {
  return Terminal(
    maxLines: maxLines,
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

List<String> _bufferRows(Terminal terminal) {
  return <String>[
    for (var row = 0; row < terminal.buffer.lines.length; row++)
      terminal.buffer.lines[row].toString(),
  ];
}

void _expectBufferDeltaParity(
  TerminalXtermWorkerBufferDelta actual,
  TerminalXtermWorkerBufferDelta expected,
) {
  expect(actual.cols, expected.cols);
  expect(actual.rows, expected.rows);
  expect(actual.bufferLength, expected.bufferLength);
  expect(actual.scrollBack, expected.scrollBack);
  expect(actual.cursorX, expected.cursorX);
  expect(actual.cursorY, expected.cursorY);
  expect(actual.cursorVisible, expected.cursorVisible);
  expect(actual.cursorKeys, expected.cursorKeys);
  expect(actual.keypadKeys, expected.keypadKeys);
  expect(actual.bracketedPaste, expected.bracketedPaste);
  expect(actual.focusEvents, expected.focusEvents);
  expect(actual.altScroll, expected.altScroll);
  expect(actual.mouseMode, expected.mouseMode);
  expect(actual.mouseReportMode, expected.mouseReportMode);
  expect(
    actual.globalState.keyboardActionMode,
    expected.globalState.keyboardActionMode,
  );
  expect(
    actual.globalState.cursorLineHighlight,
    expected.globalState.cursorLineHighlight,
  );
  expect(actual.rowDeltas, hasLength(expected.rowDeltas.length));
  for (var rowIndex = 0; rowIndex < expected.rowDeltas.length; rowIndex++) {
    final actualRow = actual.rowDeltas[rowIndex];
    final expectedRow = expected.rowDeltas[rowIndex];
    expect(actualRow.row, expectedRow.row);
    expect(actualRow.rowLength, expectedRow.rowLength);
    expect(actualRow.text, expectedRow.text);
    expect(actualRow.isWrapped, expectedRow.isWrapped);
    expect(actualRow.cells, hasLength(expectedRow.cells.length));
    for (var cellIndex = 0; cellIndex < expectedRow.cells.length; cellIndex++) {
      final actualCell = actualRow.cells[cellIndex];
      final expectedCell = expectedRow.cells[cellIndex];
      expect(actualCell.width, expectedCell.width);
      expect(actualCell.foreground, expectedCell.foreground);
      expect(actualCell.background, expectedCell.background);
      expect(actualCell.attributes, expectedCell.attributes);
      expect(actualCell.underlineColor, expectedCell.underlineColor);
      expect(actualCell.content, expectedCell.content);
      expect(actualCell.combiningCharacters, expectedCell.combiningCharacters);
    }
  }
}

void _applyBufferDelta(
  List<String> mirror,
  TerminalXtermWorkerBufferDelta delta,
) {
  if (delta.fullRepaint) {
    mirror.clear();
  } else if (delta.trimStart > 0) {
    mirror.removeRange(0, delta.trimStart);
  }
  for (final row in delta.rowDeltas) {
    if (row.row == mirror.length) {
      mirror.add(row.text);
    } else {
      mirror[row.row] = row.text;
    }
  }
  expect(mirror, hasLength(delta.bufferLength));
}

void _applyDelta(List<String> mirror, TerminalXtermWorkerDelta delta) {
  for (final row in delta.rowDeltas) {
    mirror[row.row] = row.text;
  }
}
