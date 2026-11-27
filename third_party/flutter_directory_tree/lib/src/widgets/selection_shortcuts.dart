// lib/src/widgets/selection_shortcuts.dart
import 'package:directory_tree/directory_tree.dart'
    show SelectionMode, VisibleNode;
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_directory_tree/src/controller/directory_tree_controller.dart';

/// Adds keyboard navigation and selection support to the tree.
///
/// **What:** Wraps the tree view to handle:
/// * **Arrow Up/Down:** Move cursor / single select.
/// * **Shift + Arrow Up/Down:** Range selection (extend anchor).
/// * **Arrow Left/Right:** Collapse/Expand folder.
/// * **Space:** Toggle selection.
/// * **Enter:** Activate node (or toggle folder).
/// * **Ctrl/Cmd + A:** Select all (if multi-select allowed).
/// * **Home/End/PageUp/PageDown:** Quick navigation.
///
/// **Usage:** Wrap your [DirectoryTreeView] with this widget.
class SelectionShortcuts extends StatefulWidget {
  /// Creates the shortcut manager.
  const SelectionShortcuts({
    super.key,
    required this.controller,
    required this.child,
    this.onActivate,
    this.autofocus = true,
    this.pageJumpSize = 10,
  });

  /// The controller used to change selection/expansion in response to keys.
  final DirectoryTreeController controller;

  /// The child widget (typically [DirectoryTreeView]) that receives the focus.
  final Widget child;

  /// Called when the user presses Enter on a non-folder node.
  final ValueChanged<VisibleNode>? onActivate;

  /// Whether this widget should request focus on mount.
  final bool autofocus;

  /// Number of items to skip for PageUp/PageDown.
  final int pageJumpSize;

  @override
  State<SelectionShortcuts> createState() => _SelectionShortcutsState();
}

class _SelectionShortcutsState extends State<SelectionShortcuts> {
  int? _cursor; // visible row index
  String? _anchorId; // for shift-extended selection

  DirectoryTreeController get c => widget.controller;

  @override
  void initState() {
    super.initState();
    c.addListener(_syncWithController);
    _syncWithController();
  }

  @override
  void dispose() {
    c.removeListener(_syncWithController);
    super.dispose();
  }

  void _syncWithController() {
    final selected = c.selection.selectedIds;
    final nodes = c.visibleNodes;
    if (nodes.isEmpty) {
      _cursor = null;
      _anchorId = null;
      return;
    }
    // Cursor prefers last-selected item; otherwise 0.
    final currentId = selected.isNotEmpty ? selected.last : nodes.first.id;
    final idx = c.indexOfNode(currentId);
    _cursor = idx == -1 ? 0 : idx;
    _anchorId ??= nodes[_cursor!].id;
    setState(() {});
  }

  /// Moves the selection cursor by [delta] rows.
  ///
  /// * [extend]: If true (Shift key), expands the selection range from the current [_anchorId]
  ///   to the new target. Otherwise, moves the anchor and selects only the target.
  void _move(int delta, {bool extend = false}) {
    final nodes = c.visibleNodes;
    if (nodes.isEmpty) return;
    final cur = (_cursor ?? 0).clamp(0, nodes.length - 1);
    final next = (cur + delta).clamp(0, nodes.length - 1);
    _cursor = next;

    final id = nodes[next].id;
    if (extend) {
      _anchorId ??= nodes[cur].id;
      c.selectRange(anchorId: _anchorId!, toId: id);
    } else {
      _anchorId = id;
      c.selectOnly(id);
    }
  }

  /// Moves the selection by a "page" (set by [widget.pageJumpSize]).
  void _page(int delta, {bool extend = false}) {
    final nodes = c.visibleNodes;
    if (nodes.isEmpty) return;
    final length = nodes.length;
    final clampedSize = widget.pageJumpSize.clamp(1, length);
    final current = (_cursor ?? 0).clamp(0, length - 1);
    final target = (current + (clampedSize * delta)).clamp(0, length - 1);
    _cursor = target;

    final id = nodes[target].id;
    if (extend) {
      _anchorId ??= nodes[current].id;
      c.selectRange(anchorId: _anchorId!, toId: id);
    } else {
      _anchorId = id;
      c.selectOnly(id);
    }
  }

  /// Moves the selection to the very top or bottom of the list.
  void _moveToEdge({required bool start, bool extend = false}) {
    final nodes = c.visibleNodes;
    if (nodes.isEmpty) return;
    final target = start ? 0 : nodes.length - 1;
    final current = (_cursor ?? 0).clamp(0, nodes.length - 1);
    _cursor = target;

    final id = nodes[target].id;
    if (extend) {
      _anchorId ??= nodes[current].id;
      c.selectRange(anchorId: _anchorId!, toId: id);
    } else {
      _anchorId = id;
      c.selectOnly(id);
    }
  }

  void _collapse() {
    final nodes = c.visibleNodes;
    if (nodes.isEmpty || _cursor == null) return;
    final n = nodes[_cursor!];
    final isExpanded = c.expansions.isExpanded(n.id);
    if (isExpanded) {
      c.collapse(n.id);
      return;
    }
    // move to parent
    final parent = c.data.nodes[n.id]?.parentId;
    if (parent == null || parent.isEmpty) return;
    final parentIndex = c.indexOfNode(parent);
    if (parentIndex != -1) {
      _cursor = parentIndex;
      _anchorId = nodes[parentIndex].id;
      c.selectOnly(_anchorId!);
    }
  }

  void _expand() {
    final nodes = c.visibleNodes;
    if (nodes.isEmpty || _cursor == null) return;
    final n = nodes[_cursor!];
    final hasKids = n.hasChildren;
    final isExpanded = c.expansions.isExpanded(n.id);
    if (hasKids && !isExpanded) {
      c.expand(n.id);
      return;
    }
    if (hasKids && isExpanded) {
      // move to first child
      final nextIndex = _cursor! + 1;
      if (nextIndex < nodes.length && nodes[nextIndex].depth == n.depth + 1) {
        _cursor = nextIndex;
        _anchorId = nodes[nextIndex].id;
        c.selectOnly(_anchorId!);
      }
    }
  }

  void _toggleSelection() {
    final nodes = c.visibleNodes;
    if (nodes.isEmpty || _cursor == null) return;
    final id = nodes[_cursor!].id;
    c.toggleSelection(id);
  }

  void _activate() {
    final nodes = c.visibleNodes;
    if (nodes.isEmpty || _cursor == null) return;
    final current = nodes[_cursor!];
    // Default: toggle folders, notify for files
    if (current.hasChildren) {
      c.toggle(current.id);
    } else {
      if (widget.onActivate != null) {
        widget.onActivate!(current);
      } else {
        c.toggleSelection(current.id);
      }
    }
  }

  void _selectAll() {
    if (c.selection.mode == SelectionMode.single) return;
    final nodes = c.visibleNodes;
    if (nodes.isEmpty) return;
    final first = nodes.first.id;
    final last = nodes.last.id;
    c.selectRange(anchorId: first, toId: last);
    _anchorId = first;
    _cursor = 0;
  }

  Map<ShortcutActivator, Intent> get _shortcutMap =>
      <ShortcutActivator, Intent>{
        // Move cursor
        const SingleActivator(LogicalKeyboardKey.arrowDown):
            const _MoveIntent(1),
        const SingleActivator(LogicalKeyboardKey.arrowUp):
            const _MoveIntent(-1),
        // Extend selection with Shift
        const SingleActivator(LogicalKeyboardKey.arrowDown, shift: true):
            const _ExtendIntent(1),
        const SingleActivator(LogicalKeyboardKey.arrowUp, shift: true):
            const _ExtendIntent(-1),

        // Collapse/expand
        const SingleActivator(LogicalKeyboardKey.arrowLeft):
            const _CollapseIntent(),
        const SingleActivator(LogicalKeyboardKey.arrowRight):
            const _ExpandIntent(),

        // Page/Home navigation
        const SingleActivator(LogicalKeyboardKey.pageDown):
            const _PageIntent(1, extend: false),
        const SingleActivator(LogicalKeyboardKey.pageDown, shift: true):
            const _PageIntent(1, extend: true),
        const SingleActivator(LogicalKeyboardKey.pageUp):
            const _PageIntent(-1, extend: false),
        const SingleActivator(LogicalKeyboardKey.pageUp, shift: true):
            const _PageIntent(-1, extend: true),
        const SingleActivator(LogicalKeyboardKey.home):
            const _HomeIntent(extend: false),
        const SingleActivator(LogicalKeyboardKey.home, shift: true):
            const _HomeIntent(extend: true),
        const SingleActivator(LogicalKeyboardKey.end):
            const _EndIntent(extend: false),
        const SingleActivator(LogicalKeyboardKey.end, shift: true):
            const _EndIntent(extend: true),

        // Toggle / Activate
        const SingleActivator(LogicalKeyboardKey.space): const _ToggleIntent(),
        const SingleActivator(LogicalKeyboardKey.enter):
            const _ActivateIntent(),
        const SingleActivator(LogicalKeyboardKey.numpadEnter):
            const _ActivateIntent(),

        // Select all
        const SingleActivator(LogicalKeyboardKey.keyA, control: true):
            const _SelectAllIntent(),
        const SingleActivator(LogicalKeyboardKey.keyA, meta: true):
            const _SelectAllIntent(),
      };

  @override
  Widget build(BuildContext context) {
    return Shortcuts(
      shortcuts: _shortcutMap,
      child: Actions(
        actions: <Type, Action<Intent>>{
          _MoveIntent: CallbackAction<_MoveIntent>(
            onInvoke: (i) => _move(i.delta),
          ),
          _ExtendIntent: CallbackAction<_ExtendIntent>(
            onInvoke: (i) => _move(i.delta, extend: true),
          ),
          _CollapseIntent: CallbackAction<_CollapseIntent>(
            onInvoke: (i) => _collapse(),
          ),
          _ExpandIntent: CallbackAction<_ExpandIntent>(
            onInvoke: (i) => _expand(),
          ),
          _PageIntent: CallbackAction<_PageIntent>(
            onInvoke: (i) => _page(i.delta, extend: i.extend),
          ),
          _HomeIntent: CallbackAction<_HomeIntent>(
            onInvoke: (i) => _moveToEdge(start: true, extend: i.extend),
          ),
          _EndIntent: CallbackAction<_EndIntent>(
            onInvoke: (i) => _moveToEdge(start: false, extend: i.extend),
          ),
          _ToggleIntent: CallbackAction<_ToggleIntent>(
            onInvoke: (i) => _toggleSelection(),
          ),
          _ActivateIntent: CallbackAction<_ActivateIntent>(
            onInvoke: (i) => _activate(),
          ),
          _SelectAllIntent: CallbackAction<_SelectAllIntent>(
            onInvoke: (i) => _selectAll(),
          ),
        },
        child: Focus(
          autofocus: widget.autofocus,
          child: widget.child,
        ),
      ),
    );
  }
}

// Intents
class _MoveIntent extends Intent {
  const _MoveIntent(this.delta);
  final int delta;
}

class _ExtendIntent extends Intent {
  const _ExtendIntent(this.delta);
  final int delta;
}

class _CollapseIntent extends Intent {
  const _CollapseIntent();
}

class _ExpandIntent extends Intent {
  const _ExpandIntent();
}

class _PageIntent extends Intent {
  const _PageIntent(this.delta, {required this.extend});
  final int delta;
  final bool extend;
}

class _HomeIntent extends Intent {
  const _HomeIntent({required this.extend});
  final bool extend;
}

class _EndIntent extends Intent {
  const _EndIntent({required this.extend});
  final bool extend;
}

class _ToggleIntent extends Intent {
  const _ToggleIntent();
}

class _ActivateIntent extends Intent {
  const _ActivateIntent();
}

class _SelectAllIntent extends Intent {
  const _SelectAllIntent();
}
