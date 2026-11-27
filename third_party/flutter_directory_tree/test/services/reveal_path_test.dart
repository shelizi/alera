import 'package:flutter_directory_tree/src/controller/directory_tree_controller.dart';
import 'package:flutter_directory_tree/src/services/reveal_path.dart';
import 'package:flutter_test/flutter_test.dart';

import '../helpers/test_tree.dart';

void main() {
  test('ancestorChain returns nodes from root to target', () {
    final tree = buildTestTreeData();
    final chain = ancestorChain(tree, 'draft');

    expect(chain, equals(['root', 'home', 'docs', 'draft']));
  });

  test('findByVirtualPath finds matching node', () {
    final tree = buildTestTreeData();

    expect(findByVirtualPath(tree, '/home/docs/draft.md'), 'draft');
    expect(findByVirtualPath(tree, '/no/such/path'), isNull);
  });

  test('revealAndSelect expands ancestors and selects node', () async {
    final tree = buildTestTreeData();
    final controller = DirectoryTreeController(data: tree);

    await revealAndSelect(controller, nodeId: 'draft');

    expect(controller.selection.selectedIds, contains('draft'));
    expect(controller.expansions.isExpanded('docs'), isTrue);
    expect(controller.expansions.isExpanded('home'), isTrue);
  });
}
