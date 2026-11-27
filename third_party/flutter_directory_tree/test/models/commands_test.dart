import 'package:flutter/widgets.dart';
import 'package:flutter_directory_tree/src/controller/directory_tree_controller.dart';
import 'package:flutter_directory_tree/src/models/commands.dart';
import 'package:flutter_test/flutter_test.dart';

import '../helpers/test_tree.dart';

void main() {
  test('DirectoryTreeAction routes intents to controller methods', () async {
    final controller = DirectoryTreeController(data: buildTestTreeData());
    final action = DirectoryTreeAction<Intent>(controller)
      ..invoke(const ExpandNodeIntent('docs'));
    expect(controller.expansions.isExpanded('docs'), isTrue);

    action.invoke(const CollapseNodeIntent('docs'));
    expect(controller.expansions.isExpanded('docs'), isFalse);

    action.invoke(const ToggleNodeIntent('pictures'));
    expect(controller.expansions.isExpanded('pictures'), isTrue);

    action.invoke(const SelectOnlyIntent('readme'));
    expect(controller.selection.isSelected('readme'), isTrue);

    action.invoke(const ClearSelectionIntent());
    expect(controller.selection.selectedIds, isEmpty);

    action.invoke(const RevealNodeIntent(virtualPath: '/home/docs/draft.md'));
    // Allow the async reveal to complete.
    await Future<void>.delayed(Duration.zero);

    expect(controller.expansions.isExpanded('docs'), isTrue);
  });
}
