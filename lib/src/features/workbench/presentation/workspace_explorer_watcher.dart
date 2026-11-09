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
    if (!mounted || _watcherHandle != null) return;
    final generation = _watcherGeneration;
    final workspacePath = widget.workspace.path;
    try {
      final handle = await _workspaceFiles.startExplorerWatcher(
        workspacePath: workspacePath,
      );
      if (!mounted ||
          generation != _watcherGeneration ||
          widget.workspace.path != workspacePath) {
        await _workspaceFiles.stopExplorerWatcher(handle: handle);
        return;
      }
      _watcherHandle = handle;
      _watchSubscription = _workspaceFiles
          .watchExplorerEvents(handle: handle)
          .listen(
            (batch) {
              if (mounted && generation == _watcherGeneration) {
                _scheduleWatchedRefresh(batch);
              }
            },
            onError: (Object _) => _scheduleWatcherRecovery(generation),
            onDone: () => _scheduleWatcherRecovery(generation),
          );
      await _syncWatchedDirectories();
      if (generation == _watcherGeneration && _watcherRetryTimer == null) {
        _watcherFailureCount = 0;
      }
    } catch (_) {
      _scheduleWatcherRecovery(generation);
    }
  }

  /// OS watcher overflow, failed startup or a closed event stream may leave
  /// Explorer rows and Quick Open stale. Reconcile immediately, then reattach.
  void _scheduleWatcherRecovery(int generation) {
    if (!mounted ||
        generation != _watcherGeneration ||
        _watcherRetryTimer != null ||
        _watcherRecoveryInFlight) {
      return;
    }
    _workspaceFiles.scheduleQuickOpenRefresh(
      workspacePath: widget.workspace.path,
      debounce: const Duration(milliseconds: 120),
    );
    _pendingWatchedDirectories.add('');
    if (!_watchRefreshInFlight) {
      _watchRefreshInFlight = true;
      unawaited(_drainWatchedRefreshes());
    }
    final exponent = _watcherFailureCount.clamp(0, 4);
    _watcherFailureCount += 1;
    _watcherRetryTimer = Timer(
      Duration(milliseconds: 300 * (1 << exponent)),
      () {
        _watcherRetryTimer = null;
        if (mounted && generation == _watcherGeneration) {
          unawaited(_restartNativeWatcher(generation));
        }
      },
    );
  }

  Future<void> _restartNativeWatcher(int generation) async {
    if (!mounted ||
        generation != _watcherGeneration ||
        _watcherRecoveryInFlight) {
      return;
    }
    _watcherRecoveryInFlight = true;
    final workspacePath = widget.workspace.path;
    try {
      await _stopNativeWatcher(discardPending: false);
    } finally {
      _watcherRecoveryInFlight = false;
    }
    if (mounted && widget.workspace.path == workspacePath) {
      await _startNativeWatcher();
    }
  }

  Future<void> _stopNativeWatcher({bool discardPending = true}) async {
    _watcherGeneration += 1;
    _watcherRetryTimer?.cancel();
    _watcherRetryTimer = null;
    final subscription = _watchSubscription;
    final handle = _watcherHandle;
    _watchSubscription = null;
    _watcherHandle = null;
    if (discardPending) _pendingWatchedDirectories.clear();
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
      _scheduleWatcherRecovery(_watcherGeneration);
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
