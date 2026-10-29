part of 'workspace_explorer.dart';

/// Avoid a full Dart -> Rust -> Dart projection roundtrip when a watcher
/// re-reads a directory with exactly the same ordered entry metadata.
/// Compare *all* fields that Rust receives: paths alone miss Git state,
/// file edits, symlinks, permissions and placeholder decisions.
@visibleForTesting
bool workspaceExplorerDirectoryChildrenUnchanged(
  List<native.WorkspaceFileEntry>? previous,
  List<native.WorkspaceFileEntry> current,
) {
  if (previous == null || previous.length != current.length) return false;
  for (var index = 0; index < current.length; index++) {
    final a = previous[index];
    final b = current[index];
    if (a.relativePath != b.relativePath ||
        a.name != b.name ||
        a.kind != b.kind ||
        a.size != b.size ||
        a.modifiedMillis != b.modifiedMillis ||
        a.contentToken != b.contentToken ||
        a.isIgnored != b.isIgnored ||
        a.isHidden != b.isHidden ||
        a.isSymlink != b.isSymlink ||
        a.isProtected != b.isProtected ||
        a.hasChildrenHint != b.hasChildrenHint ||
        a.gitStatus != b.gitStatus) {
      return false;
    }
  }
  return true;
}

extension _WorkspaceExplorerRefresh on _WorkspaceExplorerState {
  void _listenForRevealRequest() {
    ref.listen<WorkspaceExplorerRevealRequest?>(
      workspaceExplorerRevealControllerProvider,
      (previous, next) {
        if (next == null || next.workspaceId != widget.workspace.id) {
          return;
        }
        unawaited(_revealPendingPath(relativePath: next.relativePath));
      },
    );
  }

  Future<void> _bootstrapExplorer() async {
    await _startNativeWatcher();
    if (!mounted) {
      return;
    }
    await _reloadRoot();
    if (!mounted) {
      return;
    }
    await _restoreWorkspaceUiState();
    if (!mounted) {
      return;
    }
    await _revealPendingPath();
  }

  Future<void> _restartExplorer() async {
    unawaited(_stopNativeWatcher());
    if (!mounted) {
      return;
    }
    await _bootstrapExplorer();
  }

  Future<void> _restoreWorkspaceUiState({
    _WorkspaceExplorerUiState? snapshot,
  }) async {
    final savedState = snapshot ?? _readWorkspaceUiState();
    if (savedState == null) {
      _expandedDirectoryPaths.clear();
      _selectedRelativePath = null;
      return;
    }

    final loadedDirectories = savedState.loadedDirectories.toList(
      growable: false,
    )..sort(_compareDirectoryDepth);
    for (final relativePath in loadedDirectories) {
      if (!mounted || !_isDirectoryEntry(_entryByPath[relativePath])) {
        continue;
      }
      await _loadDirectory(relativePath);
    }
    if (!mounted) {
      return;
    }

    _expandedDirectoryPaths
      ..clear()
      ..addAll(
        savedState.expandedDirectories.where(
          (path) => _isDirectoryEntry(_entryByPath[path]),
        ),
      );
    _selectedRelativePath =
        _entryByPath.containsKey(savedState.selectedRelativePath)
        ? savedState.selectedRelativePath
        : null;
    _rebuildTree();

    for (final entry in _entryByNodeId.entries) {
      final relativePath = entry.value.relativePath;
      if (_expandedDirectoryPaths.contains(relativePath)) {
        _controller.expansions.setExpanded(entry.key, true);
      }
      if (relativePath == _selectedRelativePath) {
        _controller.selection.selectOnly(entry.key);
      }
    }
  }

  Future<void> _replaceDirectoryChildren(
    String relativePath,
    List<native.WorkspaceFileEntry> children,
  ) {
    final generation = _nativeProjectionGeneration;
    // A watcher event can overlap user-triggered refreshes; serialize native
    // versions so a later directory replacement never races an earlier one.
    final update = _nativeProjectionQueue.then((_) async {
      if (!mounted || generation != _nativeProjectionGeneration) return;
      if (workspaceExplorerDirectoryChildrenUnchanged(
        _childrenByDirectory[relativePath],
        children,
      )) {
        await _syncWatchedDirectories();
        return;
      }
      final replacement = native.WorkspaceExplorerDirectoryChildren(
        relativePath: relativePath,
        children: children,
      );
      if (!_nativeProjectionDisabled) {
        try {
          final handle = await _ensureNativeProjectionSession(generation);
          if (!mounted || generation != _nativeProjectionGeneration) return;
          final delta = await _workspaceFiles.updateExplorerProjectionSession(
            handle: handle,
            replacement: replacement,
          );
          if (!mounted || generation != _nativeProjectionGeneration) return;
          _applyNativeProjectionDelta(delta);
          _nativeProjectionHandle = native.WorkspaceExplorerProjectionHandle(
            id: handle.id,
            version: delta.version,
          );
          await _syncWatchedDirectories();
          return;
        } catch (_) {
          // Older native binaries, lost handles or stale versions must never
          // strand the Explorer; the stateless Rust projection remains valid.
          _disposeNativeProjectionSession(bumpGeneration: false);
          _nativeProjectionDisabled = true;
        }
      }
      final projection = await _workspaceFiles.projectExplorerTree(
        workspaceName: widget.workspace.name,
        workspacePath: widget.workspace.path,
        directories: _directorySnapshots(),
        replacement: replacement,
      );
      if (!mounted || generation != _nativeProjectionGeneration) return;
      _applyProjection(projection);
      await _syncWatchedDirectories();
    });
    _nativeProjectionQueue = update.catchError((Object _) {});
    return update;
  }

  Future<native.WorkspaceExplorerProjectionHandle>
  _ensureNativeProjectionSession(int generation) async {
    final active = _nativeProjectionHandle;
    if (active != null) return active;
    final starting = _nativeProjectionStarting ??= _workspaceFiles
        .startExplorerProjectionSession(
          workspaceName: widget.workspace.name,
          workspacePath: widget.workspace.path,
          directories: _directorySnapshots(),
        );
    final handle = await starting;
    if (!mounted || generation != _nativeProjectionGeneration) {
      unawaited(
        _workspaceFiles
            .stopExplorerProjectionSession(handle: handle)
            .catchError((Object _) {}),
      );
      throw StateError(
        'Explorer projection was replaced during initialization',
      );
    }
    _nativeProjectionHandle = handle;
    return handle;
  }

  void _applyNativeProjectionDelta(
    native.WorkspaceExplorerProjectionDelta delta,
  ) {
    if (_applyMetadataOnlyProjectionDelta(delta)) return;
    final directories = <String, native.WorkspaceExplorerDirectoryChildren>{
      for (final item
          in _projection?.directories ??
              const <native.WorkspaceExplorerDirectoryChildren>[])
        item.relativePath: item,
    };
    for (final path in delta.removedDirectoryPaths) {
      directories.remove(path);
    }
    for (final item in delta.changedDirectories) {
      directories[item.relativePath] = item;
    }
    final nodes = <String, native.WorkspaceExplorerTreeNode>{
      for (final node
          in _projection?.nodes ?? const <native.WorkspaceExplorerTreeNode>[])
        node.id: node,
    };
    for (final id in delta.removedNodeIds) {
      nodes.remove(id);
    }
    for (final node in delta.changedNodes) {
      nodes[node.id] = node;
    }
    final bindings = <String, native.WorkspaceExplorerEntryBinding>{
      for (final binding
          in _projection?.entryBindings ??
              const <native.WorkspaceExplorerEntryBinding>[])
        binding.nodeId: binding,
    };
    for (final id in delta.removedBindingNodeIds) {
      bindings.remove(id);
    }
    for (final binding in delta.changedBindings) {
      bindings[binding.nodeId] = binding;
    }
    final sortedDirectories = directories.values.toList(growable: false)
      ..sort((a, b) => a.relativePath.compareTo(b.relativePath));
    final sortedNodes = nodes.values.toList(growable: false)
      ..sort((a, b) {
        if (a.id == _WorkspaceExplorerState._rootId) return -1;
        if (b.id == _WorkspaceExplorerState._rootId) return 1;
        return a.virtualPath.compareTo(b.virtualPath);
      });
    final sortedBindings = bindings.values.toList(growable: false)
      ..sort((a, b) => a.nodeId.compareTo(b.nodeId));
    _applyProjection(
      native.WorkspaceExplorerTreeProjection(
        directories: sortedDirectories,
        nodes: sortedNodes,
        entryBindings: sortedBindings,
      ),
      delta: delta,
    );
  }

  /// A native metadata-only Delta has no graph or binding changes. Preserve
  /// the already allocated tree nodes/bindings and patch only the affected
  /// directory caches and entry indexes. Ordinary Git status, mtime and size
  /// changes no longer rebuild three whole-workspace Dart index maps.
  bool _applyMetadataOnlyProjectionDelta(
    native.WorkspaceExplorerProjectionDelta delta,
  ) {
    final previous = _projection;
    if (previous == null ||
        delta.changedNodes.isNotEmpty ||
        delta.removedNodeIds.isNotEmpty ||
        delta.changedBindings.isNotEmpty ||
        delta.removedBindingNodeIds.isNotEmpty ||
        delta.removedDirectoryPaths.isNotEmpty) {
      return false;
    }
    if (delta.changedDirectories.isEmpty) return true;

    final changes = <String, native.WorkspaceExplorerDirectoryChildren>{
      for (final directory in delta.changedDirectories)
        directory.relativePath: directory,
    };
    final directories = <native.WorkspaceExplorerDirectoryChildren>[
      for (final existing in previous.directories)
        changes.remove(existing.relativePath) ?? existing,
    ];
    if (changes.isNotEmpty) {
      // A newly loaded but empty folder may have no node changes.
      directories.addAll(changes.values);
      directories.sort(
        (left, right) => left.relativePath.compareTo(right.relativePath),
      );
    }

    for (final directory in delta.changedDirectories) {
      final oldChildren = _childrenByDirectory[directory.relativePath];
      if (oldChildren != null) {
        for (final entry in oldChildren) {
          _entryByPath.remove(entry.relativePath);
          _entryByNodeId.remove('path:${entry.relativePath}');
        }
      }
      _childrenByDirectory[directory.relativePath] = directory.children;
      for (final entry in directory.children) {
        _entryByPath[entry.relativePath] = entry;
        // Native Explorer bindings use path:<relativePath> for entries;
        // placeholders have no binding and are not part of these children.
        _entryByNodeId['path:${entry.relativePath}'] = entry;
      }
    }
    _entryProjectionRevision += 1;

    _projection = native.WorkspaceExplorerTreeProjection(
      directories: directories,
      nodes: previous.nodes,
      entryBindings: previous.entryBindings,
    );
    // Repaint the instantiated rows, not the whole tree's node hierarchy.
    _rowMetadataRevision.value += 1;
    return true;
  }

  void _disposeNativeProjectionSession({bool bumpGeneration = true}) {
    if (bumpGeneration) _nativeProjectionGeneration += 1;
    final active = _nativeProjectionHandle;
    final starting = _nativeProjectionStarting;
    _nativeProjectionHandle = null;
    _nativeProjectionStarting = null;
    if (active != null) {
      unawaited(
        _workspaceFiles
            .stopExplorerProjectionSession(handle: active)
            .catchError((Object _) {}),
      );
    } else if (starting != null) {
      unawaited(
        starting
            .then(
              (handle) =>
                  _workspaceFiles.stopExplorerProjectionSession(handle: handle),
            )
            .catchError((Object _) {}),
      );
    }
  }

  void _resetExplorerProjection() {
    _disposeNativeProjectionSession();
    _nativeProjectionDisabled = false;
    _projection = null;
    _treeNodesById.clear();
    _childrenByDirectory.clear();
    _entryByPath.clear();
    _entryByNodeId.clear();
    _entryProjectionRevision += 1;
  }

  void _applyProjection(
    native.WorkspaceExplorerTreeProjection projection, {
    native.WorkspaceExplorerProjectionDelta? delta,
  }) {
    _projection = projection;
    _treeStructureRevision += 1;
    if (delta != null && _treeNodesById.isNotEmpty) {
      for (final id in delta.removedNodeIds) {
        _treeNodesById.remove(id);
      }
      for (final node in delta.changedNodes) {
        _treeNodesById[node.id] = _toDartExplorerTreeNode(node);
      }
    } else {
      // First projection or stateless fallback must seed the full cache.
      _treeNodesById
        ..clear()
        ..addEntries(
          projection.nodes.map(
            (node) => MapEntry(node.id, _toDartExplorerTreeNode(node)),
          ),
        );
    }
    _childrenByDirectory
      ..clear()
      ..addEntries(
        projection.directories.map(
          (directory) => MapEntry(directory.relativePath, directory.children),
        ),
      );
    _entryByPath
      ..clear()
      ..addEntries(
        projection.directories.expand(
          (directory) => directory.children.map(
            (entry) => MapEntry(entry.relativePath, entry),
          ),
        ),
      );
    _entryByNodeId.clear();
    _entryProjectionRevision += 1;
    for (final binding in projection.entryBindings) {
      final entry = _entryByPath[binding.relativePath];
      if (entry != null) {
        _entryByNodeId[binding.nodeId] = entry;
      }
    }
  }

  List<native.WorkspaceExplorerDirectoryChildren> _directorySnapshots() {
    return _childrenByDirectory.entries
        .map(
          (entry) => native.WorkspaceExplorerDirectoryChildren(
            relativePath: entry.key,
            children: entry.value,
          ),
        )
        .toList(growable: false);
  }
}
