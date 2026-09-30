part of 'workspace_explorer.dart';

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

  Future<void> _restoreWorkspaceUiState() async {
    final snapshot = _readWorkspaceUiState();
    if (snapshot == null) {
      _expandedDirectoryPaths.clear();
      _selectedRelativePath = null;
      return;
    }

    final loadedDirectories = snapshot.loadedDirectories.toList(growable: false)
      ..sort(_compareDirectoryDepth);
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
      ..addAll(snapshot.expandedDirectories);
    _selectedRelativePath = snapshot.selectedRelativePath;
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
  ) async {
    final projection = await _workspaceFiles.projectExplorerTree(
      workspaceName: widget.workspace.name,
      workspacePath: widget.workspace.path,
      directories: _directorySnapshots(),
      replacement: native.WorkspaceExplorerDirectoryChildren(
        relativePath: relativePath,
        children: children,
      ),
    );
    if (!mounted) {
      return;
    }
    _applyProjection(projection);
    await _syncWatchedDirectories();
  }

  void _resetExplorerProjection() {
    _projection = null;
    _childrenByDirectory.clear();
    _entryByPath.clear();
    _entryByNodeId.clear();
  }

  void _applyProjection(native.WorkspaceExplorerTreeProjection projection) {
    _projection = projection;
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

  Future<void> _startNativeWatcher() async {
    try {
      final handle = await _workspaceFiles.startExplorerWatcher(
        workspacePath: widget.workspace.path,
      );
      if (!mounted) {
        await _workspaceFiles.stopExplorerWatcher(handle: handle);
        return;
      }
      _watcherHandle = handle;
      _watchSubscription = _workspaceFiles
          .watchExplorerEvents(handle: handle)
          .listen(_scheduleWatchedRefresh, onError: (_) {});
      await _syncWatchedDirectories();
    } catch (_) {
      // File watching is best-effort; manual refresh remains available.
    }
  }

  Future<void> _stopNativeWatcher() async {
    final subscription = _watchSubscription;
    final handle = _watcherHandle;
    _watchSubscription = null;
    _watcherHandle = null;
    _pendingWatchedDirectories.clear();
    _lastSyncedWatchedDirectories = null;
    await subscription?.cancel();
    if (handle != null) {
      await _workspaceFiles.stopExplorerWatcher(handle: handle);
    }
  }

  Future<void> _syncWatchedDirectories() async {
    final handle = _watcherHandle;
    if (handle == null) {
      return;
    }
    final watchedPaths = _childrenByDirectory.keys.toSet();
    final lastSynced = _lastSyncedWatchedDirectories;
    if (lastSynced != null &&
        lastSynced.length == watchedPaths.length &&
        lastSynced.containsAll(watchedPaths)) {
      return;
    }
    _lastSyncedWatchedDirectories = watchedPaths;
    try {
      await _workspaceFiles.updateExplorerWatcher(
        handle: handle,
        watchedRelativePaths: watchedPaths.toList(growable: false),
      );
    } catch (_) {
      _lastSyncedWatchedDirectories = null;
      // File watching is best-effort; explicit refresh still works.
    }
  }

  void _scheduleWatchedRefresh(native.WorkspaceExplorerWatchBatch batch) {
    _pendingWatchedDirectories.addAll(batch.directoryRelativePaths);
    if (_watchRefreshInFlight) {
      return;
    }
    _watchRefreshInFlight = true;
    unawaited(_drainWatchedRefreshes());
  }

  Future<void> _drainWatchedRefreshes() async {
    try {
      while (mounted && _pendingWatchedDirectories.isNotEmpty) {
        final relativePaths = _pendingWatchedDirectories.toList(
          growable: false,
        );
        _pendingWatchedDirectories.clear();
        try {
          await _refreshWatchedDirectories(relativePaths);
        } catch (_) {
          // Best-effort watcher refresh: a failed batch must not strand
          // directories that were queued behind it.
        }
      }
    } finally {
      _watchRefreshInFlight = false;
    }
  }

  Future<void> _refreshWatchedDirectories(List<String> relativePaths) async {
    await _refreshGitStatusSnapshot();
    for (final relativePath in relativePaths.toSet()) {
      if (!mounted || !_childrenByDirectory.containsKey(relativePath)) {
        continue;
      }
      await _refreshDirectory(relativePath, refreshGitStatus: false);
    }
  }
}
