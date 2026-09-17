import 'dart:convert';
import 'dart:typed_data';

import 'package:alera/src/features/workbench/presentation/terminal_vt_viewport_model.dart';
import 'package:alera/src/features/workbench/presentation/terminal_vt_worker.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('rebuilds a viewport from full and partial worker deltas', () async {
    final worker = await TerminalVtWorker.start(
      cols: 12,
      rows: 4,
      maxScrollback: 32,
    );
    addTearDown(worker.close);
    final viewport = TerminalVtViewportModel();

    final initial = await worker.writeDelta(
      Uint8List.fromList(utf8.encode('one\r\ntwo')),
    );
    viewport.apply(initial);

    expect(viewport.revision, initial.revision);
    expect(viewport.cols, 12);
    expect(viewport.rows, 4);
    expect(viewport.rowText(0), startsWith('one'));
    expect(viewport.rowText(1), startsWith('two'));
    final untouchedFirstRow = viewport.renderRows[0];

    final partial = await worker.writeDelta(
      Uint8List.fromList(utf8.encode('X')),
    );
    expect(partial.fullRepaint, isFalse);
    viewport.apply(partial);

    expect(viewport.revision, partial.revision);
    expect(viewport.rowText(0), startsWith('one'));
    expect(viewport.rowText(1), startsWith('twoX'));
    expect(identical(viewport.renderRows[0], untouchedFirstRow), isTrue);
    expect(viewport.cursorVisible, partial.modes.cursorVisible);
    expect(viewport.modes.bracketedPaste, partial.modes.bracketedPaste);
  });
}
