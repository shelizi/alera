import 'dart:collection';

import 'package:flutter_directory_tree/flutter_directory_tree.dart' as tree;

/// Incrementally flattens Alera's rooted, immutable native Explorer graph.
///
/// Cache row buffers per branch and optionally split large expanded branches
/// into bounded-depth child segments. Known structural and expansion edits
/// invalidate their old and new ancestor paths. Unknown edits, filtering,
/// non-rooted graphs and very wide roots retain the reference traversal.
final class WorkspaceExplorerFlattenStrategy extends tree.FlattenStrategy {
  static const placeholderPrefix = '__alera_placeholder__:';
  static const _delegate = tree.DefaultFlattenStrategy();
  static const _minNodes = 256;
  static const _maxRootChildren = 2048;
  static const _minSegmentedRows = 2048;

  static const _maxNestedDepth = 6;
  static const _maxNestedChildren = 512;
  static const _maxDirectFileEdits = 32;
  static const _maxDirectOverlayRows = 512;

  WorkspaceExplorerFlattenStrategy({
    this.segmentedRows = true,
    this.nestedBranches = true,
  });

  /// Retain the contiguous Phase 42 path for small lists and comparisons.
  final bool segmentedRows;

  /// Retain the root-only Phase 43 strategy for comparisons and fallback.
  final bool nestedBranches;

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
      final branch = _buildBranch(
        data,
        expandedIds,
        id,
        data.rootId,
        0,
        previous: canReuse ? previous : null,
        old: old,
        dirty: dirty,
      );
      if (branch == null) return fallback();
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
              (nonemptyBranches > 1 ||
                  nextBranches.values.any(
                    (b) => b.rows is tree.IndexedVisibleRows,
                  ))
        ? _SegmentedVisibleRows(nextBranches, parentId: data.rootId)
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
      dirty.add(current);
      if (node.parentId == data.rootId) {
        if (!rootChildren.contains(current)) return false;
        return true;
      }
      final parent = data.nodes[node.parentId];
      if (parent == null || !parent.childIds.contains(current)) return false;
      current = node.parentId;
    }
    return false;
  }

  _VisibleBranch? _buildBranch(
    tree.TreeData data,
    Set<String> expandedIds,
    String id,
    String parentId,
    int depth, {
    required tree.TreeData? previous,
    required _VisibleBranch? old,
    required Set<String> dirty,
  }) {
    // A collapsed unchanged node cannot expose edited hidden descendants.
    if (old != null &&
        (!dirty.contains(id) ||
            (old.rows.length <= 1 &&
                !expandedIds.contains(id) &&
                identical(previous!.nodes[id], data.nodes[id])))) {
      return old;
    }

    final node = data.nodes[id];
    if (node == null || node.id != id || node.parentId != parentId) {
      return null;
    }
    final expanded =
        node.type != tree.NodeType.file &&
        node.childIds.isNotEmpty &&
        expandedIds.contains(id);
    final canNest =
        segmentedRows &&
        nestedBranches &&
        expanded &&
        !id.startsWith(placeholderPrefix);

    // For a wide pure-file folder, retain a verified immutable base and
    // patch only a small number of same-position row replacements.
    if (canNest && old?.rows is _DirectFileRows && previous != null) {
      final previousNode = previous.nodes[id];
      if (previousNode != null) {
        final patched = _patchDirectFiles(
          old!.rows as _DirectFileRows,
          previousNode,
          node,
          depth,
          data,
          dirty,
        );
        if (patched != null) {
          return identical(patched, old.rows) ? old : _VisibleBranch(patched);
        }
      }
    }

    // Split a directory with expanded child folders. Each child owns its
    // own row buffer, and nested segments do not concatenate descendants.
    // Bound recursion and fan-out; wide flat folders use the path above.
    if (canNest &&
        depth < _maxNestedDepth &&
        node.childIds.length <= _maxNestedChildren &&
        (old?.children != null ||
            node.childIds.any((childId) {
              if (!expandedIds.contains(childId)) return false;
              final child = data.nodes[childId];
              return child != null &&
                  child.type != tree.NodeType.file &&
                  child.childIds.isNotEmpty;
            }))) {
      final children = <String, _VisibleBranch>{};
      for (final childId in node.childIds) {
        if (children.containsKey(childId)) return null;
        final child = _buildBranch(
          data,
          expandedIds,
          childId,
          id,
          depth + 1,
          previous: previous,
          old: old?.children?[childId],
          dirty: dirty,
        );
        if (child == null) return null;
        children[childId] = child;
      }
      return _VisibleBranch(
        _SegmentedVisibleRows(
          children,
          parentId: id,
          prefix: [_row(node, depth)],
        ),
        children: children,
      );
    }

    final rows = _flattenBranch(data, expandedIds, id, parentId, depth);
    if (rows == null) return null;
    // The reference walk already validated parent links and uniqueness.
    // Enable a sparse direct-file overlay only for an exact, simple shape.
    if (canNest &&
        rows.length >= _minSegmentedRows &&
        rows.length == node.childIds.length + 1 &&
        rows.first.id == id) {
      var directFiles = true;
      for (var i = 1; i < rows.length; i++) {
        final row = rows[i];
        if (row.id != node.childIds[i - 1] ||
            row.depth != depth + 1 ||
            row.type != tree.NodeType.file ||
            row.hasChildren) {
          directFiles = false;
          break;
        }
      }
      if (directFiles) return _VisibleBranch(_DirectFileRows(rows));
    }
    return _VisibleBranch(rows);
  }

  static _DirectFileRows? _patchDirectFiles(
    _DirectFileRows old,
    tree.TreeNode previous,
    tree.TreeNode next,
    int depth,
    tree.TreeData data,
    Set<String> dirty,
  ) {
    if (previous.childIds.length != next.childIds.length ||
        old.length != next.childIds.length + 1 ||
        old.first.id != next.id ||
        old.first.depth != depth) {
      return null;
    }
    final edits = <int, tree.VisibleNode>{};
    if (!identical(previous, next)) edits[0] = _row(next, depth);
    final addedIds = <String>{};
    for (var i = 0; i < next.childIds.length; i++) {
      final id = next.childIds[i];
      final oldId = previous.childIds[i];
      if (id == oldId && !dirty.contains(id)) continue;
      if (edits.length >= _maxDirectFileEdits) return null;
      // Reorderings of retained IDs, duplicates, and non-file entries use
      // the full validated traversal rather than risking an ambiguous index.
      if (id != oldId &&
          (old.indexOfVisibleNode(id, data) >= 0 || !addedIds.add(id))) {
        return null;
      }
      final node = data.nodes[id];
      if (node == null ||
          node.id != id ||
          node.parentId != next.id ||
          node.type != tree.NodeType.file ||
          node.childIds.isNotEmpty ||
          id.startsWith(placeholderPrefix)) {
        return null;
      }
      edits[i + 1] = _row(node, depth + 1);
    }
    if (edits.isEmpty) return old;
    return old.withEdits(edits, _maxDirectOverlayRows);
  }

  static tree.VisibleNode _row(tree.TreeNode node, int depth) =>
      tree.VisibleNode(
        id: node.id,
        depth: depth,
        name: node.name,
        type: node.type,
        hasChildren: node.childIds.isNotEmpty,
        virtualPath: node.virtualPath,
        entryId: node.entryId,
        isVirtual: node.isVirtual,
        sourcePath: node.sourcePath,
        origin: node.origin,
      );

  static List<tree.VisibleNode>? _flattenBranch(
    tree.TreeData data,
    Set<String> expandedIds,
    String branchId,
    String parentId,
    int startDepth,
  ) {
    final rows = <tree.VisibleNode>[];
    final seen = <String>{};
    final stack = <(String, String, int)>[(branchId, parentId, startDepth)];
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
        rows.add(_row(node, depth));
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
  _VisibleBranch(this.rows, {this.children});
  final List<tree.VisibleNode> rows;
  final Map<String, _VisibleBranch>? children;
  late final Map<String, int> _index = {
    for (var i = 0; i < rows.length; i++) rows[i].id: i,
  };

  int indexOf(String id, tree.TreeData data) {
    final indexed = rows;
    if (indexed is tree.IndexedVisibleRows) {
      return indexed.indexOfVisibleNode(id, data);
    }
    return _index[id] ?? -1;
  }
}

/// The view and controller can retain this exact immutable list. Its
/// storage holds only branch references, cumulative ends and root offsets.
final class _SegmentedVisibleRows extends UnmodifiableListView<tree.VisibleNode>
    implements tree.IndexedVisibleRows {
  _SegmentedVisibleRows(
    Map<String, _VisibleBranch> branches, {
    required String parentId,
    List<tree.VisibleNode> prefix = const [],
  }) : this._(_SegmentStorage(branches, parentId, prefix));

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
  _SegmentStorage(
    Map<String, _VisibleBranch> branches,
    this._parentId,
    this._prefix,
  ) {
    var offset = _prefix.length;
    if (_prefix.isNotEmpty) {
      _segments.add(_VisibleBranch(_prefix));
      _ends.add(offset);
    }
    for (final entry in branches.entries) {
      final branch = entry.value;
      if (branch.rows.isEmpty) continue;
      _locations[entry.key] = (branch, offset);
      _segments.add(branch);
      offset += branch.rows.length;
      _ends.add(offset);
    }
  }

  final String _parentId;
  final List<tree.VisibleNode> _prefix;
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
      yield* rows.getRange(index - base, limit - base);
      index = limit;
      segment++;
    }
  }

  int indexOfVisibleNode(String nodeId, tree.TreeData data) {
    if (nodeId == _parentId) {
      return _prefix.isNotEmpty && _prefix.first.id == nodeId ? 0 : -1;
    }
    var current = nodeId;
    final seen = <String>{};
    while (seen.add(current)) {
      final node = data.nodes[current];
      if (node == null || node.id != current) return -1;
      if (node.parentId == _parentId) {
        final location = _locations[current];
        if (location == null) return -1;
        final local = location.$1.indexOf(nodeId, data);
        return local < 0 ? -1 : location.$2 + local;
      }
      if (node.parentId.isEmpty) return -1;
      current = node.parentId;
    }
    return -1;
  }
}

/// The compact direct-file base owns its rows and lazily builds one ID map.
/// Sparse publications share it, never forming a chain of prior snapshots.
final class _DirectFileBase {
  _DirectFileBase(this.rows);
  final List<tree.VisibleNode> rows;
  late final Map<String, int> index = {
    for (var i = 0; i < rows.length; i++) rows[i].id: i,
  };
}

final class _DirectFileRows extends UnmodifiableListView<tree.VisibleNode>
    implements tree.IndexedVisibleRows {
  _DirectFileRows(List<tree.VisibleNode> rows)
    : this._(_DirectFileStorage(_DirectFileBase(rows), const {}));

  _DirectFileRows._(this._storage) : super(_storage);

  final _DirectFileStorage _storage;

  _DirectFileRows? withEdits(Map<int, tree.VisibleNode> edits, int maxRecords) {
    final merged = <int, tree.VisibleNode>{..._storage.edits, ...edits};
    if (merged.length > maxRecords) return null;
    return _DirectFileRows._(_DirectFileStorage(_storage.base, merged));
  }

  @override
  int indexOfVisibleNode(String nodeId, tree.TreeData data) =>
      _storage.indexOfId(nodeId);
}

final class _DirectFileStorage extends ListBase<tree.VisibleNode> {
  _DirectFileStorage(this.base, this.edits);

  final _DirectFileBase base;
  final Map<int, tree.VisibleNode> edits;
  late final Map<String, int> _indexEdits = {
    for (final i in edits.keys) base.rows[i].id: -1,
    for (final entry in edits.entries) entry.value.id: entry.key,
  };

  int indexOfId(String id) => _indexEdits[id] ?? base.index[id] ?? -1;

  @override
  int get length => base.rows.length;

  @override
  tree.VisibleNode operator [](int index) {
    RangeError.checkValidIndex(index, this);
    return edits[index] ?? base.rows[index];
  }

  @override
  set length(int value) => throw UnsupportedError('Immutable visible rows');

  @override
  void operator []=(int index, tree.VisibleNode value) =>
      throw UnsupportedError('Immutable visible rows');
}
