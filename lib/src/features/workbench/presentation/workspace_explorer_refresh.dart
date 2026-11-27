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
    final initialGeneration = _nativeProjectionGeneration;
    await _startNativeWatcher();
    if (!mounted || initialGeneration != _nativeProjectionGeneration) return;
    final rootLoad = _reloadRoot();
    final generation = _nativeProjectionGeneration;
    bool isCurrent() => mounted && generation == _nativeProjectionGeneration;
    await rootLoad;
    if (!isCurrent()) return;
    await _restoreWorkspaceUiState();
    if (!isCurrent()) return;
    await _revealPendingPath();
  }

  Future<void> _restartExplorer() async {
    final generation = _nativeProjectionGeneration;
    await _stopNativeWatcher();
    // A newer workspace navigation may have started while the old native
    // stream was being cancelled. Only the latest request can re-bootstrap.
    if (!mounted || generation != _nativeProjectionGeneration) {
      return;
    }
    await _bootstrapExplorer();
  }

  Future<void> _restoreWorkspaceUiState({
    _WorkspaceExplorerUiState? snapshot,
  }) async {
    final generation = _nativeProjectionGeneration;
    bool isCurrent() => mounted && generation == _nativeProjectionGeneration;
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
      if (!isCurrent()) return;
      if (!_isDirectoryEntry(_entryByPath[relativePath])) {
        continue;
      }
      await _loadDirectory(relativePath);
    }
    if (!isCurrent()) return;

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
    _publishPendingTreeStructure();

    restoreWorkspaceExplorerControllerState(
      controller: _controller,
      index: _projectionIndex,
      expandedPaths: _expandedDirectoryPaths,
      selectedPath: _selectedRelativePath,
    );
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
          // An old projection may fail after a workspace switch. It must not
          // retire the newer workspace's active native handle or fallback.
          if (!mounted || generation != _nativeProjectionGeneration) return;
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
    final structural =
        !_hasProjection ||
        delta.changedNodes.isNotEmpty ||
        delta.removedNodeIds.isNotEmpty ||
        delta.changedBindings.isNotEmpty ||
        delta.removedBindingNodeIds.isNotEmpty ||
        delta.removedDirectoryPaths.isNotEmpty;
    if (!structural && delta.changedDirectories.isEmpty) return;

    _projectionIndex.apply(delta);
    _entryProjectionRevision += 1;
    if (structural) {
      // Native includes the root on the first projection, even for an
      // empty directory. Unchanged Dart nodes and entries remain in place.
      for (final id in delta.removedNodeIds) {
        _treeNodesById.remove(id);
      }
      for (final node in delta.changedNodes) {
        _treeNodesById[node.id] = _toDartExplorerTreeNode(node);
      }
      _hasProjection = true;
      _treeStructureRevision += 1;
    } else {
      // Repaint the instantiated rows without republishing the tree.
      _rowMetadataRevision.value += 1;
    }
  }

  void _disposeNativeProjectionSession({bool bumpGeneration = true}) {
    if (bumpGeneration) {
      _nativeProjectionGeneration += 1;
      _cancelBatchedTreeRebuild();
      _publishedTreeStructureRevision = _treeStructureRevision;
      _gitSnapshotRefresh.clear();
      _gitStatusSnapshot = const GitExplorerStatusSnapshot.empty();
    }
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
    // Pending native requests from the previous workspace may be slow or
    // unresponsive. Future requests belong to a new versioned session and
    // must not wait on the retired workspace's serialization chain.
    _nativeProjectionQueue = Future<void>.value();
    _nativeProjectionDisabled = false;
    _hasProjection = false;
    _treeNodesById.clear();
    _projectionIndex.clear();
    _entryProjectionRevision += 1;
  }

  void _applyProjection(native.WorkspaceExplorerTreeProjection projection) {
    _projectionIndex.replace(projection);
    _hasProjection = true;
    _treeStructureRevision += 1;
    _entryProjectionRevision += 1;
    _treeNodesById
      ..clear()
      ..addEntries(
        projection.nodes.map(
          (node) => MapEntry(node.id, _toDartExplorerTreeNode(node)),
        ),
      );
  }

  List<native.WorkspaceExplorerDirectoryChildren> _directorySnapshots() =>
      _projectionIndex.snapshots();
}
