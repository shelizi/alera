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
      changedRelativePaths.any((path) {
        if (path.isEmpty) return true;
        final parts = path
            .split('/')
            .map((part) => part.toLowerCase())
            .toList();
        for (var index = 0; index < parts.length; index++) {
          final part = parts[index];
          if (part == '.git') {
            // Ignore object/ref churn, but preserve changes to ignore
            // configuration, worktree boundaries and a replaced .git file.
            if (index + 1 == parts.length) return true;
            return const {
              'info',
              'config',
              'config.worktree',
              'worktrees',
            }.contains(parts[index + 1]);
          }
          if (excluded.contains(part)) return false;
        }
        return true;
      });
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
    _watchQuickOpenRefreshQueue.enqueue(const <String>[
      'full',
    ], immediate: true);
    _watchRefreshQueue.enqueue(const <String>[''], immediate: true);
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
      // An in-flight reconciliation may have been invalidated when the
      // watcher generation changed. Always reconcile once after reattach.
      if (mounted && _watcherHandle != null) {
        _watchRefreshQueue.enqueue(const <String>[''], immediate: true);
        _watchQuickOpenRefreshQueue.enqueue(const <String>[
          'full',
        ], immediate: true);
      }
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
    if (discardPending) {
      _watchRefreshQueue.clear();
      _watchQuickOpenRefreshQueue.clear();
      _watchDeferredQuickOpenQueue.clear();
    }
    _lastSyncedWatchedDirectories = null;
    // Cancellation may stall if a platform event stream never acknowledges
    // teardown. Generation guards already reject the old stream: do not keep
    // the next Workspace behind a permanently animated loading indicator.
    if (subscription != null) {
      try {
        await subscription.cancel().timeout(const Duration(milliseconds: 300));
      } catch (_) {
        // Best-effort stream cleanup continues after the bounded wait.
      }
    }
    if (handle != null) {
      try {
        await _workspaceFiles
            .stopExplorerWatcher(handle: handle)
            .timeout(const Duration(milliseconds: 500));
      } catch (_) {
        // Native cleanup is best effort; a stale handle must not block UI.
      }
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
    final excluded = ref
        .read(settingsControllerProvider)
        .editor
        .quickOpenExcludedDirectories
        .toSet();
    final deferred = batch.deferredQuickOpenPaths?.toSet() ?? const <String>{};
    final prompt = batch.changedRelativePaths
        .where((path) => path.isEmpty || !deferred.contains(path))
        .toList(growable: false);
    final relevantPrompt =
        batch.changedRelativePaths.isEmpty ||
        (prompt.isNotEmpty &&
            workspaceExplorerWatcherTouchesSearch(
              changedRelativePaths: prompt,
              excludedDirectories: excluded,
            ));
    if (relevantPrompt) {
      _watchDeferredQuickOpenQueue.clear();
      _watchQuickOpenRefreshQueue.enqueue(const <String>['index']);
    } else if (batch.changedRelativePaths.any(
      (path) =>
          deferred.contains(path) &&
          workspaceExplorerWatcherTouchesSearch(
            changedRelativePaths: [path],
            excludedDirectories: excluded,
          ),
    )) {
      _watchDeferredQuickOpenQueue.enqueue(const <String>['index']);
    }
    _watchRefreshQueue.enqueue(batch.directoryRelativePaths);
  }

  Future<void> _scheduleWatchedQuickOpenRefresh(List<String> requests) async {
    if (!mounted) return;
    _watchDeferredQuickOpenQueue.clear();
    // The prompt/deferred queue has already bounded the wait. Do not
    // restart the backend's debounce on every watcher notification.
    _workspaceFiles.scheduleQuickOpenRefresh(
      workspacePath: widget.workspace.path,
      debounce: Duration.zero,
      allowIncremental: !requests.contains('full'),
    );
  }

  Future<void> _refreshWatchedDirectories(List<String> relativePaths) async {
    final generation = _watcherGeneration;
    final projectionGeneration = _nativeProjectionGeneration;
    final workspacePath = widget.workspace.path;
    await _refreshGitStatusSnapshot();
    if (!mounted ||
        generation != _watcherGeneration ||
        projectionGeneration != _nativeProjectionGeneration ||
        workspacePath != widget.workspace.path) {
      return;
    }
    for (final relativePath in relativePaths) {
      if (!mounted ||
          generation != _watcherGeneration ||
          projectionGeneration != _nativeProjectionGeneration ||
          workspacePath != widget.workspace.path) {
        return;
      }
      if (!_childrenByDirectory.containsKey(relativePath)) {
        continue;
      }
      await _refreshDirectory(
        relativePath,
        refreshGitStatus: false,
        refreshQuickOpen: false,
      );
    }
  }
}
