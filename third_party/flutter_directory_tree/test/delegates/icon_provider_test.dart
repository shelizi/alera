import 'package:directory_tree/directory_tree.dart';
import 'package:flutter/material.dart';
import 'package:flutter_directory_tree/src/delegates/icon_provider.dart';
import 'package:flutter_test/flutter_test.dart';

VisibleNode _node({
  required String id,
  required NodeType type,
  required String name,
  bool isVirtual = false,
}) =>
    VisibleNode(
      id: id,
      depth: 0,
      name: name,
      type: type,
      hasChildren: false,
      virtualPath: '/$name',
      isVirtual: isVirtual,
    );

void main() {
  const provider = MaterialIconProvider();

  Future<Icon> iconFor(WidgetTester tester, VisibleNode node) async {
    Icon? captured;
    await tester.pumpWidget(
      MaterialApp(
        home: Builder(
          builder: (context) {
            captured = provider.leadingIcon(context, node) as Icon?;
            return const SizedBox();
          },
        ),
      ),
    );
    return captured!;
  }

  testWidgets('returns folder icon for folders', (tester) async {
    final icon = await iconFor(
      tester,
      _node(id: 'folder', type: NodeType.folder, name: 'folder'),
    );
    expect(icon.icon, Icons.folder);
  });

  testWidgets('returns file-type specific icons', (tester) async {
    final dartIcon = await iconFor(
      tester,
      _node(id: 'dart', type: NodeType.file, name: 'main.dart'),
    );
    expect(dartIcon.icon, Icons.code);

    final imageIcon = await iconFor(
      tester,
      _node(id: 'img', type: NodeType.file, name: 'photo.png'),
    );
    expect(imageIcon.icon, Icons.image);
  });

  testWidgets('returns cloud icon for virtual files', (tester) async {
    final icon = await iconFor(
      tester,
      _node(
        id: 'virtual',
        type: NodeType.file,
        name: 'virtual.custom',
        isVirtual: true,
      ),
    );
    expect(icon.icon, Icons.cloud_queue);
  });

  testWidgets('falls back to generic file icon', (tester) async {
    final icon = await iconFor(
      tester,
      _node(id: 'plain', type: NodeType.file, name: 'plain.bin'),
    );
    expect(icon.icon, Icons.insert_drive_file);
  });
}
