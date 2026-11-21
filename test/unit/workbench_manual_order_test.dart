import 'package:alera/src/features/workbench/application/workbench_manual_order.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('automatic sort never accepts a sidebar reorder', () {
    final next = moveSidebarOrderId(
      savedOrder: const <String>[],
      visibleOrder: const <String>['a', 'b'],
      knownIds: const <String>['a', 'b'],
      sourceId: 'a',
      targetId: 'b',
      isManual: false,
    );
    expect(next, isNull);
  });

  test('manual sort preserves hidden IDs without mutating metadata', () {
    final next = moveSidebarOrderId(
      savedOrder: const <String>['a', 'hidden', 'b'],
      visibleOrder: const <String>['a', 'b'],
      knownIds: const <String>['a', 'hidden', 'b'],
      sourceId: 'a',
      targetId: 'b',
      isManual: true,
    );
    expect(next, <String>['hidden', 'b', 'a']);
  });
}
