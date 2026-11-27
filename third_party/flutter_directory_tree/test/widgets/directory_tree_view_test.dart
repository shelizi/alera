import 'package:directory_tree/directory_tree.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_directory_tree/src/controller/directory_tree_controller.dart';
import 'package:flutter_directory_tree/src/controller/selection_controller.dart';
import 'package:flutter_directory_tree/src/delegates/node_renderer.dart';
import 'package:flutter_directory_tree/src/theme/directory_tree_theme.dart';
import 'package:flutter_directory_tree/src/widgets/directory_tree_view.dart';
import 'package:flutter_directory_tree/src/widgets/selection_shortcuts.dart';
import 'package:flutter_test/flutter_test.dart';

import '../helpers/test_tree.dart';

TreeData _buildLargeTree({int childCount = 40}) {
  final nodes = <String, TreeNode>{
    'root': TreeNode(
      id: 'root',
      name: 'Root',
      type: NodeType.root,
      parentId: '',
      virtualPath: '/',
      childIds: const ['home'],
      isExpanded: true,
    ),
  };

  final children = <String>[];
  for (var i = 0; i < childCount; i++) {
    final id = 'file_$i';
    children.add(id);
    nodes[id] = TreeNode(
      id: id,
      name: 'file_$i.txt',
      type: NodeType.file,
      parentId: 'home',
      virtualPath: '/home/file_$i.txt',
      entryId: 'entry_$i',
    );
  }

  nodes['home'] = TreeNode(
    id: 'home',
    name: 'home',
    type: NodeType.folder,
    parentId: 'root',
    virtualPath: '/home',
    childIds: List<String>.from(children),
    isExpanded: true,
  );

  return TreeData(nodes: nodes, rootId: 'root', visibleRootId: 'home');
}

TreeData _buildAlternateTree() {
  final nodes = <String, TreeNode>{
    'root': TreeNode(
      id: 'root',
      name: 'Root',
      type: NodeType.root,
      parentId: '',
      virtualPath: '/',
      childIds: const ['workspace'],
      isExpanded: true,
    ),
    'workspace': TreeNode(
      id: 'workspace',
      name: 'workspace',
      type: NodeType.folder,
      parentId: 'root',
      virtualPath: '/workspace',
      childIds: const ['src'],
      isExpanded: true,
    ),
    'src': TreeNode(
      id: 'src',
      name: 'src',
      type: NodeType.folder,
      parentId: 'workspace',
      virtualPath: '/workspace/src',
      childIds: const ['main'],
      isExpanded: true,
    ),
    'main': TreeNode(
      id: 'main',
      name: 'main.dart',
      type: NodeType.file,
      parentId: 'src',
      virtualPath: '/workspace/src/main.dart',
      entryId: 'entry-main',
    ),
  };

  return TreeData(
    nodes: nodes,
    rootId: 'root',
    visibleRootId: 'workspace',
  );
}

Widget _buildNode(
    BuildContext context, VisibleNode node, NodeVisualState state) {
  return Text(node.name, key: ValueKey(node.id));
}

void main() {
  group('DirectoryTreeView', () {
    testWidgets('renders nodes and toggles expansion via default expander',
        (tester) async {
      final controller = DirectoryTreeController(data: buildTestTreeData());
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        MaterialApp(
          home: DirectoryTreeView(
            controller: controller,
            nodeBuilder: _buildNode,
            showScrollbar: false,
          ),
        ),
      );
      await tester.pump();

      expect(find.text('home'), findsOneWidget);
      expect(find.text('docs'), findsOneWidget);
      expect(find.text('notes.txt'), findsNothing);

      await tester.tap(find.text('▸').at(1));
      await tester.pumpAndSettle();

      expect(controller.expansions.isExpanded('docs'), isTrue);
      expect(find.text('notes.txt'), findsOneWidget);
      expect(find.text('draft.md'), findsOneWidget);
    });

    testWidgets('preserves scroll anchor when data is rebuilt', (tester) async {
      final controller = DirectoryTreeController(data: _buildLargeTree());
      addTearDown(controller.dispose);

      final scrollController = ScrollController();
      addTearDown(scrollController.dispose);

      await tester.pumpWidget(
        MaterialApp(
          home: SizedBox(
            height: 240,
            child: DirectoryTreeView(
              controller: controller,
              nodeBuilder: _buildNode,
              scrollController: scrollController,
              preserveScrollOnChanges: true,
            ),
          ),
        ),
      );

      await tester.pump();
      expect(scrollController.hasClients, isTrue);

      scrollController.jumpTo(140);
      await tester.pump();
      final initialOffset = scrollController.offset;

      final theme = DirectoryTreeTheme.of(
        tester.element(find.byType(DirectoryTreeView)),
      );
      final rowHeight = theme.rowHeight;
      final current = controller.data;
      final previousNodes = List<VisibleNode>.from(controller.visibleNodes);
      final anchorIndex = (initialOffset / rowHeight).floor();
      final anchorId = previousNodes[anchorIndex].id;
      final nextNodes = Map<String, TreeNode>.from(current.nodes);
      const newNodeId = 'file_new';
      nextNodes[newNodeId] = TreeNode(
        id: newNodeId,
        name: 'file_new.txt',
        type: NodeType.file,
        parentId: 'home',
        virtualPath: '/home/file_new.txt',
        entryId: 'entry_new',
      );
      final home = nextNodes['home']!;
      nextNodes['home'] = home.copyWith(
        childIds: [newNodeId, ...home.childIds],
      );

      controller.rebuild(
        TreeData(
          nodes: nextNodes,
          rootId: current.rootId,
          visibleRootId: current.visibleRootId,
        ),
      );

      await tester.pumpAndSettle();

      expect(controller.visibleNodes.any((n) => n.id == newNodeId), isTrue);
      final newIndex = (scrollController.offset / rowHeight).floor();
      final anchoredId = controller.visibleNodes[newIndex].id;
      expect(anchoredId, anchorId);
    });

    testWidgets('rebuilds cleanly when controller instance changes',
        (tester) async {
      final controllerA = DirectoryTreeController(data: buildTestTreeData());
      final controllerB = DirectoryTreeController(data: _buildAlternateTree());
      addTearDown(controllerA.dispose);
      addTearDown(controllerB.dispose);

      await tester.pumpWidget(
        MaterialApp(
          home: DirectoryTreeView(
            controller: controllerA,
            nodeBuilder: _buildNode,
            showScrollbar: false,
          ),
        ),
      );
      await tester.pump();

      expect(find.text('home'), findsOneWidget);
      expect(find.text('workspace'), findsNothing);

      await tester.pumpWidget(
        MaterialApp(
          home: DirectoryTreeView(
            controller: controllerB,
            nodeBuilder: _buildNode,
            showScrollbar: false,
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(tester.takeException(), isNull);
      expect(find.text('workspace'), findsOneWidget);
      expect(find.text('main.dart'), findsOneWidget);
      expect(find.text('docs'), findsNothing);
    });
  });

  group('SelectionShortcuts', () {
    testWidgets('supports keyboard navigation and selection toggles',
        (tester) async {
      final selection = SelectionController(mode: SelectionMode.multi);
      final controller = DirectoryTreeController(
        data: buildTestTreeData(),
        selection: selection,
      );
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        MaterialApp(
          home: SelectionShortcuts(
            controller: controller,
            autofocus: true,
            child: DirectoryTreeView(
              controller: controller,
              nodeBuilder: _buildNode,
              showScrollbar: false,
            ),
          ),
        ),
      );

      await tester.pump();

      await simulateKeyDownEvent(LogicalKeyboardKey.arrowDown);
      await simulateKeyUpEvent(LogicalKeyboardKey.arrowDown);
      await tester.pump();
      expect(controller.selection.selectedIds, equals({'docs'}));

      await simulateKeyDownEvent(LogicalKeyboardKey.arrowRight);
      await simulateKeyUpEvent(LogicalKeyboardKey.arrowRight);
      await tester.pump();
      expect(controller.expansions.isExpanded('docs'), isTrue);

      await simulateKeyDownEvent(LogicalKeyboardKey.arrowDown);
      await simulateKeyUpEvent(LogicalKeyboardKey.arrowDown);
      await tester.pump();
      expect(controller.selection.selectedIds, equals({'notes'}));

      await simulateKeyDownEvent(LogicalKeyboardKey.space);
      await simulateKeyUpEvent(LogicalKeyboardKey.space);
      await tester.pump();
      expect(controller.selection.selectedIds, isEmpty);

      await simulateKeyDownEvent(LogicalKeyboardKey.arrowDown);
      await simulateKeyUpEvent(LogicalKeyboardKey.arrowDown);
      await tester.pump();
      expect(controller.selection.selectedIds, equals({'docs'}));

      await simulateKeyDownEvent(LogicalKeyboardKey.arrowLeft);
      await simulateKeyUpEvent(LogicalKeyboardKey.arrowLeft);
      await tester.pump();
      expect(controller.expansions.isExpanded('docs'), isFalse);
    });
  });
}
