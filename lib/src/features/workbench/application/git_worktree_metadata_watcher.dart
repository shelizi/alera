import 'dart:async';
import 'dart:io';

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
    Duration pollInterval = const Duration(seconds: 60),
    GitMetadataWatchFactory? watchDirectory,
  }) : _onRefresh = onRefresh,
       _debounce = debounce,
       _pollInterval = pollInterval,
       _watchDirectory = watchDirectory ?? _defaultGitMetadataWatch;

  final String repoPath;
  final Future<void> Function() _onRefresh;
  final Duration _debounce;
  final Duration _pollInterval;
  final GitMetadataWatchFactory _watchDirectory;

  StreamSubscription<String>? _commonGitDirSub;
  StreamSubscription<String>? _worktreesSub;
  Timer? _debounceTimer;
  Timer? _pollTimer;
  Future<void>? _refreshFuture;
  String? _commonGitDir;
  bool _started = false;
  bool _disposed = false;
  bool _refreshRunning = false;
  bool _refreshAgain = false;
  int _refreshSuspendCount = 0;
  bool _refreshQueuedWhileSuspended = false;

  void start() {
    if (_started || _disposed) {
      return;
    }
    _started = true;
    _ensureMetadataWatches();
    if (_pollInterval.inMicroseconds > 0) {
      _pollTimer = Timer.periodic(_pollInterval, (_) {
        _ensureMetadataWatches();
        _scheduleRefresh();
      });
    }
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
    _debounceTimer?.cancel();
    _debounceTimer = null;
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

    final refreshFuture = _refreshFuture;
    if (refreshFuture != null) {
      await refreshFuture;
    }
  }

  void _ensureMetadataWatches() {
    if (_disposed) {
      return;
    }
    final resolvedCommonDir = resolveGitCommonDir(repoPath);
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

    if (_commonGitDirSub == null && Directory(resolvedCommonDir).existsSync()) {
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
    _ensureWorktreesWatch();
  }

  void _ensureWorktreesWatch() {
    final worktreesPath = _worktreesPath;
    if (_disposed || worktreesPath == null) {
      return;
    }
    if (!Directory(worktreesPath).existsSync()) {
      _resetWorktreesWatch();
      return;
    }
    if (_worktreesSub != null) {
      return;
    }
    try {
      _worktreesSub = _watchDirectory(worktreesPath, recursive: true).listen(
        (_) => _scheduleRefresh(),
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
    if (worktreesPath == null || !_isWithinOrEqual(worktreesPath, eventPath)) {
      return;
    }
    _resetWorktreesWatch();
    _ensureWorktreesWatch();
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
    if (_debounce.inMicroseconds <= 0) {
      _debounceTimer = null;
      _queueRefresh();
      return;
    }
    _debounceTimer = Timer(_debounce, () {
      _debounceTimer = null;
      _queueRefresh();
    });
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
    _refreshFuture = _runRefreshLoop();
  }

  Future<void> _runRefreshLoop() async {
    try {
      do {
        _refreshAgain = false;
        try {
          await _onRefresh();
        } catch (_) {
          // Automatic refresh is best-effort. Manual refresh still surfaces
          // errors through WorkbenchController.reconcileProjectWorkspaces.
        }
      } while (_refreshAgain && !_disposed && _refreshSuspendCount == 0);
    } finally {
      _refreshRunning = false;
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
String? resolveGitCommonDir(String repoPath) {
  try {
    final dotGitPath = p.join(repoPath, '.git');
    final dotGitDirectory = Directory(dotGitPath);
    if (dotGitDirectory.existsSync()) {
      return _absoluteNormalized(dotGitDirectory.path);
    }

    final dotGitFile = File(dotGitPath);
    if (!dotGitFile.existsSync()) {
      return null;
    }
    final match = RegExp(
      r'^gitdir:\s*(.+)$',
      caseSensitive: false,
      multiLine: true,
    ).firstMatch(dotGitFile.readAsStringSync());
    final rawGitDir = match?.group(1)?.trim();
    if (rawGitDir == null || rawGitDir.isEmpty) {
      return null;
    }
    final gitDir = _absoluteNormalized(
      p.isAbsolute(rawGitDir) ? rawGitDir : p.join(repoPath, rawGitDir),
    );

    final commonDirFile = File(p.join(gitDir, 'commondir'));
    if (!commonDirFile.existsSync()) {
      return gitDir;
    }
    final rawCommonDir = commonDirFile.readAsStringSync().trim();
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
