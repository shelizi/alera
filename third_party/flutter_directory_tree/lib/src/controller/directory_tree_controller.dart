// lib/src/controller/directory_tree_controller.dart
import 'dart:collection';
import 'package:directory_tree/directory_tree.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_directory_tree/src/controller/expansion_controller.dart';
import 'package:flutter_directory_tree/src/controller/indexed_visible_rows.dart';
import 'package:flutter_directory_tree/src/controller/selection_controller.dart';

/// The central coordinator for the directory tree's state and logic.
///
/// [DirectoryTreeController] manages the intersection of:
/// * **Data:** The underlying [TreeData] structure.
/// * **Expansion:** Which folders are open or closed (via [expansions]).
/// * **Selection:** Which nodes are currently active (via [selection]).
/// * **Visibility:** Which nodes are actually rendered after flattening and filtering.
///
/// ### Usage
/// * **Initialization:** Create this once and pass it to [DirectoryTreeView].
/// * **Updates:** Call [rebuild] when the underlying file system changes to swap the tree
///   while preserving user state (selection/expansion) where possible.
/// * **Interactions:** Use methods like [expand], [toggle], or [reveal] to programmatically
///   drive the view.
class DirectoryTreeController extends ChangeNotifier {
  /// Creates a controller with the given initial [data].
  ///
  /// If [expansions] or [selection] are not provided, this controller creates and manages
  /// its own default instances.
  ///
  /// * [seedSelectionFromCore]: If true, initializes selection from [TreeData.nodes] where `isSelected` is true.
  /// * [autoExpandVisibleRoot]: If true, ensures the root node is expanded so children are visible immediately.
  /// * [lazyLookupIndexes]: For immutable snapshots, defer path/entry-ID lookup
  ///   indexes until their first use. Node-ID navigation does not require them.
  DirectoryTreeController({
    required TreeData data,
    ExpansionController? expansions,
    SelectionController? selection,
    FlattenStrategy? flattenStrategy,
    bool seedSelectionFromCore = true,
    this.autoExpandVisibleRoot = true,
    this.lazyLookupIndexes = false,
    this.immutableVisibleRows = false,
  })  : _data = data,
        _ownsExpansions = expansions == null,
        _ownsSelection = selection == null,
        expansions = expansions ??
            ExpansionController(
              initiallyExpanded: _initialExpandedIds(
                data,
                includeVisibleRoot: autoExpandVisibleRoot,
              ),
            ),
        selection =
            selection ?? SelectionController(mode: SelectionMode.single),
        _flattenStrategy = flattenStrategy ?? const DefaultFlattenStrategy() {
    if (!_ownsExpansions) {
      final external = this.expansions;
      if (external.expandedIds.isEmpty) {
        final seeds = _initialExpandedIds(
          _data,
          includeVisibleRoot: autoExpandVisibleRoot,
        );
        if (seeds.isNotEmpty) {
          external.expandAll(seeds);
        }
      }
    }

    if (_ownsSelection && seedSelectionFromCore) {
      final seeds = _initialSelectedIds(_data);
      if (seeds.isNotEmpty) {
        this.selection.addAll(seeds);
      }
    }

    // Recompute when child controllers change.
    this.expansions.addListener(_bubble);
    this.selection.addListener(_relaySelection);
    _recomputeVisible();
    if (!lazyLookupIndexes) _rebuildVirtualIndex();
  }

  /// The immutable snapshot of the tree structure currently held by the controller.
  ///
  /// This data determines the hierarchy. To change the structure (e.g. file added),
  /// use [rebuild] which handles state preservation.
  TreeData get data => _data;
  TreeData _data;

  /// Whether the root node is automatically expanded during initialization or rebuilds.
  ///
  /// * If `true`, the root's children are shown immediately.
  /// * If `false`, the user sees a collapsed root and must expand it manually.
  final bool autoExpandVisibleRoot;

  /// Opt-in for consumers that publish immutable snapshots and normally
  /// navigate by node ID. Lookup indexes are still cached per snapshot.
  final bool lazyLookupIndexes;

  /// Opt-in for a flatten strategy that publishes immutable row lists with
  /// globally unique IDs. Reuse an identical list/index and let the view
  /// retain that snapshot and preserve its scroll anchor directly.
  final bool immutableVisibleRows;

  /// The sub-controller managing which folder nodes are currently open.
  ///
  /// Exposes methods like [expandAll], [collapseAll] and allows listening to expansion state changes.
  final ExpansionController expansions;

  /// The sub-controller managing which nodes are currently selected.
  ///
  /// Supports [SelectionMode.single] or [SelectionMode.multiple].
  final SelectionController selection;
  final bool _ownsExpansions;
  final bool _ownsSelection;

  final FlattenStrategy _flattenStrategy;

  String _filterQuery = '';

  /// The active search string used to prune the tree view.
  ///
  /// Setting this value triggers a recalculation of [visibleNodes].
  ///
  /// * **Empty (`''`):** The tree renders its standard hierarchical structure.
  /// * **Non-Empty:** The tree enters "search mode," showing only nodes that match
  ///   the query (and their ancestors) in a flattened list.
  ///
  /// This change notifies listeners immediately.
  String get filterQuery => _filterQuery;

  set filterQuery(String value) {
    final next = value.trim();
    if (next == _filterQuery) return;
    _filterQuery = next;
    _recomputeVisible();
    notifyListeners();
  }

  /// The flat list of nodes currently displayed in the tree view.
  ///
  /// This list is derived from [data] by flattening the hierarchy based on
  /// current [expansions] and applying any [filterQuery].
  ///
  /// This is the data source for the view's [ListView].
  List<VisibleNode> get visibleNodes => _visibleNodesView;
  late List<VisibleNode> _visibleNodes;
  late List<VisibleNode> _visibleNodesView;
  Map<String, int>? _indexById;
  IndexedVisibleRows? _indexedVisibleRows;
  List<VisibleNode>? _lastImmutableRows;
  Map<String, String>? _idByVirtualPath;
  Map<String, String>? _idByEntryId;
  bool _suppressBubble = false;
  void _relaySelection() {
    if (_suppressBubble) return;
    notifyListeners();
  }

  /// Replaces the underlying tree data with [next].
  ///
  /// Use this when the file system structure changes (e.g., file added/removed) to
  /// swap the tree while maintaining the user's context.
  ///
  /// ### State Preservation
  /// * If [tryPreserveState] is true (default), this controller attempts to carry over
  ///   the list of expanded folders and selected items to the new tree. IDs that no
  ///   longer exist in [next] are silently dropped.
  /// * If [reseedFromCore] is true, the UI state is reset to match the `isExpanded`
  ///   and `isSelected` flags defined in the [next] data model itself.
  void rebuild(
    TreeData next, {
    bool tryPreserveState = true,
    bool reseedFromCore = false,
  }) {
    _data = next;
    // Invalidate before sub-controller listeners can look up the new data.
    // The two indexes are independent; an entry-ID query does not allocate
    // a virtual-path index, and vice versa.
    _idByVirtualPath = null;
    _idByEntryId = null;

    // Alera publishes immutable maps, so membership can use the map itself.
    // Keep the default upstream mode's existing membership snapshot.
    final bool Function(String) exists = lazyLookupIndexes
        ? next.nodes.containsKey
        : next.nodes.keys.toSet().contains;

    _suppressBubble = true;
    try {
      if (tryPreserveState) {
        expansions.retainWhere(exists);
        selection.retainWhere(exists);
      } else {
        expansions.collapseAll();
        selection.clear();
      }

      if (reseedFromCore) {
        final seeds = _initialExpandedIds(
          next,
          includeVisibleRoot: autoExpandVisibleRoot,
        );
        expansions
          ..collapseAll()
          ..expandAll(seeds);
        final selectionSeeds = _initialSelectedIds(next);
        selection.performBatch(() {
          selection.clear();
          if (selectionSeeds.isNotEmpty) {
            selection.addAll(selectionSeeds);
          }
        });
      } else if (autoExpandVisibleRoot &&
          !expansions.isExpanded(next.visibleRootId)) {
        expansions.setExpanded(next.visibleRootId, true);
      }
    } finally {
      _suppressBubble = false;
    }

    _recomputeVisible();
    if (!lazyLookupIndexes) _rebuildVirtualIndex();
    notifyListeners();
  }

  /// Expands all ancestor folders required to make the target node visible.
  ///
  /// This modifies the [expansions] state. It does **not** scroll the view to the node;
  /// use [revealAndScroll] for a complete "find in tree" experience.
  ///
  /// Can resolve the target by either [nodeId] or [virtualPath].
  /// * [select]: If true, also replaces the current selection with the target node.
  ///
  /// Does nothing if the target cannot be resolved.
  Future<void> reveal({
    String? nodeId,
    String? virtualPath,
    bool select = false,
  }) async {
    final id = nodeId ?? _findByVirtualPath(virtualPath ?? '');
    if (id == null) return;

    // Walk parents and expand.
    String? current = id;
    while (current != null && current.isNotEmpty) {
      final node = _data.nodes[current];
      if (node == null) break;
      if (node.type == NodeType.folder || node.type == NodeType.root) {
        expansions.setExpanded(node.id, true);
      }
      if (node.id == _data.visibleRootId) break;
      current = node.parentId;
    }

    if (select) {
      selection.selectOnly(id);
    }
  }

  /// Reveals a node and smooth-scrolls the [scrollController] to bring it into view.
  ///
  /// This helps deeply nested items become immediately visible to the user.
  ///
  /// * [rowExtent]: The height of a single row, required to calculate scroll offsets.
  /// * [animate]: If true, uses [duration] and [curve] to scroll. Otherwise jumps.
  ///
  /// **Constraints:**
  /// * Does nothing if [scrollController] has no attached clients (e.g. view not built yet).
  /// * Does nothing if the node cannot be resolved.
  Future<void> revealAndScroll({
    String? nodeId,
    String? virtualPath,
    bool select = false,
    required ScrollController scrollController,
    required double rowExtent,
    bool animate = true,
    Duration duration = const Duration(milliseconds: 220),
    Curve curve = Curves.easeOutCubic,
  }) async {
    assert(nodeId != null || (virtualPath != null && virtualPath.isNotEmpty),
        'Either nodeId or virtualPath must be provided.');

    final resolvedId = nodeId ?? _findByVirtualPath(virtualPath ?? '');
    if (resolvedId == null) {
      return;
    }

    await reveal(nodeId: resolvedId, select: select);

    if (!scrollController.hasClients) {
      return;
    }

    // Let any synchronous listeners run before computing offsets.
    await Future<void>.microtask(() {});

    final index = indexOfNode(resolvedId);
    if (index < 0) {
      return;
    }

    final target = index * rowExtent;
    final position = scrollController.position;
    final viewportExtent = position.viewportDimension;
    final contentExtent = _visibleNodes.length * rowExtent;
    final maxExtent =
        (contentExtent - viewportExtent).clamp(0.0, double.infinity);
    final clamped = target.clamp(position.minScrollExtent, maxExtent);

    if (animate) {
      await scrollController.animateTo(
        clamped,
        duration: duration,
        curve: curve,
      );
    } else {
      scrollController.jumpTo(clamped);
    }
  }

  /// Opens the folder at [nodeId].
  ///
  /// * [recursive]: If true, also expands all nested folders inside this one.
  ///
  /// **Constraints:**
  /// * Ignored if [nodeId] does not exist or is a file.
  void expand(String nodeId, {bool recursive = false}) {
    if (!_data.nodes.containsKey(nodeId) || !_isFolderLike(nodeId)) return;
    if (recursive) {
      expansions.performBatch(() {
        _expandDescendants(nodeId);
      });
    } else {
      expansions.setExpanded(nodeId, true);
    }
  }

  /// Closes the folder at [nodeId].
  ///
  /// * [recursive]: If true, also collapses all nested folders inside this one.
  ///
  /// **Constraints:**
  /// * Ignored if [nodeId] does not exist or is a file.
  void collapse(String nodeId, {bool recursive = false}) {
    if (!_data.nodes.containsKey(nodeId) || !_isFolderLike(nodeId)) return;
    if (recursive) {
      expansions.performBatch(() {
        _collapseDescendants(nodeId);
      });
    } else {
      expansions.setExpanded(nodeId, false);
    }
  }

  /// Inverts the expansion state of the folder at [nodeId].
  ///
  /// **Constraints:**
  /// * Ignored if [nodeId] does not exist or is a file.
  void toggle(String nodeId) {
    if (!_data.nodes.containsKey(nodeId) || !_isFolderLike(nodeId)) return;
    expansions.toggle(nodeId);
  }

  /// Clears the current selection and selects only the specified [nodeId].
  ///
  /// Use this for standard "click to select" behavior.
  void selectOnly(String nodeId) {
    selection.selectOnly(nodeId);
  }

  /// Adds [nodeId] to the selection if missing, or removes it if present.
  ///
  /// Commonly bound to "Ctrl+Click" or "Cmd+Click".
  void toggleSelection(String nodeId) {
    selection.toggle(nodeId);
  }

  /// Selects all visible nodes between [anchorId] and [toId] (inclusive).
  ///
  /// **Behavior:**
  /// * Determines the visual order of nodes.
  /// * Adds the entire range to the current selection.
  /// * Typically used for "Shift+Click" interactions.
  void selectRange({required String anchorId, required String toId}) {
    final a = indexOfNode(anchorId);
    final b = indexOfNode(toId);
    if (a < 0 || b < 0) return;
    final start = a < b ? a : b;
    final end = a < b ? b : a;
    final ids = <String>[
      for (final node in _visibleNodes.getRange(start, end + 1)) node.id,
    ];
    selection.selectRange(ids, anchorId, toId);
  }

  /// Deselects all currently selected nodes.
  void clearSelection() {
    selection.clear();
  }

  /// Add all file descendants of [nodeId] to the current selection.
  void selectSubtree(String nodeId) {
    final ids = _descendantFileIds(nodeId);
    if (ids.isEmpty) return;
    selection.performBatch(() {
      selection.addAll(ids);
    });
  }

  /// Remove all file descendants of [nodeId] from the current selection.
  void deselectSubtree(String nodeId) {
    final ids = _descendantFileIds(nodeId);
    if (ids.isEmpty) return;
    selection.performBatch(() {
      selection.removeAll(ids);
    });
  }

  /// Lookup a visible node id using a core entryId (file identifier).
  String? nodeIdForEntryId(String entryId) =>
      (_idByEntryId ??= _buildEntryIdIndex())[entryId];

  /// Reveal and optionally select a node by its underlying entry id.
  Future<void> revealByEntryId(String entryId, {bool select = false}) async {
    final id = nodeIdForEntryId(entryId);
    if (id == null) return;
    await reveal(nodeId: id, select: select);
  }

  /// Return the visible index of a node, or -1 if not visible.
  int indexOfNode(String nodeId) =>
      _indexedVisibleRows?.indexOfVisibleNode(nodeId, _data) ??
      _indexById?[nodeId] ??
      -1;

  // ---- internals ------------------------------------------------------------

  bool _isFolderLike(String nodeId) {
    final type = _data.nodes[nodeId]?.type;
    return type == NodeType.folder || type == NodeType.root;
  }

  /// Re-calculates the flat list of nodes to be rendered.
  ///
  /// **Why:** The UI renders a flat [ListView]. We must flatten the tree structure
  /// based on the current [expansions] state and any active [filterQuery].
  ///
  /// **Side Effects:**
  /// * Updates [_visibleNodes] and [_indexById].
  /// * This is O(N) relative to the visible nodes, so we cache the result until invalidation.
  void _recomputeVisible() {
    final next = _flattenStrategy.flatten(
      data: _data,
      expandedIds: expansions.expandedIds,
      filterQuery: _filterQuery.isEmpty ? null : _filterQuery,
    );
    if (immutableVisibleRows) {
      if (identical(next, _lastImmutableRows)) return;
      _lastImmutableRows = next;
    }
    _visibleNodes = next;
    if (immutableVisibleRows && next is IndexedVisibleRows) {
      _indexedVisibleRows = next;
      _visibleNodesView = next;
      _indexById = null;
    } else {
      _indexedVisibleRows = null;
      _visibleNodesView = UnmodifiableListView(_visibleNodes);
      _indexById = {
        for (var i = 0; i < _visibleNodes.length; i++) _visibleNodes[i].id: i,
      };
    }
  }

  /// Re-indexes nodes for O(1) lookup by path or entry ID.
  ///
  /// **Why:** Methods like [reveal] need to find a node ID given a path string (e.g. from a deep link).
  /// Scanning the list every time would be too slow.
  void _rebuildVirtualIndex() {
    _idByVirtualPath = _buildVirtualPathIndex();
    _idByEntryId = _buildEntryIdIndex();
  }

  // Preserve original last-entry-wins semantics for duplicate paths and IDs.
  Map<String, String> _buildVirtualPathIndex() => {
        for (final entry in _data.nodes.entries)
          if (entry.value.virtualPath.isNotEmpty)
            entry.value.virtualPath: entry.key,
      };

  Map<String, String> _buildEntryIdIndex() => {
        for (final entry in _data.nodes.entries)
          if (entry.value.entryId != null) entry.value.entryId!: entry.key,
      };

  void _bubble() {
    if (_suppressBubble) return;
    _recomputeVisible();
    notifyListeners();
  }

  String? _findByVirtualPath(String virtualPath) {
    if (virtualPath.isEmpty) return null;
    return (_idByVirtualPath ??= _buildVirtualPathIndex())[virtualPath];
  }

  /// Recursively expands a node and all its folder descendants.
  ///
  /// **Algorithm:** Uses an iterative stack approach to avoid stack overflow on deep trees.
  void _expandDescendants(String nodeId) {
    final stack = <String>[nodeId];
    while (stack.isNotEmpty) {
      final id = stack.removeLast();
      final node = _data.nodes[id];
      if (node == null) continue;
      if (_isFolderLike(id)) {
        expansions.setExpanded(id, true);
      }
      if (node.childIds.isNotEmpty) {
        stack.addAll(node.childIds);
      }
    }
  }

  /// Recursively collapses a node and all its folder descendants.
  ///
  /// **Algorithm:** Uses an iterative stack approach to avoid stack overflow.
  void _collapseDescendants(String nodeId) {
    final stack = <String>[nodeId];
    while (stack.isNotEmpty) {
      final id = stack.removeLast();
      final node = _data.nodes[id];
      if (node == null) continue;
      if (_isFolderLike(id)) {
        expansions.setExpanded(id, false);
      }
      stack.addAll(node.childIds);
    }
  }

  /// Finds all file IDs nested under [nodeId].
  ///
  /// **Why:** Used by [selectSubtree] to bulk-select files.
  List<String> _descendantFileIds(String nodeId) {
    final root = _data.nodes[nodeId];
    if (root == null) return const <String>[];
    final fileIds = <String>[];
    final stack = <String>[nodeId];
    while (stack.isNotEmpty) {
      final id = stack.removeLast();
      final node = _data.nodes[id];
      if (node == null) continue;
      if (node.type == NodeType.file) {
        fileIds.add(id);
      } else if (node.childIds.isNotEmpty) {
        stack.addAll(node.childIds);
      }
    }
    return fileIds;
  }

  @override
  void dispose() {
    expansions.removeListener(_bubble);
    selection.removeListener(_relaySelection);
    if (_ownsExpansions) {
      expansions.dispose();
    }
    if (_ownsSelection) {
      selection.dispose();
    }
    super.dispose();
  }

  static Set<String> _initialExpandedIds(
    TreeData data, {
    bool includeVisibleRoot = true,
  }) {
    final seeds = <String>{};
    if (includeVisibleRoot) {
      seeds.add(data.visibleRootId);
    }
    for (final node in data.nodes.values) {
      if (node.isExpanded &&
          (includeVisibleRoot || node.id != data.visibleRootId)) {
        seeds.add(node.id);
      }
    }
    return seeds;
  }

  static Set<String> _initialSelectedIds(TreeData data) {
    final seeds = <String>{};
    for (final node in data.nodes.values) {
      if (node.isSelected) {
        seeds.add(node.id);
      }
    }
    return seeds;
  }
}
