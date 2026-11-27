import 'package:flutter_directory_tree/src/controller/selection_controller.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('single mode selectOnly replaces selection', () {
    final controller = SelectionController()
      ..selectOnly('node1')
      ..selectOnly('node2');

    expect(controller.selectedIds, equals({'node2'}));
  });

  test('multi mode toggle and range selection', () {
    final controller = SelectionController(mode: SelectionMode.multi)
      ..toggle('a')
      ..toggle('b');
    expect(controller.selectedIds, equals({'a', 'b'}));

    controller.toggle('a');
    expect(controller.selectedIds, equals({'b'}));

    controller.selectRange(['a', 'b', 'c', 'd'], 'b', 'd');
    expect(controller.selectedIds, equals({'b', 'c', 'd'}));

    controller.clear();
    expect(controller.selectedIds, isEmpty);
  });

  test('retainWhere keeps only matching ids', () {
    final controller = SelectionController(mode: SelectionMode.multi)
      ..toggle('keep')
      ..toggle('drop')
      ..retainWhere((id) => id == 'keep');
    expect(controller.selectedIds, equals({'keep'}));
  });
}
