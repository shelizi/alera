// lib/src/widgets/tree_node_tile.dart
import 'package:directory_tree/directory_tree.dart' show VisibleNode;
import 'package:flutter/widgets.dart';
import 'package:flutter_directory_tree/src/delegates/node_renderer.dart';
import 'package:flutter_directory_tree/src/theme/directory_tree_theme.dart';

/// A standard row widget that respects the active [DirectoryTreeTheme].
///
/// **Behavior:**
/// * Automatically draws background colors for selection, focus, and hover.
/// * Handles indentation based on [node.depth] and theme settings.
/// * Provides callbacks for tap, double-tap, and secondary tap.
///
/// Use this as your default [NodeBuilder] unless you need a completely custom look.
class TreeNodeTile extends StatefulWidget {
  /// Creates a standard tile.
  const TreeNodeTile({
    super.key,
    required this.node,
    required this.state,
    this.leading,
    this.trailing,
    this.title,
    this.subtitle,
    this.onTap,
    this.onDoubleTap,
    this.onTertiaryTap,
  });

  /// The data for this row.
  final VisibleNode node;

  /// The visual state (selected, hovered, etc.) passed from the tree view.
  final NodeVisualState state;

  /// Widget displayed before the title (e.g. file icon).
  ///
  /// If null, no leading space or icon is rendered.
  final Widget? leading;

  /// Widget displayed at the end of the row (e.g. status icon).
  final Widget? trailing;

  /// The main text of the row.
  ///
  /// Defaults to [node.name] inside a [Text] widget if not provided.
  final Widget? title;

  /// Optional secondary text.
  final Widget? subtitle;

  /// Callback fired when the user performs a primary click (tap) on the tile.
  ///
  /// **Interaction:**
  /// * Typically triggers selection logic.
  /// * If [onDoubleTap] is also provided, this callback is fired on **pointer down**
  ///   (instead of tap up) to ensure immediate selection feedback, bypassing the
  ///   slight delay of the gesture arena.
  final VoidCallback? onTap;

  /// Callback for double click.
  final VoidCallback? onDoubleTap;

  /// Callback for secondary/tertiary click (e.g. middle mouse button).
  final VoidCallback? onTertiaryTap;

  @override
  State<TreeNodeTile> createState() => _TreeNodeTileState();
}

class _TreeNodeTileState extends State<TreeNodeTile> {
  bool _hovering = false;

  @override
  Widget build(BuildContext context) {
    final theme = DirectoryTreeTheme.of(context);
    final indentWidth =
        widget.state.contentIndent ?? widget.node.depth * theme.indent;

    Color? background;
    if (widget.state.isSelected && theme.selectionColor != null) {
      background = theme.selectionColor;
    } else if (widget.state.isFocused && theme.focusColor != null) {
      background = theme.focusColor;
    } else if (_hovering && theme.hoverColor != null) {
      background = theme.hoverColor;
    }

    final row = Row(
      crossAxisAlignment: CrossAxisAlignment.center,
      children: [
        SizedBox(width: indentWidth),
        if (widget.leading != null) widget.leading!,
        Expanded(
          child: widget.title ??
              Text(
                widget.node.name,
                overflow: TextOverflow.ellipsis,
              ),
        ),
        if (widget.subtitle != null) widget.subtitle!,
        if (widget.trailing != null) widget.trailing!,
      ],
    );

    final decorated = DecoratedBox(
      decoration: BoxDecoration(
        color: background,
        borderRadius: theme.roundedCorners ? BorderRadius.circular(4) : null,
      ),
      child: row,
    );

    final handleTap = widget.onTap;
    final hasDoubleTap = widget.onDoubleTap != null;

    final gesture = GestureDetector(
      behavior: HitTestBehavior.opaque,
      // When a double-tap handler is wired, eagerly run the primary tap logic
      // on pointer down so selection updates without waiting for the double-tap
      // gesture timeout.
      onTapDown: hasDoubleTap && handleTap != null ? (_) => handleTap() : null,
      onTap: hasDoubleTap ? null : handleTap,
      onDoubleTap: widget.onDoubleTap,
      onTertiaryTapUp:
          widget.onTertiaryTap == null ? null : (_) => widget.onTertiaryTap!(),
      child: decorated,
    );

    final hoverable = MouseRegion(
      onEnter: (_) => setState(() => _hovering = true),
      onExit: (_) => setState(() => _hovering = false),
      child: gesture,
    );

    final hasChildren = widget.node.hasChildren;

    return Semantics(
      container: true,
      focusable: true,
      label: widget.node.name,
      selected: widget.state.isSelected,
      expanded: hasChildren ? widget.state.isExpanded : null,
      enabled: widget.onTap != null || widget.onDoubleTap != null,
      button: widget.onTap != null,
      onTap: widget.onTap,
      child: hoverable,
    );
  }
}
