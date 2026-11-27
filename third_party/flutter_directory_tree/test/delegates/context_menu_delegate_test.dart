import 'package:directory_tree/directory_tree.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_directory_tree/src/delegates/context_menu_delegate.dart';
import 'package:flutter_test/flutter_test.dart';

VisibleNode _fileNode(String id) => VisibleNode(
      id: id,
      depth: 0,
      name: '$id.txt',
      type: NodeType.file,
      hasChildren: false,
      virtualPath: '/$id.txt',
    );

void main() {
  testWidgets('MaterialContextMenuDelegate invokes action on selection',
      (tester) async {
    final node = _fileNode('readme');
    bool invoked = false;
    final delegate = MaterialContextMenuDelegate((visibleNode) {
      expect(visibleNode, same(node));
      return [
        NodeAction(
          id: 'open',
          label: 'Open',
          onInvoke: (_) async {
            invoked = true;
          },
        ),
      ];
    });

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Builder(
            builder: (context) => delegate.wrapWithMenu(
              context,
              const Text('target'),
              node,
            ),
          ),
        ),
      ),
    );

    final gesture = await tester.startGesture(
      tester.getCenter(find.text('target')),
      buttons: kSecondaryButton,
    );
    await gesture.up();
    await tester.pumpAndSettle();

    expect(find.text('Open'), findsOneWidget);

    await tester.tap(find.text('Open'));
    await tester.pumpAndSettle();

    expect(invoked, isTrue);
  });
}
