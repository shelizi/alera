import 'dart:async';
import 'dart:io';

import 'package:alera/src/features/workbench/application/workbench_background_task_pool.dart';
import 'package:path/path.dart' as p;

typedef GitMetadataWatchFactory = Stream<String> Function(
  String path, {
  required bool recursive,
});

/// Watches Git's shared worktree metadata and requests authoritative refreshes.
///
/// Filesystem events are only hints. Callers still reconcile through GitBackend
/// so Alera never treats watcher payloads as source-of-truth repository state.
final class GitWorktreeMetadataWatcher {
  GitWorktreeMetadataWatcher({
    required this.repoPath,
    required Future<void> Function() onRefresh,
    Duration debounce = const Duration(milliseconds: 300),
    Duration maxRefreshDelay = const Duration(seconds: 1),
    WorkbenchBackgroundTaskPool? refreshPool,
    WorkbenchBackgroundTaskPool? metadataProbePool,
    Duration pollInterval = const Duration(seconds: 60),
    GitMetadataWatchFactory? watchDirectory,
    Future<String?> Function(String)? resolveCommonDir,
    Future<bool> Function(String)? directoryExists,
  }) : _onRefresh = onRefresh,
       _debounce = debounce,
       _maxRefreshDelay = maxRefreshDelay,
       _refreshPool =
           refreshPool ?? WorkbenchBackgroundTaskPool(maxConcurrent: 1),
       _metadataProbePool =
           metadataProbePool ?? WorkbenchBackgroundTaskPool(maxConcurrent: 1),
       _pollInterval = pollInterval,
       _watchDirectory = watchDirectory ?? _defaultGitMetadataWatch,
       _resolveCommonDir = resolveCommonDir ?? resolveGitCommonDir,
       _directoryExists = directoryExists ?? _defaultDirectoryExists;

  final String repoPath;
  final Future<void> Function() _onRefresh;
  final Duration _debounce;
  final Duration _maxRefreshDelay;
  final WorkbenchBackgroundTaskPool _refreshPool;
  final WorkbenchBackgroundTaskPool _metadataProbePool;
  final Object _refreshOwner = Object();
  final Object _metadataProbeOwner = Object();
  final Duration _pollInterval;
  final GitMetadataWatchFactory _watchDirectory;
  final Future<String?> Function(String) _resolveCommonDir;
  final Future<bool> Function(String) _directoryExists;

  StreamSubscription<String>? _commonGitDirSub;
  StreamSubscription<String>? _worktreesSub;
  Timer? _debounceTimer;
  Timer? _maxRefreshTimer;
  Timer? _pollTimer;
  Future<void>? _refreshFuture;
  Future<void>? _metadataProbeFuture;
  bool _metadataProbeAgain = false;
  bool _resetWorktreesOnProbe = false;
  String? _commonGitDir;
  bool _started = false;
  bool _disposed = false;
  bool _refreshRunning = false;
  bool _refreshAgain = false;
  int _refreshSuspendCount = 0;
  bool _refreshQueuedWhileSuspended = false;

  Future<void> start() {
    if (_started || _disposed) {
      return _metadataProbeFuture ?? Future<void>.value();
    }
    _started = true;
    final initialProbe = _queueMetadataProbe();
    if (_pollInterval.inMicroseconds > 0) {
      _pollTimer = Timer.periodic(_pollInterval, (_) {
        unawaited(_queueMetadataProbe());
        // A slow filesystem probe must not delay the authoritative poll.
        _scheduleRefresh();
      });
    }
    return initialProbe;
  }

  /// Defers watcher-driven refreshes while Alera itself mutates worktrees.
  ///
  /// Events are retained as one pending refresh and replayed after resume, so
  /// internal create/remove operations cannot race reconciliation before their
  /// workspace repository writes complete.
  Future<void> suspendRefresh() async {
    if (_disposed) {
      return;
    }
    _refreshSuspendCount += 1;
    if (_debounceTimer != null || _maxRefreshTimer != null) {
      _cancelRefreshTimers();
      _refreshQueuedWhileSuspended = true;
    }
    // A local mutation must not wait behind unrelated projects. Cancel a
    // queued pass, retain its dirty flag, and only wait for active local work.
    if (_refreshPool.cancelPending(_refreshOwner)) {
      _refreshQueuedWhileSuspended = true;
    }
    final refreshFuture = _refreshRunning ? _refreshFuture : null;
    if (refreshFuture != null) {
      await refreshFuture;
    }
  }

  void resumeRefresh() {
    if (_disposed || _refreshSuspendCount == 0) {
      return;
    }
    _refreshSuspendCount -= 1;
    if (_refreshSuspendCount == 0 && _refreshQueuedWhileSuspended) {
      _refreshQueuedWhileSuspended = false;
      _scheduleRefresh();
    }
  }

  Future<void> dispose() async {
    if (_disposed) {
      return;
    }
    _disposed = true;
    _cancelRefreshTimers();
    _refreshPool.cancelPending(_refreshOwner);
    _metadataProbePool.cancelPending(_metadataProbeOwner);
    _pollTimer?.cancel();
    _pollTimer = null;

    final subscriptions = <StreamSubscription<String>>[
      if (_commonGitDirSub != null) _commonGitDirSub!,
      if (_worktreesSub != null) _worktreesSub!,
    ];
    _commonGitDirSub = null;
    _worktreesSub = null;
    await Future.wait<void>(
      subscriptions.map((subscription) => subscription.cancel()),
    );

    final metadataProbe = _metadataProbeFuture;
    if (metadataProbe != null) {
      await metadataProbe;
    }
    final refreshFuture = _refreshFuture;
    if (refreshFuture != null) {
      await refreshFuture;
    }
  }

  /// Serialize filesystem probes. A burst during an in-flight probe retains
  /// one trailing pass, including any request to reattach the worktrees watch.
  Future<void> _queueMetadataProbe({bool resetWorktrees = false}) {
    if (_disposed) {
      return Future<void>.value();
    }
    _resetWorktreesOnProbe |= resetWorktrees;
    final running = _metadataProbeFuture;
    if (running != null) {
      _metadataProbeAgain = true;
      return running;
    }
    final future = Future<void>.microtask(_runMetadataProbeLoop);
    _metadataProbeFuture = future;
    return future;
  }

  Future<void> _runMetadataProbeLoop() async {
    try {
      do {
        if (_disposed) break;
        try {
          await _metadataProbePool.run(_metadataProbeOwner, () async {
            if (_disposed) return;
            // Requests arriving before this queued pass begins are covered
            // by it; only events during the actual I/O require a trailing pass.
            _metadataProbeAgain = false;
            final resetWorktrees = _resetWorktreesOnProbe;
            _resetWorktreesOnProbe = false;
            await _ensureMetadataWatches(resetWorktrees: resetWorktrees);
          });
        } catch (_) {
          // Metadata watching is best-effort. The next poll retries.
        }
      } while (_metadataProbeAgain && !_disposed);
    } finally {
      _metadataProbeFuture = null;
    }
  }

  Future<void> _ensureMetadataWatches({required bool resetWorktrees}) async {
    final resolvedCommonDir = await _resolveCommonDir(repoPath);
    if (_disposed) {
      return;
    }
    if (resolvedCommonDir == null) {
      _resetCommonGitDirWatch();
      _resetWorktreesWatch();
      _commonGitDir = null;
      return;
    }

    final commonDirChanged =
        _commonGitDir != null && !p.equals(_commonGitDir!, resolvedCommonDir);
    if (commonDirChanged) {
      _resetCommonGitDirWatch();
      _resetWorktreesWatch();
    }
    _commonGitDir = resolvedCommonDir;

    if (_commonGitDirSub == null && await _directoryExists(resolvedCommonDir)) {
      if (_disposed) {
        return;
      }
      try {
        _commonGitDirSub = _watchDirectory(resolvedCommonDir, recursive: false)
            .listen(
              _handleCommonGitDirEvent,
              onError: (Object _) => _commonGitDirSub = null,
              onDone: () => _commonGitDirSub = null,
              cancelOnError: true,
            );
      } catch (_) {
        _commonGitDirSub = null;
      }
    }
    if (_disposed) {
      return;
    }
    if (resetWorktrees) {
      _resetWorktreesWatch();
    }
    await _ensureWorktreesWatch();
  }

  Future<void> _ensureWorktreesWatch() async {
    final worktreesPath = _worktreesPath;
    if (_disposed || worktreesPath == null) {
      return;
    }
    final exists = await _directoryExists(worktreesPath);
    if (_disposed) {
      return;
    }
    if (!exists) {
      _resetWorktreesWatch();
      return;
    }
    if (_worktreesSub != null) {
      return;
    }
    try {
      _worktreesSub = _watchDirectory(worktreesPath, recursive: true).listen(
        (eventPath) {
          // Index writes do not change the linked-worktree catalog. Keep
          // structural events and the authoritative fallback poll.
          if (_isWorktreeIndexEvent(worktreesPath, eventPath)) return;
          _scheduleRefresh();
        },
        onError: (Object _) => _worktreesSub = null,
        onDone: () => _worktreesSub = null,
        cancelOnError: true,
      );
    } catch (_) {
      _worktreesSub = null;
    }
  }

  void _handleCommonGitDirEvent(String eventPath) {
    final worktreesPath = _worktreesPath;
    if (_disposed ||
        worktreesPath == null ||
        !_isWithinOrEqual(worktreesPath, eventPath) ||
        _isWorktreeIndexEvent(worktreesPath, eventPath)) {
      return;
    }
    unawaited(_queueMetadataProbe(resetWorktrees: true));
    _scheduleRefresh();
  }

  String? get _worktreesPath {
    final commonGitDir = _commonGitDir;
    return commonGitDir == null ? null : p.join(commonGitDir, 'worktrees');
  }

  void _scheduleRefresh() {
    if (_disposed) {
      return;
    }
    if (_refreshSuspendCount > 0) {
      _refreshQueuedWhileSuspended = true;
      return;
    }
    _debounceTimer?.cancel();
    if (_debounce.inMicroseconds <= 0 || _maxRefreshDelay.inMicroseconds <= 0) {
      _flushScheduledRefresh();
      return;
    }
    _debounceTimer = Timer(_debounce, _flushScheduledRefresh);
    // Continuous metadata notifications cannot postpone reconciliation
    // forever. This deadline is anchored at the first event in the burst.
    _maxRefreshTimer ??= Timer(_maxRefreshDelay, _flushScheduledRefresh);
  }

  void _cancelRefreshTimers() {
    _debounceTimer?.cancel();
    _debounceTimer = null;
    _maxRefreshTimer?.cancel();
    _maxRefreshTimer = null;
  }

  void _flushScheduledRefresh() {
    _cancelRefreshTimers();
    _queueRefresh();
  }

  void _queueRefresh() {
    if (_disposed) {
      return;
    }
    if (_refreshSuspendCount > 0) {
      _refreshQueuedWhileSuspended = true;
      return;
    }
    if (_refreshRunning) {
      _refreshAgain = true;
      return;
    }
    _refreshRunning = true;
    _refreshFuture = Future<void>.microtask(_runRefreshLoop);
  }

  Future<void> _runRefreshLoop() async {
    try {
      do {
        _refreshAgain = false;
        if (_disposed) break;
        if (_refreshSuspendCount > 0) {
          _refreshQueuedWhileSuspended = true;
          break;
        }
        try {
          // Release the pool slot after each pass, including a trailing
          // pass. Other projects keep their FIFO position during bursts.
          await _refreshPool.run(_refreshOwner, () async {
            if (_disposed) return;
            if (_refreshSuspendCount > 0) {
              _refreshQueuedWhileSuspended = true;
              return;
            }
            // An authoritative pass also covers all requests accumulated
            // while it was waiting for a slot.
            _refreshAgain = false;
            await _onRefresh();
          });
        } catch (_) {
          // Automatic refresh is best-effort. Manual refresh still surfaces
          // errors through WorkbenchController.reconcileProjectWorkspaces.
        }
      } while (_refreshAgain && !_disposed && _refreshSuspendCount == 0);
    } finally {
      _refreshRunning = false;
      _refreshFuture = null;
      if (_refreshAgain && !_disposed) {
        _refreshAgain = false;
        _queueRefresh();
      }
    }
  }

  void _resetCommonGitDirWatch() {
    final subscription = _commonGitDirSub;
    _commonGitDirSub = null;
    if (subscription != null) {
      unawaited(subscription.cancel());
    }
  }

  void _resetWorktreesWatch() {
    final subscription = _worktreesSub;
    _worktreesSub = null;
    if (subscription != null) {
      unawaited(subscription.cancel());
    }
  }
}

/// Resolves the shared Git directory for both main and linked worktrees.
Future<String?> resolveGitCommonDir(String repoPath) async {
  try {
    final dotGitPath = p.join(repoPath, '.git');
    final type = await FileSystemEntity.type(dotGitPath);
    if (type == FileSystemEntityType.directory) {
      return _absoluteNormalized(dotGitPath);
    }
    if (type != FileSystemEntityType.file) {
      return null;
    }
    final match = _gitDirPattern.firstMatch(
      await File(dotGitPath).readAsString(),
    );
    final rawGitDir = match?.group(1)?.trim();
    if (rawGitDir == null || rawGitDir.isEmpty) {
      return null;
    }
    final gitDir = _absoluteNormalized(
      p.isAbsolute(rawGitDir) ? rawGitDir : p.join(repoPath, rawGitDir),
    );

    final commonDirFile = File(p.join(gitDir, 'commondir'));
    if (!await commonDirFile.exists()) {
      return gitDir;
    }
    final rawCommonDir = (await commonDirFile.readAsString()).trim();
    if (rawCommonDir.isEmpty) {
      return gitDir;
    }
    return _absoluteNormalized(
      p.isAbsolute(rawCommonDir) ? rawCommonDir : p.join(gitDir, rawCommonDir),
    );
  } catch (_) {
    return null;
  }
}

final _gitDirPattern = RegExp(
  r'^gitdir:\s*(.+)$',
  caseSensitive: false,
  multiLine: true,
);

Future<bool> _defaultDirectoryExists(String path) => Directory(path).exists();

Stream<String> _defaultGitMetadataWatch(
  String path, {
  required bool recursive,
}) => Directory(path).watch(recursive: recursive).map((event) => event.path);

String _absoluteNormalized(String path) => p.normalize(p.absolute(path));

bool _isWithinOrEqual(String parentPath, String candidatePath) {
  final parent = _absoluteNormalized(parentPath);
  final candidate = _absoluteNormalized(candidatePath);
  return p.equals(parent, candidate) || p.isWithin(parent, candidate);
}

bool _isWorktreeIndexEvent(String worktreesPath, String eventPath) {
  final event = _absoluteNormalized(eventPath);
  // Do not ignore a worktree directory literally named "index", or any
  // unknown nested path. Only its direct administrative index files.
  if (!p.equals(p.dirname(p.dirname(event)), worktreesPath)) return false;
  final name = p.basename(event);
  return name == 'index' || name == 'index.lock';
}
