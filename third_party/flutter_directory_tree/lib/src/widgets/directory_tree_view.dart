// lib/src/widgets/directory_tree_view.dart
import 'package:directory_tree/directory_tree.dart' show VisibleNode;
import 'package:flutter/widgets.dart';
import 'package:flutter_directory_tree/src/controller/directory_tree_controller.dart';
import 'package:flutter_directory_tree/src/controller/tree_diff.dart';
import 'package:flutter_directory_tree/src/delegates/context_menu_delegate.dart';
import 'package:flutter_directory_tree/src/delegates/node_renderer.dart';
import 'package:flutter_directory_tree/src/theme/directory_tree_theme.dart';

/// The primary widget for rendering the directory tree.
///
/// **What:** Displays a flat list of visible nodes, managing scrolling, indentation,
/// and row rendering.
///
/// **How:**
/// * Reacts to changes in [controller] (expansion, selection, filter).
/// * Uses a [ListView.builder] for performance (only renders visible rows).
/// * Supports custom [nodeBuilder] for full control over row appearance.
/// * Maintains scroll anchors when the tree structure changes (via [preserveScrollOnChanges]).
class DirectoryTreeView extends StatefulWidget {
  /// Creates a virtualized tree view.
  const DirectoryTreeView({
    super.key,
    required this.controller,
    required this.nodeBuilder,
    this.expanderBuilder,
    this.contextMenuDelegate,
    this.scrollController,
    this.padding,
    this.addAutomaticKeepAlives = false,
    this.showScrollbar = true,
    this.focusNode,
    this.autofocus = false,
    this.preserveScrollOnChanges = true,
    this.expanderSize = 24,
    this.expanderGap = 8,
  });

  /// The controller providing data and state.
  final DirectoryTreeController controller;

  /// Callback to render the body of each row.
  ///
  /// **Contract:**
  /// The returned widget is wrapped in a [SizedBox] with height equal to
  /// [DirectoryTreeThemeData.rowHeight]. Ensure your content fits within this constraint.
  final NodeBuilder nodeBuilder;

  /// Optional builder for the expand/collapse chevron.
  ///
  /// If null, a default text-based chevron (▸/▾) is used to minimize dependencies.
  final ExpanderBuilder? expanderBuilder;

  /// Optional delegate to handle right-click context menus.
  final ContextMenuDelegate? contextMenuDelegate;

  /// Optional scroll controller.
  ///
  /// If null, a local one is created and managed internally.
  final ScrollController? scrollController;

  /// Padding around the list content.
  final EdgeInsets? padding;

  /// Whether to wrap items in [AutomaticKeepAlive].
  ///
  /// Defaults to false for better performance.
  final bool addAutomaticKeepAlives;

  /// Whether to display a scrollbar.
  final bool showScrollbar;

  /// Optional focus node for the list.
  final FocusNode? focusNode;

  /// Whether the list should focus itself on mount.
  final bool autofocus;

  /// If true, attempts to keep the same node at the top of the viewport when items
  /// are expanded or collapsed above it.
  ///
  /// **Why:** Without this, expanding a folder above your current view pushes the
  /// content down, causing the items you were looking at to "jump" away.
  ///
  /// **How:** The view calculates which node is currently at the top, and after the update,
  /// adjusts the scroll offset to keep that specific node in the same visual position.
  final bool preserveScrollOnChanges;

  /// Fixed square extent used for folder expand/collapse affordances and file placeholders.
  final double expanderSize;

  /// Horizontal gap between the expander column (or placeholder) and the node body.
  final double expanderGap;

  @override
  State<DirectoryTreeView> createState() => _DirectoryTreeViewState();
}

class _DirectoryTreeViewState extends State<DirectoryTreeView> {
  late ScrollController _ownScroll;
  List<VisibleNode> _visibleSnapshot = const <VisibleNode>[];
  Widget _cachedListView = const SizedBox.shrink();
  bool _dirtySnapshot = true;
  bool _hasTreeFocus = false;

  @override
  void initState() {
    super.initState();
    _ownScroll = widget.scrollController ?? ScrollController();
    widget.controller.addListener(_handleControllerChange);
    _hasTreeFocus = widget.focusNode?.hasFocus ?? false;
  }

  @override
  void didUpdateWidget(covariant DirectoryTreeView oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(oldWidget.controller, widget.controller)) {
      oldWidget.controller.removeListener(_handleControllerChange);
      widget.controller.addListener(_handleControllerChange);
      _visibleSnapshot = const <VisibleNode>[];
      _cachedListView = const SizedBox.shrink();
      _dirtySnapshot = true;
    }
    if (!identical(oldWidget.scrollController, widget.scrollController)) {
      if (oldWidget.scrollController == null) {
        _ownScroll.dispose();
      }
      _ownScroll = widget.scrollController ?? ScrollController();
    }
    if (!identical(oldWidget.focusNode, widget.focusNode)) {
      _hasTreeFocus = widget.focusNode?.hasFocus ?? _hasTreeFocus;
      _dirtySnapshot = true;
    }
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _dirtySnapshot = true;
  }

  @override
  void dispose() {
    widget.controller.removeListener(_handleControllerChange);
    if (widget.scrollController == null) {
      _ownScroll.dispose();
    }
    super.dispose();
  }

  void _handleControllerChange() {
    _dirtySnapshot = true;
    if (mounted) {
      setState(() {});
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = DirectoryTreeTheme.of(context);
    final treeHasFocus = widget.focusNode?.hasFocus ?? _hasTreeFocus;

    Widget rowFor(List<VisibleNode> nodes, int index) {
      final node = nodes[index];
      final isExpanded = widget.controller.expansions.isExpanded(node.id);
      final hasExpander = node.hasChildren;
      final isSelected = widget.controller.selection.isSelected(node.id);

      final state = NodeVisualState(
        isExpanded: isExpanded,
        isSelected: isSelected,
        isFocused: treeHasFocus && isSelected,
        isHovered: false,
        depth: node.depth,
        // DirectoryTreeView handles all indentation; downstream builders should not add more.
        contentIndent: 0.0,
      );

      Widget? expander;
      if (hasExpander) {
        void onToggle() => widget.controller.toggle(node.id);
        final chevron = widget.expanderBuilder?.call(
              context,
              node,
              isExpanded,
              onToggle,
            ) ??
            _DefaultChevron(
              expanded: isExpanded,
              onTap: onToggle,
            );
        expander = SizedBox(
          width: widget.expanderSize,
          height: widget.expanderSize,
          child: Center(child: chevron),
        );
      }

      Widget rowChild = widget.nodeBuilder(context, node, state);

      final rowContent = <Widget>[];

      final indent = theme.indent * node.depth;
      if (indent > 0) {
        rowContent.add(SizedBox(width: indent));
      }

      rowContent.add(
        hasExpander
            ? expander!
            : SizedBox(width: widget.expanderSize, height: widget.expanderSize),
      );

      if (widget.expanderGap > 0) {
        rowContent.add(SizedBox(width: widget.expanderGap));
      }

      rowContent.add(Expanded(child: rowChild));

      rowChild = Row(children: rowContent);

      if (widget.contextMenuDelegate != null) {
        rowChild =
            widget.contextMenuDelegate!.wrapWithMenu(context, rowChild, node);
      }

      // Fix row height even when animating and isolate paints to the row.
      return RepaintBoundary(
        child: SizedBox(height: theme.rowHeight, child: rowChild),
      );
    }

    Widget buildPlainList(List<VisibleNode> nodes) {
      Widget list = ListView.builder(
        controller: _ownScroll,
        padding: widget.padding,
        itemExtent: theme.rowHeight,
        itemCount: nodes.length,
        addAutomaticKeepAlives: widget.addAutomaticKeepAlives,
        itemBuilder: (context, index) => KeyedSubtree(
          key: ValueKey(nodes[index].id),
          child: rowFor(nodes, index),
        ),
      );
      if (widget.showScrollbar) {
        list = RawScrollbar(
          controller: _ownScroll,
          thumbVisibility: true,
          child: list,
        );
      }
      return list;
    }

    if (_dirtySnapshot) {
      final oldNodes = _visibleSnapshot;
      final newNodes = List<VisibleNode>.from(widget.controller.visibleNodes);

      if (widget.preserveScrollOnChanges &&
          oldNodes.isNotEmpty &&
          _ownScroll.hasClients) {
        final diff = diffVisibleNodes(oldNodes, newNodes);
        if (!diff.isNoop) {
          final oldOffset = _ownScroll.offset.clamp(0.0, double.infinity);
          final topIndex = (oldOffset / theme.rowHeight)
              .floor()
              .clamp(0, oldNodes.length - 1);
          final anchorId = oldNodes[topIndex].id;
          final newIndex = newNodes.indexWhere((n) => n.id == anchorId);
          if (newIndex != -1) {
            schedulePreserveScrollOffset(
              controller: _ownScroll,
              before: oldNodes,
              after: newNodes,
              rowExtent: theme.rowHeight,
            );
          }
        }
      }

      _visibleSnapshot = newNodes;
      _cachedListView = buildPlainList(_visibleSnapshot);
      _dirtySnapshot = false;
    }

    final focusable = Focus(
      focusNode: widget.focusNode,
      autofocus: widget.autofocus,
      onFocusChange: (value) {
        if (!mounted || _hasTreeFocus == value) return;
        setState(() {
          _hasTreeFocus = value;
          _dirtySnapshot = true;
        });
      },
      child: _cachedListView,
    );

    return Semantics(
      container: true,
      explicitChildNodes: true,
      child: focusable,
    );
  }
}

/// Minimal default expander; stays text-based to keep deps light.
class _DefaultChevron extends StatelessWidget {
  const _DefaultChevron({required this.expanded, required this.onTap});

  final bool expanded;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final dur = DirectoryTreeTheme.of(context).animationDuration;
    final isRtl = Directionality.of(context) == TextDirection.rtl;
    final expandedTurns = isRtl ? -0.25 : 0.25;
    final glyph = isRtl ? '◂' : '▸';
    return Semantics(
      button: true,
      label: expanded ? 'Collapse' : 'Expand',
      onTap: onTap,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsetsDirectional.only(start: 4, end: 4),
          child: AnimatedRotation(
            turns: expanded ? expandedTurns : 0.0, // ▶/◀ to ▾
            duration: dur,
            child: Text(glyph, textAlign: TextAlign.center),
          ),
        ),
      ),
    );
  }
}
