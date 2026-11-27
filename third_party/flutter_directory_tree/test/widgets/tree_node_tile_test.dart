import 'package:directory_tree/directory_tree.dart';
import 'package:flutter/material.dart';
import 'package:flutter_directory_tree/src/delegates/node_renderer.dart';
import 'package:flutter_directory_tree/src/theme/directory_tree_theme.dart';
import 'package:flutter_directory_tree/src/widgets/tree_node_tile.dart';
import 'package:flutter_test/flutter_test.dart';

VisibleNode _node({int depth = 0, String name = 'file.txt'}) => VisibleNode(
      id: name,
      depth: depth,
      name: name,
      type: NodeType.file,
      hasChildren: false,
      virtualPath: '/$name',
    );

void main() {
  testWidgets('TreeNodeTile applies selection color and indent',
      (tester) async {
    const selectionColor = Color(0xFF123456);
    const themeData = DirectoryTreeThemeData(
      selectionColor: selectionColor,
      indent: 20,
    );

    await tester.pumpWidget(
      MaterialApp(
        home: DirectoryTreeTheme(
          data: themeData,
          child: TreeNodeTile(
            node: _node(depth: 2),
            state: const NodeVisualState(
              isExpanded: false,
              isSelected: true,
              isFocused: false,
              isHovered: false,
              depth: 2,
            ),
          ),
        ),
      ),
    );

    final decorated = tester.widget<DecoratedBox>(find.byType(DecoratedBox));
    final boxDecoration = decorated.decoration as BoxDecoration;
    expect(boxDecoration.color, selectionColor);

    final indentSizedBox = tester
        .widgetList<SizedBox>(find.byType(SizedBox))
        .firstWhere((box) => box.width == 40);
    expect(indentSizedBox.width, 40);
  });
}
