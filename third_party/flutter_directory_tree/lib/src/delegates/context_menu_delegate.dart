// lib/src/delegates/context_menu_delegate.dart
import 'package:directory_tree/directory_tree.dart' show VisibleNode;
import 'package:flutter/material.dart';

/// Defines a single actionable item in a context menu.
///
/// * [id]: Unique identifier for logic checks.
/// * [label]: Human-readable text.
/// * [onInvoke]: Callback executed when the user selects this action.
class NodeAction {
  /// Creates a menu action.
  const NodeAction({
    required this.id,
    required this.label,
    this.icon,
    this.onInvoke,
  });

  /// Unique ID for this action (e.g. 'delete', 'rename').
  final String id;

  /// The text displayed in the menu item.
  final String label;

  /// Optional leading icon for the menu item.
  final Widget? icon;

  /// Callback to execute when the action is chosen.
  ///
  /// Receives the [VisibleNode] that was clicked.
  final Future<void> Function(VisibleNode node)? onInvoke;
}

/// Abstract strategy for handling secondary clicks (right-click) on tree nodes.
///
/// **Why:** Decouples the tree's logic from specific UI implementations (Material, Cupertino, MacosUI).
///
/// Implement this to show custom menus or use [MaterialContextMenuDelegate] for a standard popup.
abstract class ContextMenuDelegate {
  /// Abstract constant constructor.
  const ContextMenuDelegate();

  /// Returns the list of actions available for the specific [node].
  List<NodeAction> actionsFor(VisibleNode node);

  /// Wraps the row [child] with a gesture detector that triggers the menu.
  Widget wrapWithMenu(BuildContext context, Widget child, VisibleNode node);
}

/// A standard implementation that displays a Material [showMenu] on right-click or long-press.
class MaterialContextMenuDelegate extends ContextMenuDelegate {
  /// Creates a delegate using a callback to supply actions.
  const MaterialContextMenuDelegate(this._provider);
  final List<NodeAction> Function(VisibleNode node) _provider;

  @override
  List<NodeAction> actionsFor(VisibleNode node) => _provider(node);

  @override
  Widget wrapWithMenu(BuildContext context, Widget child, VisibleNode node) {
    Future<void> showMenuAt(Offset position) async {
      final items = actionsFor(node);
      if (items.isEmpty) return;

      final selected = await showMenu<String>(
        context: context,
        position: RelativeRect.fromLTRB(
          position.dx,
          position.dy,
          position.dx,
          position.dy,
        ),
        items: [
          for (final a in items)
            PopupMenuItem<String>(
              value: a.id,
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  if (a.icon != null) ...[a.icon!, const SizedBox(width: 8)],
                  Text(a.label),
                ],
              ),
            ),
        ],
      );

      if (selected == null) return;
      NodeAction? action;
      for (final candidate in items) {
        if (candidate.id == selected) {
          action = candidate;
          break;
        }
      }
      if (action?.onInvoke == null) return;
      await action!.onInvoke!(node);
    }

    return GestureDetector(
      behavior: HitTestBehavior.translucent,
      onSecondaryTapDown: (details) => showMenuAt(details.globalPosition),
      onLongPressStart: (details) => showMenuAt(details.globalPosition),
      child: child,
    );
  }
}
