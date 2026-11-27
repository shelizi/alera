import 'package:directory_tree/directory_tree.dart';
import 'package:flutter/material.dart';
import 'package:flutter_directory_tree/src/controller/directory_tree_controller.dart';
import 'package:flutter_directory_tree/src/controller/selection_controller.dart';
import 'package:flutter_directory_tree/src/theme/directory_tree_theme.dart';
import 'package:flutter_directory_tree/src/widgets/directory_tree_view.dart';
import 'package:flutter_test/flutter_test.dart';

import '../helpers/test_tree.dart';

List<String> _visibleIds(DirectoryTreeController controller) =>
    controller.visibleNodes.map((n) => n.id).toList(growable: false);

void main() {
  test('constructor seeds expansion hints from tree data', () {
    final tree = buildTestTreeData();
    final controller = DirectoryTreeController(data: tree);

    expect(controller.expansions.isExpanded(tree.visibleRootId), isTrue);
    expect(controller.expansions.isExpanded('root'), isTrue);
    expect(controller.expansions.isExpanded('docs'), isFalse);
  });

  test('constructor seeds selection hints from tree data when enabled', () {
    final base = buildTestTreeData();
    final nodes = Map<String, TreeNode>.from(base.nodes)
      ..['draft'] = base.nodes['draft']!.copyWith(isSelected: true);
    final tree = TreeData(
      nodes: nodes,
      rootId: base.rootId,
      visibleRootId: base.visibleRootId,
    );

    final seeded = DirectoryTreeController(data: tree);
    expect(seeded.selection.selectedIds, equals({'draft'}));

    final skipped = DirectoryTreeController(
      data: tree,
      seedSelectionFromCore: false,
    );
    expect(skipped.selection.selectedIds, isEmpty);
  });

  test('visibleNodes reflect flatten results and respond to filter', () {
    final controller = DirectoryTreeController(data: buildTestTreeData());

    expect(
      _visibleIds(controller),
      equals(['home', 'docs', 'pictures', 'readme']),
    );

    controller.filterQuery = '  vaca ';

    expect(controller.filterQuery, 'vaca');
    expect(
      _visibleIds(controller),
      equals(['home', 'pictures', 'vacation']),
    );
  });

  test('expand, collapse, and toggle update expansion state', () {
    final controller = DirectoryTreeController(data: buildTestTreeData())
      ..expand('docs');
    expect(controller.expansions.isExpanded('docs'), isTrue);
    expect(_visibleIds(controller), containsAll(['notes', 'draft']));

    controller.collapse('docs');
    expect(controller.expansions.isExpanded('docs'), isFalse);
    expect(_visibleIds(controller), isNot(contains('notes')));

    controller.toggle('pictures');
    expect(controller.expansions.isExpanded('pictures'), isTrue);
  });

  test(
      'recursive expand and collapse affect descendants while keeping root open',
      () {
    final controller = DirectoryTreeController(data: buildTestTreeData())
      ..expand('home', recursive: true);
    expect(controller.expansions.expandedIds,
        containsAll({'home', 'docs', 'pictures'}));

    controller.collapse('home', recursive: true);
    expect(controller.expansions.expandedIds, equals({'root'}));
  });

  test('selectRange and clearSelection manipulate selection controller', () {
    final selection = SelectionController(mode: SelectionMode.multi);
    final controller = DirectoryTreeController(
      data: buildTestTreeData(),
      selection: selection,
    )
      ..expand('docs')
      ..selectRange(anchorId: 'docs', toId: 'notes');
    expect(controller.selection.selectedIds, equals({'docs', 'notes'}));

    controller.clearSelection();
    expect(controller.selection.selectedIds, isEmpty);
  });

  test('rebuild preserves or resets state based on flag', () {
    final tree = buildTestTreeData();
    final controller = DirectoryTreeController(data: tree)
      ..expand('docs')
      ..selectOnly('notes');

    final updatedNodes = Map<String, TreeNode>.from(tree.nodes)
      ..remove('pictures');
    updatedNodes['home'] = updatedNodes['home']!.copyWith(
      childIds: const ['docs', 'readme'],
    );
    final next = TreeData(
      nodes: updatedNodes,
      rootId: tree.rootId,
      visibleRootId: tree.visibleRootId,
    );

    controller.rebuild(next);

    expect(controller.expansions.isExpanded('docs'), isTrue);
    expect(controller.selection.isSelected('notes'), isTrue);
    expect(controller.expansions.isExpanded('pictures'), isFalse);

    controller.rebuild(tree, tryPreserveState: false);
    expect(controller.expansions.expandedIds, equals({tree.visibleRootId}));
    expect(controller.selection.selectedIds, isEmpty);
  });

  test('rebuild with reseedFromCore honors new expansion hints', () {
    final tree = buildTestTreeData();
    final controller = DirectoryTreeController(data: tree)..expand('docs');

    final updatedNodes = Map<String, TreeNode>.from(tree.nodes);
    updatedNodes['docs'] = updatedNodes['docs']!.copyWith(isExpanded: true);
    updatedNodes['pictures'] = updatedNodes['pictures']!.copyWith(
      isExpanded: false,
    );
    updatedNodes['draft'] = updatedNodes['draft']!.copyWith(isSelected: true);
    final next = TreeData(
      nodes: updatedNodes,
      rootId: tree.rootId,
      visibleRootId: tree.visibleRootId,
    );

    controller.rebuild(next, tryPreserveState: false, reseedFromCore: true);

    expect(controller.expansions.expandedIds,
        equals({tree.visibleRootId, 'root', 'docs'}));
    expect(controller.expansions.isExpanded('pictures'), isFalse);
    expect(controller.selection.selectedIds, equals({'draft'}));
  });

  test('reveal expands ancestors and optional selection', () async {
    final controller = DirectoryTreeController(data: buildTestTreeData());

    await controller.reveal(nodeId: 'draft', select: true);

    expect(controller.selection.selectedIds, contains('draft'));
    expect(controller.expansions.isExpanded('docs'), isTrue);
  });

  test('indexOfNode returns visible index', () {
    final controller = DirectoryTreeController(data: buildTestTreeData());
    expect(controller.indexOfNode('home'), 0);
    expect(controller.indexOfNode('docs'), 1);
    expect(controller.indexOfNode('notes'), -1);

    controller.expand('docs');
    expect(controller.indexOfNode('notes'),
        controller.visibleNodes.indexWhere((n) => n.id == 'notes'));

    controller.filterQuery = 'vac';
    expect(controller.indexOfNode('notes'), -1);
  });

  test('reveal accepts virtualPath lookups', () async {
    final controller = DirectoryTreeController(data: buildTestTreeData());

    await controller.reveal(
      virtualPath: '/home/docs/draft.md',
      select: true,
    );

    expect(controller.selection.selectedIds, contains('draft'));
    expect(controller.expansions.isExpanded('docs'), isTrue);
  });

  testWidgets('revealAndScroll scrolls target into view', (tester) async {
    final view = tester.view;
    view.physicalSize = const Size(240, 120);
    view.devicePixelRatio = 1.0;
    addTearDown(() {
      view.resetPhysicalSize();
      view.resetDevicePixelRatio();
    });

    final controller = DirectoryTreeController(data: buildTestTreeData());
    final scrollController = ScrollController();
    const rowHeight = 24.0;

    await tester.pumpWidget(
      MaterialApp(
        home: Directionality(
          textDirection: TextDirection.ltr,
          child: DirectoryTreeTheme(
            data: const DirectoryTreeThemeData(rowHeight: rowHeight),
            child: SizedBox(
              height: rowHeight * 3,
              child: DirectoryTreeView(
                controller: controller,
                scrollController: scrollController,
                showScrollbar: false,
                preserveScrollOnChanges: false,
                nodeBuilder: (context, node, state) => Text(node.name),
              ),
            ),
          ),
        ),
      ),
    );

    await tester.pump();

    expect(scrollController.hasClients, isTrue);
    expect(scrollController.offset, closeTo(0, 0.001));

    await controller.revealAndScroll(
      nodeId: 'draft',
      select: true,
      scrollController: scrollController,
      rowExtent: rowHeight,
      animate: false,
    );

    await tester.pump();

    final expectedIndex = controller.indexOfNode('draft');
    expect(expectedIndex, greaterThanOrEqualTo(0));
    final maxExtent = scrollController.position.maxScrollExtent;
    final targetOffset = expectedIndex * rowHeight;
    expect(
      scrollController.offset,
      closeTo(targetOffset.clamp(0.0, maxExtent), 0.001),
    );
    expect(controller.selection.selectedIds, contains('draft'));

    await tester.pumpWidget(const SizedBox.shrink());
    controller.dispose();
    scrollController.dispose();
  });
}
