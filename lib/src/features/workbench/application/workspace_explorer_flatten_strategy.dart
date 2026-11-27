import 'dart:collection';

import 'package:flutter_directory_tree/flutter_directory_tree.dart' as tree;

/// Incrementally flattens Alera's rooted, immutable native Explorer graph.
///
/// Cache one row list per root child, not every ancestor's descendants.
/// Known structural edits invalidate the affected old and new root branches.
/// Expansion changes invalidate their branches too. Unknown edits, filtering,
/// non-rooted graphs and very wide roots retain the reference traversal.
final class WorkspaceExplorerFlattenStrategy extends tree.FlattenStrategy {
  static const placeholderPrefix = '__alera_placeholder__:';
  static const _delegate = tree.DefaultFlattenStrategy();
  static const _minNodes = 256;
  static const _maxRootChildren = 2048;

  tree.TreeData? _pendingData;
  Set<String>? _pendingChanges;
  tree.TreeData? _previousData;
  Set<String> _previousExpanded = const {};
  Map<String, List<tree.VisibleNode>> _branches = const {};
  List<String> _previousOrder = const [];
  List<tree.VisibleNode>? _previousRows;

  /// A complete set of changed or removed graph IDs, relative to the last
  /// published data. Null explicitly means that full invalidation is needed.
  void prepareUpdate(tree.TreeData next, Set<String>? changedIds) {
    _pendingData = next;
    _pendingChanges = changedIds;
  }

  @override
  List<tree.VisibleNode> flatten({
    required tree.TreeData data,
    required Set<String> expandedIds,
    String? filterQuery,
  }) {
    final previous = _previousData;
    final changed = identical(data, previous)
        ? const <String>{}
        : identical(data, _pendingData)
        ? _pendingChanges
        : null;
    _pendingData = null;
    _pendingChanges = null;

    List<tree.VisibleNode> fallback() {
      _previousData = null;
      _previousExpanded = const {};
      _branches = const {};
      _previousOrder = const [];
      _previousRows = null;
      return UnmodifiableListView(
        _delegate
            .flatten(
              data: data,
              expandedIds: expandedIds,
              filterQuery: filterQuery,
            )
            .where((node) => !node.id.startsWith(placeholderPrefix))
            .toList(growable: false),
      );
    }

    final root = data.nodes[data.visibleRootId];
    if ((filterQuery?.trim().isNotEmpty ?? false) ||
        !data.omitContainerRowAtRoot ||
        data.rootId != data.visibleRootId ||
        root == null ||
        root.id != data.rootId ||
        root.type != tree.NodeType.root ||
        root.parentId.isNotEmpty ||
        data.nodes.length < _minNodes ||
        root.childIds.length > _maxRootChildren) {
      return fallback();
    }
    final rootChildren = root.childIds.toSet();
    if (rootChildren.length != root.childIds.length) return fallback();

    final dirty = <String>{};
    final canReuse =
        previous != null &&
        changed != null &&
        previous.rootId == data.rootId &&
        _previousRows != null;
    if (canReuse) {
      final oldRoot = previous.nodes[previous.rootId]!;
      final oldChildren = oldRoot.childIds.toSet();
      bool mark(String id) =>
          _markBranch(previous, oldChildren, id, dirty) &&
          _markBranch(data, rootChildren, id, dirty);
      for (final id in changed) {
        if (!mark(id)) return fallback();
      }
      for (final id in _previousExpanded) {
        if (!expandedIds.contains(id) && !mark(id)) return fallback();
      }
      for (final id in expandedIds) {
        if (!_previousExpanded.contains(id) && !mark(id)) return fallback();
      }
    }

    final nextBranches = <String, List<tree.VisibleNode>>{};
    var sameRows = canReuse && _previousOrder.length == root.childIds.length;
    var index = 0;
    for (final id in root.childIds) {
      final old = canReuse ? _branches[id] : null;
      List<tree.VisibleNode>? rows;
      // A collapsed, unchanged root child cannot expose an edited hidden
      // descendant. Keep its previous row (or hidden placeholder) intact.
      final unchangedCollapsedChild =
          old != null &&
          old.length <= 1 &&
          !expandedIds.contains(id) &&
          identical(previous!.nodes[id], data.nodes[id]);
      if (old != null && (!dirty.contains(id) || unchangedCollapsedChild)) {
        rows = old;
      } else {
        rows = _flattenBranch(data, expandedIds, id);
        if (rows == null) return fallback();
      }
      nextBranches[id] = rows;
      if (sameRows && (_previousOrder[index] != id || !identical(rows, old))) {
        sameRows = false;
      }
      index++;
    }

    final rows = sameRows
        ? _previousRows!
        : UnmodifiableListView<tree.VisibleNode>([
            for (final branch in nextBranches.values) ...branch,
          ]);
    _previousData = data;
    _previousExpanded = Set<String>.of(expandedIds);
    _branches = nextBranches;
    _previousOrder = List<String>.of(root.childIds);
    _previousRows = rows;
    return rows;
  }

  // Verify the parent chain against the actual child links before using it
  // to invalidate a cached branch. Absent IDs on one side are normal for
  // additions/removals. Broken or ambiguous topology uses full traversal.
  static bool _markBranch(
    tree.TreeData data,
    Set<String> rootChildren,
    String id,
    Set<String> dirty,
  ) {
    if (id == data.rootId || !data.nodes.containsKey(id)) return true;
    var current = id;
    final seen = <String>{};
    while (seen.add(current)) {
      final node = data.nodes[current];
      if (node == null || node.id != current) return false;
      if (node.parentId == data.rootId) {
        if (!rootChildren.contains(current)) return false;
        dirty.add(current);
        return true;
      }
      final parent = data.nodes[node.parentId];
      if (parent == null || !parent.childIds.contains(current)) return false;
      current = node.parentId;
    }
    return false;
  }

  static List<tree.VisibleNode>? _flattenBranch(
    tree.TreeData data,
    Set<String> expandedIds,
    String branchId,
  ) {
    final rows = <tree.VisibleNode>[];
    final seen = <String>{};
    final stack = <(String, String, int)>[(branchId, data.rootId, 0)];
    while (stack.isNotEmpty) {
      final (id, parent, depth) = stack.removeLast();
      final node = data.nodes[id];
      if (node == null ||
          node.id != id ||
          node.parentId != parent ||
          !seen.add(id)) {
        return null;
      }
      final hasChildren = node.childIds.isNotEmpty;
      if (!id.startsWith(placeholderPrefix)) {
        rows.add(
          tree.VisibleNode(
            id: id,
            depth: depth,
            name: node.name,
            type: node.type,
            hasChildren: hasChildren,
            virtualPath: node.virtualPath,
            entryId: node.entryId,
            isVirtual: node.isVirtual,
            sourcePath: node.sourcePath,
            origin: node.origin,
          ),
        );
      }
      if (node.type != tree.NodeType.file &&
          hasChildren &&
          expandedIds.contains(id)) {
        for (var i = node.childIds.length - 1; i >= 0; i--) {
          stack.add((node.childIds[i], id, depth + 1));
        }
      }
    }
    return rows;
  }
}
