import 'package:flutter/material.dart';
import 'package:flutter_directory_tree/src/controller/directory_tree_controller.dart';
import 'package:flutter_directory_tree/src/controller/selection_controller.dart';
import 'package:flutter_directory_tree/src/prebuilt/directory_tree_picker.dart';
import 'package:flutter_test/flutter_test.dart';

import '../helpers/test_tree.dart';

Future<void> _openPicker(WidgetTester tester) async {
  await tester.tap(find.byKey(const ValueKey('open-picker')));
  await tester.pumpAndSettle();
}

Finder get _selectButton => find.widgetWithText(FilledButton, 'Select');

void main() {
  testWidgets('returns selected id on confirm', (tester) async {
    final controller = DirectoryTreeController(data: buildTestTreeData());
    addTearDown(controller.dispose);

    controller.selectOnly('home');

    Set<String>? latest;
    await tester.pumpWidget(
      MaterialApp(
        home: StatefulBuilder(
          builder: (context, setState) => Scaffold(
            body: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                Text(latest == null ? '-' : latest!.join(','),
                    key: const ValueKey('result')),
                ElevatedButton(
                  key: const ValueKey('open-picker'),
                  onPressed: () async {
                    final picked = await showDirectoryTreePicker(
                      context: context,
                      controller: controller,
                    );
                    setState(() => latest = picked);
                  },
                  child: const Text('Open'),
                ),
              ],
            ),
          ),
        ),
      ),
    );

    await _openPicker(tester);

    await tester.tap(_selectButton);
    await tester.pumpAndSettle();

    expect(latest, equals({'home'}));
  });

  testWidgets('disables confirm for files when foldersOnly is true',
      (tester) async {
    final controller = DirectoryTreeController(data: buildTestTreeData());
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      MaterialApp(
        home: Builder(
          builder: (context) => ElevatedButton(
            key: const ValueKey('open-picker'),
            onPressed: () => showDirectoryTreePicker(
              context: context,
              controller: controller,
              foldersOnly: true,
            ),
            child: const Text('Open'),
          ),
        ),
      ),
    );

    controller.selectOnly('readme');
    await _openPicker(tester);
    var selectButton = tester.widget<FilledButton>(_selectButton);
    expect(selectButton.onPressed, isNull);
    await tester.tap(find.text('Cancel'));
    await tester.pumpAndSettle();

    controller.selectOnly('docs');
    await _openPicker(tester);
    selectButton = tester.widget<FilledButton>(_selectButton);
    expect(selectButton.onPressed, isNotNull);
    await tester.tap(find.text('Cancel'));
    await tester.pumpAndSettle();
  });

  testWidgets('supports multi-select mode when allowMultiple is true',
      (tester) async {
    final controller = DirectoryTreeController(
      data: buildTestTreeData(),
      selection: SelectionController(mode: SelectionMode.multi),
    );
    addTearDown(controller.dispose);

    Set<String>? latest;

    await tester.pumpWidget(
      MaterialApp(
        home: StatefulBuilder(
          builder: (context, setState) => Scaffold(
            body: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                Text(latest == null ? '-' : latest!.join(','),
                    key: const ValueKey('result')),
                ElevatedButton(
                  key: const ValueKey('open-picker'),
                  onPressed: () async {
                    final picked = await showDirectoryTreePicker(
                      context: context,
                      controller: controller,
                      allowMultiple: true,
                    );
                    setState(() => latest = picked);
                  },
                  child: const Text('Open'),
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
    await _openPicker(tester);

    await tester.tap(_selectButton);
    await tester.pumpAndSettle();

    expect(latest, equals({'home', 'docs'}));
  });
}
