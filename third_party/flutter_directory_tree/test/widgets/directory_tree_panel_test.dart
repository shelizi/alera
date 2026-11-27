import 'package:flutter/material.dart';
import 'package:flutter_directory_tree/src/controller/directory_tree_controller.dart';
import 'package:flutter_directory_tree/src/widgets/directory_tree_panel.dart';
import 'package:flutter_directory_tree/src/widgets/directory_tree_toolbar.dart';
import 'package:flutter_directory_tree/src/widgets/selection_shortcuts.dart';
import 'package:flutter_test/flutter_test.dart';

import '../helpers/test_tree.dart';

void main() {
  group('DirectoryTreeToolbar', () {
    testWidgets('updates controller filter and visible count', (tester) async {
      final controller = DirectoryTreeController(data: buildTestTreeData());
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: DirectoryTreeToolbar(controller: controller),
          ),
        ),
      );

      expect(find.text('4'), findsOneWidget); // default visible count

      await tester.enterText(find.byType(TextField), 'doc');
      await tester.pump(const Duration(milliseconds: 200));

      expect(controller.filterQuery, 'doc');
      expect(find.text('2'), findsOneWidget);

      await tester.tap(find.byIcon(Icons.clear));
      await tester.pump(const Duration(milliseconds: 200));

      expect(controller.filterQuery, isEmpty);
      expect(find.text('4'), findsOneWidget);
    });

    testWidgets('expand and collapse all buttons update expansions',
        (tester) async {
      final tree = buildTestTreeData();
      final controller = DirectoryTreeController(data: tree);
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: DirectoryTreeToolbar(controller: controller),
          ),
        ),
      );

      await tester.tap(find.byIcon(Icons.unfold_more));
      await tester.pump();
      expect(controller.expansions.expandedIds,
          containsAll({'home', 'docs', 'pictures'}));

      await tester.tap(find.byIcon(Icons.unfold_less));
      await tester.pump();
      expect(controller.expansions.expandedIds, equals({'home'}));
    });
  });

  group('DirectoryTreePanel', () {
    testWidgets('renders default toolbar and selection shortcuts',
        (tester) async {
      final controller = DirectoryTreeController(data: buildTestTreeData());
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: DirectoryTreePanel(
              controller: controller,
              nodeBuilder: (context, node, state) => Text(node.name),
            ),
          ),
        ),
      );

      expect(find.byType(DirectoryTreeToolbar), findsOneWidget);
      expect(find.byType(SelectionShortcuts), findsOneWidget);
    });

    testWidgets('respects custom toolbar', (tester) async {
      final controller = DirectoryTreeController(data: buildTestTreeData());
      addTearDown(controller.dispose);

      const customToolbar = SizedBox(height: 10, child: Text('custom'));

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: DirectoryTreePanel(
              controller: controller,
              nodeBuilder: (context, node, state) => Text(node.name),
              toolbar: customToolbar,
            ),
          ),
        ),
      );

      expect(find.text('custom'), findsOneWidget);
      expect(find.byType(DirectoryTreeToolbar), findsNothing);
    });
  });
}
