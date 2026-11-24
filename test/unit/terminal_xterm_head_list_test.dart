import 'package:alera/src/features/workbench/presentation/terminal_xterm_head_list.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'head trims preserve retained identities without shifting each time',
    () {
      final rows = TerminalXtermHeadList<Object>();
      final original = List<Object>.generate(2048, (_) => Object());
      rows.addAll(original);
      for (var index = 0; index < 100; index++) {
        rows.trimStart(1);
        rows.add(Object());
      }
      expect(rows.storageHead, 100);
      expect(rows.length, original.length);
      expect(rows.first, same(original[100]));
      expect(rows[1947], same(original.last));

      for (var index = 100; index < 2047; index++) {
        rows.trimStart(1);
        rows.add(Object());
      }
      // Compaction runs before the next append. After 2047 trims the
      // unused prefix is already half of the 4094 physical slots.
      expect(rows.storageHead, 0);
      expect(rows.first, same(original.last));
      final next = rows[1];
      rows.trimStart(1);
      rows.add(Object());
      expect(rows.storageHead, 1);
      expect(rows.first, same(next));
      expect(rows.length, original.length);
    },
  );

  test(
    'nullable worker caches preserve logical indices after trim and tail reuse',
    () {
      final first = Object();
      final last = Object();
      final rows = TerminalXtermHeadList<Object?>();
      rows.addAll([null, first, null, last]);
      rows.trimStart(1);
      expect(rows, [first, null, last]);
      rows.length = 2;
      rows.addAll([last, null]);
      expect(rows, [first, null, last, null]);
      rows[1] = last;
      expect(rows, [first, last, last, null]);
      rows.trimStart(rows.length);
      expect(rows, isEmpty);
      expect(rows.storageHead, 0);
      rows.add(null);
      expect(rows, [null]);
    },
  );
}
