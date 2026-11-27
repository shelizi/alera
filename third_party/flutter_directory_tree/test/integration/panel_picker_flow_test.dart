import 'package:flutter/material.dart';
import 'package:flutter_directory_tree/src/controller/directory_tree_controller.dart';
import 'package:flutter_directory_tree/src/controller/selection_controller.dart';
import 'package:flutter_directory_tree/src/prebuilt/directory_tree_picker.dart';
import 'package:flutter_directory_tree/src/widgets/directory_tree_panel.dart';
import 'package:flutter_test/flutter_test.dart';

import '../helpers/test_tree.dart';

void main() {
  testWidgets('panel integrates with picker for multi-select flow',
      (tester) async {
    final controller = DirectoryTreeController(
      data: buildTestTreeData(),
      selection: SelectionController(mode: SelectionMode.multi),
    );
    addTearDown(controller.dispose);

    Set<String>? picked;

    await tester.pumpWidget(
      MaterialApp(
        home: Builder(
          builder: (context) => Scaffold(
            body: Row(
              children: [
                Expanded(
                  child: DirectoryTreePanel(
                    controller: controller,
                    nodeBuilder: (ctx, node, state) =>
                        ListTile(title: Text(node.name)),
                  ),
                ),
                ElevatedButton(
                  key: const ValueKey('open-picker'),
                  onPressed: () async {
                    picked = await showDirectoryTreePicker(
                      context: context,
                      controller: controller,
                      allowMultiple: true,
                    );
                  },
                  child: const Text('Pick'),
                ),
              ],
            ),
          ),
        ),
      ),
    );

    controller.selection
      ..clear()
      ..toggle('home')
      ..toggle('docs');

    await tester.tap(find.byKey(const ValueKey('open-picker')));
    await tester.pumpAndSettle();

    await tester.tap(
      find
          .descendant(
            of: find.byType(AlertDialog),
            matching: find.widgetWithText(FilledButton, 'Select'),
          )
          .first,
    );
    await tester.pumpAndSettle();

    expect(picked, equals({'home', 'docs'}));
  });
}
