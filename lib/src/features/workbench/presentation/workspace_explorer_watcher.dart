part of 'workspace_explorer.dart';

/// Testable filtering boundary for native directory-watch batches. The index
/// needs refreshing for an unknown/empty batch or any relevant source file,
/// even when the same batch includes excluded metadata paths.
@visibleForTesting
bool workspaceExplorerWatcherTouchesSearch({
  required List<String> changedRelativePaths,
  required Set<String> excludedDirectories,
}) {
  final excluded = <String>{
    '.git',
    '.hg',
    '.svn',
    for (final name in excludedDirectories) name.toLowerCase(),
  };
  return changedRelativePaths.isEmpty ||
      changedRelativePaths.any(
        (path) => !path
            .split('/')
            .any((part) => excluded.contains(part.toLowerCase())),
      );
}

extension _WorkspaceExplorerWatcher on _WorkspaceExplorerState {
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
    final searchRelevant = workspaceExplorerWatcherTouchesSearch(
      changedRelativePaths: batch.changedRelativePaths,
      excludedDirectories: ref
          .read(settingsControllerProvider)
          .editor
          .quickOpenExcludedDirectories
          .toSet(),
    );
    if (searchRelevant) {
      _workspaceFiles.scheduleQuickOpenRefresh(
        workspacePath: widget.workspace.path,
        debounce: const Duration(milliseconds: 120),
      );
    }
    _pendingWatchedDirectories.addAll(batch.directoryRelativePaths);
    if (_pendingWatchedDirectories.isEmpty) {
      return;
    }
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
