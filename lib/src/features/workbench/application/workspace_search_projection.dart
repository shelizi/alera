import 'package:alera/src/rust/api/workspace_search.dart' as native;
import 'package:path/path.dart' as p;

final p.Context _workspaceSearchPathContext = p.Context(style: p.Style.posix);

String workspaceSearchDirectoryNodeKey(String relativePath) {
  return 'dir:$relativePath';
}

String workspaceSearchFileNodeKey(String relativePath) {
  return 'file:$relativePath';
}

List<String> workspaceSearchDirectoryPaths(String relativePath) {
  final segments = _workspaceSearchPathSegments(relativePath);
  if (segments.length <= 1) {
    return const <String>[];
  }
  final paths = <String>[];
  final current = <String>[];
  for (final segment in segments.take(segments.length - 1)) {
    current.add(segment);
    paths.add(_workspaceSearchPathContext.joinAll(current));
  }
  return paths;
}

Set<String> workspaceSearchCollapsibleNodeKeys(
  native.WorkspaceSearchResult? result, {
  required bool viewAsTree,
}) {
  if (result == null) {
    return const <String>{};
  }
  return WorkspaceSearchProjection(result)
      .collapsibleNodeKeys(viewAsTree: viewAsTree);
}

sealed class WorkspaceSearchProjectionRow {
  const WorkspaceSearchProjectionRow({required this.depth});

  final int depth;
}

final class WorkspaceSearchProjectionDirectoryRow
    extends WorkspaceSearchProjectionRow {
  const WorkspaceSearchProjectionDirectoryRow({
    required this.name,
    required this.path,
    required super.depth,
    required this.matchCount,
  });

  final String name;
  final String path;
  final int matchCount;
}

final class WorkspaceSearchProjectionFileRow
    extends WorkspaceSearchProjectionRow {
  const WorkspaceSearchProjectionFileRow(this.file, {required super.depth});

  final native.WorkspaceSearchFileResult file;
}

final class WorkspaceSearchProjectionMatchRow
    extends WorkspaceSearchProjectionRow {
  const WorkspaceSearchProjectionMatchRow(
    this.file,
    this.match, {
    required super.depth,
  });

  final native.WorkspaceSearchFileResult file;
  final native.WorkspaceSearchMatch match;
}

/// Immutable projection for one native workspace-search result.
///
/// Search results are replaced wholesale when a new request completes. The
/// expensive path parsing, grouping, and sorting work therefore only needs to
/// run once per result. Collapse/expand changes can reuse the retained row
/// descriptors and only flatten the visible subset.
final class WorkspaceSearchProjection {
  WorkspaceSearchProjection(this.result);

  final native.WorkspaceSearchResult result;

  late final List<_ProjectedFlatFile> _flatFiles = <_ProjectedFlatFile>[
    for (final file in result.files) _ProjectedFlatFile(file),
  ];

  late final _ProjectedTreeDirectory _treeRoot = _buildTree();

  late final Set<String> _flatCollapsibleNodeKeys = Set<String>.unmodifiable(
    <String>{for (final file in _flatFiles) file.nodeKey},
  );

  late final Set<String> _treeCollapsibleNodeKeys = () {
    final keys = <String>{};
    _treeRoot.collectCollapsibleNodeKeys(keys);
    return Set<String>.unmodifiable(keys);
  }();

  Set<String> collapsibleNodeKeys({required bool viewAsTree}) {
    return viewAsTree ? _treeCollapsibleNodeKeys : _flatCollapsibleNodeKeys;
  }

  List<WorkspaceSearchProjectionRow> rows({
    required Set<String> collapsedResultNodeKeys,
    required bool viewAsTree,
  }) {
    final rows = <WorkspaceSearchProjectionRow>[];
    if (viewAsTree) {
      _treeRoot.appendVisibleRows(rows, collapsedResultNodeKeys);
      return rows;
    }
    for (final file in _flatFiles) {
      rows.add(file.row);
      if (collapsedResultNodeKeys.contains(file.nodeKey)) {
        continue;
      }
      rows.addAll(file.matchRows);
    }
    return rows;
  }

  _ProjectedTreeDirectory _buildTree() {
    final root = _MutableSearchTreeDirectory('', '');
    for (final file in result.files) {
      final segments = _workspaceSearchPathSegments(file.relativePath);
      if (segments.length <= 1) {
        root.files.add(file);
        continue;
      }
      var directory = root;
      for (var index = 0; index < segments.length - 1; index += 1) {
        final name = segments[index];
        final path = directory.path.isEmpty
            ? name
            : _workspaceSearchPathContext.join(directory.path, name);
        directory = directory.directories.putIfAbsent(
          name,
          () => _MutableSearchTreeDirectory(name, path),
        );
        directory.matchCount += file.matches.length;
      }
      directory.files.add(file);
    }
    return _freezeTreeDirectory(root, depth: -1);
  }
}

final class _ProjectedFlatFile {
  _ProjectedFlatFile(this.file);

  final native.WorkspaceSearchFileResult file;

  late final String nodeKey = workspaceSearchFileNodeKey(file.relativePath);
  late final WorkspaceSearchProjectionFileRow row =
      WorkspaceSearchProjectionFileRow(file, depth: 0);
  late final List<WorkspaceSearchProjectionMatchRow> matchRows =
      <WorkspaceSearchProjectionMatchRow>[
        for (final match in file.matches)
          WorkspaceSearchProjectionMatchRow(file, match, depth: 0),
      ];
}

final class _ProjectedTreeFile {
  _ProjectedTreeFile(this.file, {required this.depth});

  final native.WorkspaceSearchFileResult file;
  final int depth;

  late final String nodeKey = workspaceSearchFileNodeKey(file.relativePath);
  late final WorkspaceSearchProjectionFileRow row =
      WorkspaceSearchProjectionFileRow(file, depth: depth);
  late final List<WorkspaceSearchProjectionMatchRow> matchRows =
      <WorkspaceSearchProjectionMatchRow>[
        for (final match in file.matches)
          WorkspaceSearchProjectionMatchRow(file, match, depth: depth + 1),
      ];

  void appendVisibleRows(
    List<WorkspaceSearchProjectionRow> rows,
    Set<String> collapsedResultNodeKeys,
  ) {
    rows.add(row);
    if (!collapsedResultNodeKeys.contains(nodeKey)) {
      rows.addAll(matchRows);
    }
  }
}

final class _ProjectedTreeDirectory {
  _ProjectedTreeDirectory({
    required this.name,
    required this.path,
    required this.depth,
    required this.matchCount,
    required this.directories,
    required this.files,
  });

  final String name;
  final String path;
  final int depth;
  final int matchCount;
  final List<_ProjectedTreeDirectory> directories;
  final List<_ProjectedTreeFile> files;

  late final String nodeKey = workspaceSearchDirectoryNodeKey(path);
  late final WorkspaceSearchProjectionDirectoryRow row =
      WorkspaceSearchProjectionDirectoryRow(
        name: name,
        path: path,
        depth: depth,
        matchCount: matchCount,
      );

  void appendVisibleRows(
    List<WorkspaceSearchProjectionRow> rows,
    Set<String> collapsedResultNodeKeys,
  ) {
    for (final directory in directories) {
      rows.add(directory.row);
      if (!collapsedResultNodeKeys.contains(directory.nodeKey)) {
        directory.appendVisibleRows(rows, collapsedResultNodeKeys);
      }
    }
    for (final file in files) {
      file.appendVisibleRows(rows, collapsedResultNodeKeys);
    }
  }

  void collectCollapsibleNodeKeys(Set<String> keys) {
    for (final directory in directories) {
      keys.add(directory.nodeKey);
      directory.collectCollapsibleNodeKeys(keys);
    }
    for (final file in files) {
      keys.add(file.nodeKey);
    }
  }
}

final class _MutableSearchTreeDirectory {
  _MutableSearchTreeDirectory(this.name, this.path);

  final String name;
  final String path;
  int matchCount = 0;
  final Map<String, _MutableSearchTreeDirectory> directories =
      <String, _MutableSearchTreeDirectory>{};
  final List<native.WorkspaceSearchFileResult> files =
      <native.WorkspaceSearchFileResult>[];
}

_ProjectedTreeDirectory _freezeTreeDirectory(
  _MutableSearchTreeDirectory directory, {
  required int depth,
}) {
  final sortedDirectories = directory.directories.values.toList(growable: false)
    ..sort((a, b) => a.name.compareTo(b.name));
  final sortedFiles = directory.files.toList(growable: false)
    ..sort((a, b) => a.relativePath.compareTo(b.relativePath));
  return _ProjectedTreeDirectory(
    name: directory.name,
    path: directory.path,
    depth: depth,
    matchCount: directory.matchCount,
    directories: <_ProjectedTreeDirectory>[
      for (final child in sortedDirectories)
        _freezeTreeDirectory(child, depth: depth + 1),
    ],
    files: <_ProjectedTreeFile>[
      for (final file in sortedFiles)
        _ProjectedTreeFile(file, depth: depth + 1),
    ],
  );
}

List<String> _workspaceSearchPathSegments(String relativePath) {
  final normalized = relativePath.replaceAll('\\', '/');
  return _workspaceSearchPathContext
      .split(normalized)
      .where((segment) => segment.isNotEmpty)
      .toList(growable: false);
}
