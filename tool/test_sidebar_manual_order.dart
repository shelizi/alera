// Run without Flutter dependencies: dart tool/test_sidebar_manual_order.dart
import '../lib/src/features/workbench/application/workbench_manual_order.dart';

void expectOrder(List<String>? actual, String expected) {
  if (actual == null || actual.join(',') != expected) {
    throw StateError('Expected $expected, got ${actual?.join(',')}');
  }
}

void main() {
  final before = <String>['p-a', 'hidden', 'p-b', 'p-c'];
  final visible = <String>['p-a', 'p-b', 'p-c'];
  final afterDown = moveSidebarOrderId(
    savedOrder: before,
    visibleOrder: visible,
    knownIds: before,
    sourceId: 'p-a',
    targetId: 'p-c',
    isManual: true,
  );
  expectOrder(afterDown, 'hidden,p-b,p-c,p-a');
  final afterUp = moveSidebarOrderId(
    savedOrder: before,
    visibleOrder: visible,
    knownIds: before,
    sourceId: 'p-c',
    targetId: 'p-a',
    isManual: true,
  );
  expectOrder(afterUp, 'p-c,p-a,hidden,p-b');
  final firstMove = moveSidebarOrderId(
    savedOrder: const [],
    visibleOrder: visible,
    knownIds: before,
    sourceId: 'p-a',
    targetId: 'p-b',
    isManual: false,
  );
  expectOrder(firstMove, 'p-b,p-a,p-c,hidden');
  if (moveSidebarOrderId(
        savedOrder: before,
        visibleOrder: visible,
        knownIds: before,
        sourceId: 'p-a',
        targetId: 'p-a',
        isManual: true,
      ) !=
      null) {
    throw StateError('Self drop must do nothing');
  }
  if (moveSidebarOrderId(
        savedOrder: before,
        visibleOrder: visible,
        knownIds: before,
        sourceId: 'unknown',
        targetId: 'p-a',
        isManual: true,
      ) !=
      null) {
    throw StateError('Unknown ID must be rejected');
  }
  print('Manual sidebar ordering: 5 cases passed');
}
