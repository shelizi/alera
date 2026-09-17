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
}
