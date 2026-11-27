// lib/src/delegates/node_renderer.dart
import 'package:directory_tree/directory_tree.dart' show VisibleNode;
import 'package:flutter/widgets.dart';

/// Immutable snapshot of a node's visual state during a render cycle.
///
/// Passed to [NodeBuilder] so rows can style themselves (e.g., background color on selection).
class NodeVisualState {
  /// Creates a visual state snapshot.
  const NodeVisualState({
    required this.isExpanded,
    required this.isSelected,
    required this.isFocused,
    required this.isHovered,
    required this.depth,
    this.contentIndent,
  });

  /// True if this node is a folder and is currently open.
  final bool isExpanded;

  /// True if this node is part of the active selection set.
  final bool isSelected;

  /// True if this node is selected AND the tree has input focus.
  ///
  /// Use this to draw a different background or border to indicate active focus.
  final bool isFocused;

  /// True if the mouse pointer is currently hovering over this row.
  final bool isHovered;

  /// The tree depth (0-based) used for indentation.
  final int depth;

  /// Optional override for indentation width.
  final double? contentIndent;
}

/// Signature for widgets that render a single tree row.
///
/// * [context]: The build context.
/// * [node]: The data to display.
/// * [state]: The interaction state (selected, expanded, etc.).
typedef NodeBuilder = Widget Function(
  BuildContext context,
  VisibleNode node,
  NodeVisualState state,
);

/// Signature for building the expand/collapse chevron.
///
/// * [context]: The build context.
/// * [node]: The node being rendered.
/// * [isExpanded]: Whether the node is currently open.
/// * [onPressed]: Callback to toggle the expansion state.
typedef ExpanderBuilder = Widget Function(
  BuildContext context,
  VisibleNode node,
  bool isExpanded,
  VoidCallback onPressed,
);
