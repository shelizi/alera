import 'package:alera/src/features/workbench/application/git_change_selection.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  const ids = <String>[
    'unstaged::a.dart',
    'unstaged::b.dart',
    'staged::b.dart',
    'untracked::c.dart',
  ];

  test('plain click, Ctrl/Cmd toggle and Shift range use visible order', () {
    final selection = GitChangeSelection();
    selection.tap(ids[0], visibleIds: ids, shift: false, toggle: false);
    selection.tap(ids[2], visibleIds: ids, shift: false, toggle: true);
    expect(selection.ids, <String>{ids[0], ids[2]});

    selection.tap(ids[3], visibleIds: ids, shift: true, toggle: false);
    expect(selection.ids, <String>{ids[2], ids[3]});
    selection.tap(ids[3], visibleIds: ids, shift: false, toggle: true);
    expect(selection.ids, <String>{ids[2]});
    selection.tap(ids[1], visibleIds: ids, shift: false, toggle: false);
    expect(selection.ids, <String>{ids[1]});
  });

  test(
    'context click keeps selected group but resets on an unselected row',
    () {
      final selection = GitChangeSelection();
      selection.tap(ids[0], visibleIds: ids, shift: false, toggle: false);
      selection.tap(ids[1], visibleIds: ids, shift: false, toggle: true);
      selection.contextSelect(ids[0]);
      expect(selection.ids, <String>{ids[0], ids[1]});
      selection.contextSelect(ids[3]);
      expect(selection.ids, <String>{ids[3]});
    },
  );

  test('filters and repository changes prune stale selection and anchor', () {
    final selection = GitChangeSelection();
    selection.tap(ids[0], visibleIds: ids, shift: false, toggle: false);
    selection.tap(ids[2], visibleIds: ids, shift: false, toggle: true);
    selection.retainVisible(<String>[ids[1], ids[2]]);
    expect(selection.ids, <String>{ids[2]});
    selection.tap(ids[1], visibleIds: ids, shift: true, toggle: false);
    expect(selection.ids, <String>{ids[1], ids[2]});
    selection.clear();
    expect(selection.isEmpty, isTrue);
  });
}
