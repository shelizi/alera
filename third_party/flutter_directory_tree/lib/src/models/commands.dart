// lib/src/models/commands.dart
import 'dart:async';
import 'package:flutter/widgets.dart';
import 'package:flutter_directory_tree/src/controller/directory_tree_controller.dart';

/// Intent to expand a specific node.
///
/// * [recursive]: If true, expands the node and all its folder descendants.
class ExpandNodeIntent extends Intent {
  /// Creates an intent to expand [nodeId].
  const ExpandNodeIntent(this.nodeId, {this.recursive = false});

  /// The ID of the node to expand.
  final String nodeId;

  /// Whether to also expand all nested folders.
  final bool recursive;
}

/// Intent to collapse a specific node.
class CollapseNodeIntent extends Intent {
  /// Creates an intent to collapse [nodeId].
  const CollapseNodeIntent(this.nodeId, {this.recursive = false});

  /// The ID of the node to collapse.
  final String nodeId;

  /// Whether to also collapse all nested folders.
  final bool recursive;
}

/// Intent to toggle the expansion state of a node.
class ToggleNodeIntent extends Intent {
  /// Creates an intent to toggle [nodeId].
  const ToggleNodeIntent(this.nodeId);

  /// The ID of the node to toggle.
  final String nodeId;
}

/// Intent to reveal a hidden node (by expanding parents) and optionally select it.
class RevealNodeIntent extends Intent {
  /// Creates an intent to reveal a node by ID or path.
  const RevealNodeIntent({this.nodeId, this.virtualPath, this.select = false});

  /// The direct ID of the node to reveal.
  final String? nodeId;

  /// The virtual path (e.g., 'src/main.dart') of the node to reveal.
  final String? virtualPath;

  /// Whether to select the node after revealing it.
  final bool select;
}

/// Intent to select a single node, clearing other selections.
class SelectOnlyIntent extends Intent {
  /// Creates an intent to select [nodeId].
  const SelectOnlyIntent(this.nodeId);

  /// The ID of the node to select.
  final String nodeId;
}

/// Intent to clear all selection.
class ClearSelectionIntent extends Intent {
  /// Creates an intent to clear selection.
  const ClearSelectionIntent();
}

/// Connects [Intent]s from the Flutter Actions system to the [DirectoryTreeController].
///
/// Used by [SelectionShortcuts] to map keyboard events to controller methods.
class DirectoryTreeAction<T extends Intent> extends Action<T> {
  /// Creates an action that forwards intents to [controller].
  DirectoryTreeAction(this.controller);

  /// The controller to act upon.
  final DirectoryTreeController controller;

  @override
  Object? invoke(T intent) {
    switch (intent) {
      case ExpandNodeIntent(:final nodeId, :final recursive):
        controller.expand(nodeId, recursive: recursive);
        return null;
      case CollapseNodeIntent(:final nodeId, :final recursive):
        controller.collapse(nodeId, recursive: recursive);
        return null;
      case ToggleNodeIntent(:final nodeId):
        controller.toggle(nodeId);
        return null;
      case RevealNodeIntent(:final nodeId, :final virtualPath, :final select):
        unawaited(controller.reveal(
            nodeId: nodeId, virtualPath: virtualPath, select: select));
        return null;
      case SelectOnlyIntent(:final nodeId):
        controller.selectOnly(nodeId);
        return null;
      case ClearSelectionIntent():
        controller.clearSelection();
        return null;
      default:
        return null;
    }
  }
}
