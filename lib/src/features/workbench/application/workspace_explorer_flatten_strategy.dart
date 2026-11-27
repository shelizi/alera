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
  static const _minSegmentedRows = 2048;

  WorkspaceExplorerFlattenStrategy({this.segmentedRows = true});

  /// Retain the contiguous Phase 42 path for small lists and comparisons.
  final bool segmentedRows;

  tree.TreeData? _pendingData;
  Set<String>? _pendingChanges;
  tree.TreeData? _previousData;
  Set<String> _previousExpanded = const {};
  Map<String, _VisibleBranch> _branches = const {};
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

    final nextBranches = <String, _VisibleBranch>{};
    var sameRows = canReuse && _previousOrder.length == root.childIds.length;
    var index = 0;
    var totalRows = 0;
    var nonemptyBranches = 0;
    for (final id in root.childIds) {
      final old = canReuse ? _branches[id] : null;
      _VisibleBranch branch;
      // A collapsed, unchanged root child cannot expose an edited hidden
      // descendant. Keep its previous row (or hidden placeholder) intact.
      final unchangedCollapsedChild =
          old != null &&
          old.rows.length <= 1 &&
          !expandedIds.contains(id) &&
          identical(previous!.nodes[id], data.nodes[id]);
      if (old != null && (!dirty.contains(id) || unchangedCollapsedChild)) {
        branch = old;
      } else {
        final rows = _flattenBranch(data, expandedIds, id);
        if (rows == null) return fallback();
        branch = _VisibleBranch(rows);
      }
      nextBranches[id] = branch;
      totalRows += branch.rows.length;
      if (branch.rows.isNotEmpty) nonemptyBranches++;
      if (sameRows &&
          (_previousOrder[index] != id || !identical(branch, old))) {
        sameRows = false;
      }
      index++;
    }

    final rows = sameRows
        ? _previousRows!
        : segmentedRows &&
              totalRows >= _minSegmentedRows &&
              nonemptyBranches > 1
        ? _SegmentedVisibleRows(nextBranches)
        : UnmodifiableListView<tree.VisibleNode>([
            for (final branch in nextBranches.values) ...branch.rows,
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

/// Row buffers are owned by the flatten strategy and never mutated. The
/// branch-local index is allocated on first lookup and shared with later
/// publications that retain this branch, without storing global offsets.
final class _VisibleBranch {
  _VisibleBranch(this.rows);
  final List<tree.VisibleNode> rows;
  late final Map<String, int> _index = {
    for (var i = 0; i < rows.length; i++) rows[i].id: i,
  };

  int indexOf(String id) => _index[id] ?? -1;
}

/// The view and controller can retain this exact immutable list. Its
/// storage holds only branch references, cumulative ends and root offsets.
final class _SegmentedVisibleRows extends UnmodifiableListView<tree.VisibleNode>
    implements tree.IndexedVisibleRows {
  _SegmentedVisibleRows(Map<String, _VisibleBranch> branches)
    : this._(_SegmentStorage(branches));

  _SegmentedVisibleRows._(this._storage) : super(_storage);

  final _SegmentStorage _storage;

  @override
  Iterator<tree.VisibleNode> get iterator =>
      _storage.getRange(0, length).iterator;

  @override
  Iterable<tree.VisibleNode> getRange(int start, int end) =>
      _storage.getRange(start, end);

  @override
  int indexOfVisibleNode(String nodeId, tree.TreeData data) =>
      _storage.indexOfVisibleNode(nodeId, data);
}

final class _SegmentStorage extends ListBase<tree.VisibleNode> {
  _SegmentStorage(Map<String, _VisibleBranch> branches) {
    var offset = 0;
    for (final entry in branches.entries) {
      final branch = entry.value;
      if (branch.rows.isEmpty) continue;
      _locations[entry.key] = (branch, offset);
      _segments.add(branch);
      offset += branch.rows.length;
      _ends.add(offset);
    }
  }

  final _locations = <String, (_VisibleBranch, int)>{};
  final _segments = <_VisibleBranch>[];
  final _ends = <int>[];

  @override
  int get length => _ends.isEmpty ? 0 : _ends.last;

  int _segmentIndex(int index) {
    var low = 0;
    var high = _ends.length;
    while (low < high) {
      final mid = (low + high) >> 1;
      if (_ends[mid] <= index) {
        low = mid + 1;
      } else {
        high = mid;
      }
    }
    return low;
  }

  int _startOf(int segment) => segment == 0 ? 0 : _ends[segment - 1];

  @override
  tree.VisibleNode operator [](int index) {
    RangeError.checkValidIndex(index, this);
    final segment = _segmentIndex(index);
    return _segments[segment].rows[index - _startOf(segment)];
  }

  @override
  set length(int value) => throw UnsupportedError('Immutable visible rows');

  @override
  void operator []=(int index, tree.VisibleNode value) =>
      throw UnsupportedError('Immutable visible rows');

  @override
  Iterable<tree.VisibleNode> getRange(int start, int end) {
    RangeError.checkValidRange(start, end, length);
    return _range(start, end);
  }

  // Iterate a contiguous slice without a binary search for every row.
  Iterable<tree.VisibleNode> _range(int start, int end) sync* {
    if (start == end) return;
    var segment = _segmentIndex(start);
    var index = start;
    while (index < end) {
      final rows = _segments[segment].rows;
      final base = _startOf(segment);
      final limit = end < _ends[segment] ? end : _ends[segment];
      for (; index < limit; index++) {
        yield rows[index - base];
      }
      segment++;
    }
  }

  int indexOfVisibleNode(String nodeId, tree.TreeData data) {
    if (nodeId == data.rootId) return -1;
    var current = nodeId;
    final seen = <String>{};
    while (seen.add(current)) {
      final node = data.nodes[current];
      if (node == null || node.id != current) return -1;
      if (node.parentId == data.rootId) {
        final location = _locations[current];
        if (location == null) return -1;
        final local = location.$1.indexOf(nodeId);
        return local < 0 ? -1 : location.$2 + local;
      }
      if (node.parentId.isEmpty) return -1;
      current = node.parentId;
    }
    return -1;
  }
}
