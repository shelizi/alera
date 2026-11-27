import 'package:flutter_directory_tree/src/controller/expansion_controller.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('setExpanded adds and removes ids', () {
    final controller = ExpansionController()..setExpanded('a', true);
    expect(controller.isExpanded('a'), isTrue);

    controller.setExpanded('a', false);
    expect(controller.isExpanded('a'), isFalse);
  });

  test('toggle flips expansion state', () {
    final controller = ExpansionController()..toggle('b');
    expect(controller.isExpanded('b'), isTrue);

    controller.toggle('b');
    expect(controller.isExpanded('b'), isFalse);
  });

  test('expandAll, collapseAll, retainWhere manage bulk state', () {
    final controller = ExpansionController(initiallyExpanded: {'a'})
      ..expandAll(['b', 'c']);
    expect(controller.expandedIds, containsAll({'a', 'b', 'c'}));

    controller.retainWhere((id) => id != 'b');
    expect(controller.expandedIds, isNot(contains('b')));

    controller.collapseAll();
    expect(controller.expandedIds, isEmpty);
  });

  test('performBatch coalesces notifications and supports nesting', () {
    final controller = ExpansionController();
    var notifications = 0;
    controller
      ..addListener(() => notifications++)
      ..performBatch(() {
        controller
          ..setExpanded('a', true)
          ..setExpanded('b', true);
      });

    expect(controller.expandedIds, containsAll({'a', 'b'}));
    expect(notifications, 1);

    notifications = 0;
    controller.performBatch(() {
      controller
        ..performBatch(() {
          controller.setExpanded('c', true);
        })
        ..setExpanded('a', false);
    });

    expect(controller.expandedIds, containsAll({'b', 'c'}));
    expect(controller.expandedIds, isNot(contains('a')));
    expect(notifications, 1);
  });
}
