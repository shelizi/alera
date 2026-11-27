// lib/src/prebuilt/directory_tree_picker.dart
import 'package:directory_tree/directory_tree.dart' as core;
import 'package:flutter/material.dart';
import 'package:flutter_directory_tree/src/controller/directory_tree_controller.dart';
import 'package:flutter_directory_tree/src/delegates/icon_provider.dart';
import 'package:flutter_directory_tree/src/delegates/node_renderer.dart';
import 'package:flutter_directory_tree/src/widgets/directory_tree_view.dart';
import 'package:flutter_directory_tree/src/widgets/selection_shortcuts.dart';
import 'package:flutter_directory_tree/src/widgets/tree_node_tile.dart';

/// Displays a modal dialog for selecting files or folders.
///
/// Returns a [Set] of selected node IDs.
/// * [allowMultiple]: If true, users can select multiple items (CMD/Shift click).
/// * [foldersOnly]: If true, files are unselectable, and the return value contains only folder IDs.
///   Useful for "Pick Destination" workflows.
Future<Set<String>> showDirectoryTreePicker({
  required BuildContext context,
  required DirectoryTreeController controller,
  bool allowMultiple = false,
  bool foldersOnly = false,
  String confirmLabel = 'Select',
  String title = 'Select',
}) async {
  final result = await showDialog<Set<String>>(
    context: context,
    barrierDismissible: true,
    builder: (ctx) => _PickerDialog(
      controller: controller,
      allowMultiple: allowMultiple,
      foldersOnly: foldersOnly,
      confirmLabel: confirmLabel,
      title: title,
    ),
  );
  return result ?? <String>{};
}

class _PickerDialog extends StatefulWidget {
  const _PickerDialog({
    required this.controller,
    required this.allowMultiple,
    required this.foldersOnly,
    required this.confirmLabel,
    required this.title,
  });

  final DirectoryTreeController controller;
  final bool allowMultiple;
  final bool foldersOnly;
  final String confirmLabel;
  final String title;

  @override
  State<_PickerDialog> createState() => _PickerDialogState();
}

class _PickerDialogState extends State<_PickerDialog> {
  final _icons = const MaterialIconProvider();

  bool get _hasValidSelection {
    final sel = widget.controller.selection.selectedIds;
    if (sel.isEmpty) return false;
    if (!widget.foldersOnly) return true;
    final nodes = widget.controller.data.nodes;
    return sel.every((id) => nodes[id]?.type == core.NodeType.folder);
  }

  void _confirm() {
    final selected = widget.controller.selection.selectedIds;
    if (selected.isEmpty) return;
    if (widget.foldersOnly) {
      final nodes = widget.controller.data.nodes;
      final folders = selected
          .where((id) => nodes[id]?.type == core.NodeType.folder)
          .toSet();
      if (folders.isEmpty) return;
      Navigator.of(context).pop(folders);
    } else {
      Navigator.of(context).pop(selected);
    }
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: Text(widget.title),
      content: SizedBox(
        width: 520,
        height: 420,
        child: SelectionShortcuts(
          controller: widget.controller,
          child: DirectoryTreeView(
            controller: widget.controller,
            nodeBuilder: _buildTile,
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).maybePop(),
          child: const Text('Cancel'),
        ),
        FilledButton(
          onPressed: _hasValidSelection ? _confirm : null,
          child: Text(widget.confirmLabel),
        ),
      ],
    );
  }

  Widget _buildTile(
    BuildContext context,
    core.VisibleNode node,
    NodeVisualState st,
  ) {
    final leading = _icons.leadingIcon(context, node);

    void handleTap() {
      if (widget.foldersOnly && node.type != core.NodeType.folder) {
        // Forbid selecting files in folders-only mode; just toggle folder expand/collapse.
        if (node.hasChildren) {
          widget.controller.toggle(node.id);
        }
        return;
      }
      if (widget.allowMultiple) {
        widget.controller.toggleSelection(node.id);
      } else {
        widget.controller.selectOnly(node.id);
      }
    }

    return TreeNodeTile(
      node: node,
      state: st,
      leading: Padding(
        padding: const EdgeInsetsDirectional.only(start: 4, end: 8),
        child: leading,
      ),
      title: Text(
        node.name,
        overflow: TextOverflow.ellipsis,
      ),
      onTap: handleTap,
      onDoubleTap: () {
        if (node.hasChildren) {
          widget.controller.toggle(node.id);
        } else {
          // Choose immediately when double-tapping a file in multi=false mode
          if (!widget.allowMultiple && (!widget.foldersOnly)) {
            widget.controller.selectOnly(node.id);
            _confirm();
          }
        }
      },
    );
  }
}
