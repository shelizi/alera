# flutter_directory_tree

[![pub package](https://img.shields.io/pub/v/flutter_directory_tree.svg)](https://pub.dev/packages/flutter_directory_tree)

A production-ready, virtualized file explorer widget for Flutter applications. Designed for performance with huge file trees, robust keyboard navigation, and desktop-class features.

![Demo](screenshots/1.gif)

## 🚀 Key Features

*   **Virtualization**: Renders flattened lists using `ListView.builder`, ensuring high performance even with thousands of nested nodes.
*   **State Preservation**: Smartly maintains expansion and selection states even when the underlying directory structure is refreshed (e.g., file system changes).
*   **Desktop-Class Interactions**:
    *   Keyboard navigation (Arrows, Home/End, PageUp/PageDown).
    *   Multi-select (Shift+Click, Cmd/Ctrl+Click).
    *   Context Menu support.
*   **Customizable**:
    *   **Theming**: Full control over colors, indentation, and row heights via `DirectoryTreeTheme`.
    *   **Builders**: Custom `nodeBuilder` to render rows exactly how you want.
    *   **Icons**: logic separation via `IconProvider`.

## 📦 Installation

```bash
flutter pub add flutter_directory_tree
```

## 💻 Usage

The simplest way to get started is using the `DirectoryTreePanel`, which bundles the toolbar, shortcuts, and tree view.

```dart
import 'package:flutter/material.dart';
import 'package:flutter_directory_tree/flutter_directory_tree.dart';

class MyFileExplorer extends StatefulWidget {
  @override
  _MyFileExplorerState createState() => _MyFileExplorerState();
}

class _MyFileExplorerState extends State<MyFileExplorer> {
  late DirectoryTreeController _controller;

  @override
  void initState() {
    super.initState();
    // 1. Initialize your tree data (see directory_tree package for parsing)
    final data = TreeData(root: ...);

    // 2. Create the controller
    _controller = DirectoryTreeController(data: data);
  }

  @override
  Widget build(BuildContext context) {
    // 3. Render the panel
    return DirectoryTreePanel(
      controller: _controller,
      nodeBuilder: (context, node, state) {
        // Use the built-in tile or return your own widget
        return TreeNodeTile(
          node: node,
          state: state,
          leading: const Icon(Icons.folder),
          title: Text(node.name),
        );
      },
    );
  }
}
```

## 🎨 Customization

### Theming
Wrap your tree in a `DirectoryTreeTheme` or let it inherit defaults based on your `ThemeData` brightness.

```dart
DirectoryTreeTheme(
  data: DirectoryTreeThemeData(
    rowHeight: 32,
    selectionColor: Colors.blue.withOpacity(0.2),
    folderIconColor: Colors.amber,
  ),
  child: DirectoryTreeView(...),
);
```

### Drag and Drop
To add drag-and-drop support, wrap the content in your `nodeBuilder` with standard Flutter `Draggable` and `DragTarget` widgets. The tree view provides the structure, you provide the interaction logic.

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.