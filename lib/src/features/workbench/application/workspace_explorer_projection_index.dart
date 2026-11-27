import 'package:alera/src/rust/api/workspace_files.dart' as native;

/// Owns the Dart entry projection. Native remains authoritative for tree
/// structure and bindings; only direct, uniquely owned directory children
/// are patched incrementally. Consumers must not mutate the exposed maps.
final class WorkspaceExplorerProjectionIndex {
  final childrenByDirectory = <String, List<native.WorkspaceFileEntry>>{};
  final entryByPath = <String, native.WorkspaceFileEntry>{};
  final entryByNodeId = <String, native.WorkspaceFileEntry>{};
  final _bindingPaths = <String, String>{};
  bool _directPaths = true;

  void clear() {
    childrenByDirectory.clear();
    entryByPath.clear();
    entryByNodeId.clear();
    _bindingPaths.clear();
    _directPaths = true;
  }

  void replace(native.WorkspaceExplorerTreeProjection projection) {
    clear();
    for (final directory in projection.directories) {
      childrenByDirectory[directory.relativePath] = directory.children;
      _directPaths = _directPaths && _isDirectDirectory(directory);
    }
    for (final binding in projection.entryBindings) {
      _bindingPaths[binding.nodeId] = binding.relativePath;
      _directPaths = _directPaths && _isDirectBinding(binding);
    }
    _rebuildEntryIndexes();
  }

  void apply(native.WorkspaceExplorerProjectionDelta delta) {
    final affectedDirectories = <String>{
      ...delta.removedDirectoryPaths,
      for (final directory in delta.changedDirectories) directory.relativePath,
    };
    // A noncanonical payload can contain overlapping directory children or
    // arbitrary binding IDs. Retain the full-rebuild policy for the rest of
    // this generation, until a complete projection proves the invariant again.
    _directPaths =
        _directPaths &&
        delta.changedDirectories.every(_isDirectDirectory) &&
        delta.changedBindings.every(_isDirectBinding);

    if (_directPaths) {
      // Remove all old rows before adding new rows, regardless of delta order.
      for (final path in affectedDirectories) {
        for (final entry
            in childrenByDirectory[path] ??
                const <native.WorkspaceFileEntry>[]) {
          entryByPath.remove(entry.relativePath);
          entryByNodeId.remove(_nodeId(entry.relativePath));
        }
      }
    }

    for (final path in delta.removedDirectoryPaths) {
      childrenByDirectory.remove(path);
    }
    for (final directory in delta.changedDirectories) {
      childrenByDirectory[directory.relativePath] = directory.children;
    }
    for (final id in delta.removedBindingNodeIds) {
      _bindingPaths.remove(id);
      entryByNodeId.remove(id);
    }
    for (final binding in delta.changedBindings) {
      _bindingPaths[binding.nodeId] = binding.relativePath;
    }

    if (!_directPaths) {
      _rebuildEntryIndexes();
      return;
    }
    for (final path in affectedDirectories) {
      for (final entry
          in childrenByDirectory[path] ?? const <native.WorkspaceFileEntry>[]) {
        entryByPath[entry.relativePath] = entry;
        final id = _nodeId(entry.relativePath);
        // Do not manufacture bindings for placeholders or unbound entries.
        if (_bindingPaths[id] == entry.relativePath) {
          entryByNodeId[id] = entry;
        }
      }
    }
    // A binding can arrive without a directory change (e.g. an implicit
    // parent becomes bound). Resolve against the final entry map.
    for (final binding in delta.changedBindings) {
      final entry = entryByPath[binding.relativePath];
      if (entry == null) {
        entryByNodeId.remove(binding.nodeId);
      } else {
        entryByNodeId[binding.nodeId] = entry;
      }
    }
  }

  List<native.WorkspaceExplorerDirectoryChildren> snapshots() {
    final paths = childrenByDirectory.keys.toList()..sort();
    return [
      for (final path in paths)
        native.WorkspaceExplorerDirectoryChildren(
          relativePath: path,
          children: childrenByDirectory[path]!,
        ),
    ];
  }

  void _rebuildEntryIndexes() {
    // Match the full native projection's directory and binding order when
    // nonstandard input permits duplicate entries. The last entry wins.
    entryByPath.clear();
    final paths = childrenByDirectory.keys.toList()..sort();
    for (final path in paths) {
      for (final entry in childrenByDirectory[path]!) {
        entryByPath[entry.relativePath] = entry;
      }
    }
    entryByNodeId.clear();
    final ids = _bindingPaths.keys.toList()..sort();
    for (final id in ids) {
      final entry = entryByPath[_bindingPaths[id]];
      if (entry != null) entryByNodeId[id] = entry;
    }
  }

  static String _nodeId(String path) => 'path:$path';

  static bool _isDirectBinding(native.WorkspaceExplorerEntryBinding binding) =>
      binding.nodeId == _nodeId(binding.relativePath);

  static bool _isDirectDirectory(
    native.WorkspaceExplorerDirectoryChildren directory,
  ) {
    final path = directory.relativePath;
    if (path.isNotEmpty &&
        path
            .split('/')
            .any((part) => part.isEmpty || part == '.' || part == '..')) {
      return false;
    }
    final prefix = path.isEmpty ? '' : '$path/';
    final seen = <String>{};
    for (final entry in directory.children) {
      final relative = entry.relativePath;
      if (!relative.startsWith(prefix)) return false;
      final name = relative.substring(prefix.length);
      if (name.isEmpty ||
          name == '.' ||
          name == '..' ||
          name.contains('/') ||
          !seen.add(relative)) {
        return false;
      }
    }
    return true;
  }
}
