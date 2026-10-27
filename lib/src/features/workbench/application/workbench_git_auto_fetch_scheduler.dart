import 'dart:async';
import 'dart:io';

import 'package:alera/src/features/projects/domain/project.dart';
import 'package:path/path.dart' as p;

const Duration workbenchGitAutoFetchInterval = Duration(minutes: 5);
const Duration workbenchGitAutoFetchStartupDelay = Duration(seconds: 20);
const Duration _maxRetryDelay = Duration(minutes: 30);

/// A project may own several linked worktrees, but all of them share one Git
/// repository and its remote tracking refs. Fetch once via the project root.
List<String> gitAutoFetchRepositoryPaths(Iterable<Project> projects) {
  final unique = <String>{};
  final paths = <String>[];
  for (final project in projects) {
    if (!project.isGitRepository || project.repoPath.trim().isEmpty) continue;
    final path = p.normalize(project.repoPath.trim());
    final key = Platform.isWindows ? path.toLowerCase() : path;
    if (unique.add(key)) paths.add(path);
  }
  return paths;
}

/// Serial background remote-ref refresh. One failed project does not prevent
/// other projects from fetching. The Git subprocess is non-interactive.
final class GitAutoFetchScheduler {
  GitAutoFetchScheduler({
    required Future<void> Function(String path) fetch,
    required void Function(String path) onFetched,
    required void Function(String path, Object error, StackTrace stackTrace)
    onError,
    Duration interval = workbenchGitAutoFetchInterval,
    Duration startupDelay = workbenchGitAutoFetchStartupDelay,
    Timer Function(Duration duration, void Function() callback)? createTimer,
  }) : _fetch = fetch,
       _onFetched = onFetched,
       _onError = onError,
       _interval = interval,
       _startupDelay = startupDelay,
       _createTimer = createTimer ?? Timer.new;

  final Future<void> Function(String path) _fetch;
  final void Function(String path) _onFetched;
  final void Function(String path, Object error, StackTrace stackTrace)
  _onError;
  Duration _interval;
  final Duration _startupDelay;
  final Timer Function(Duration duration, void Function() callback)
  _createTimer;

  List<String> _paths = const <String>[];
  Timer? _timer;
  bool _enabled = false;
  bool _running = false;
  bool _disposed = false;
  bool _changedDuringRun = false;
  Duration _changeDelay = workbenchGitAutoFetchStartupDelay;
  int _failedRounds = 0;

  void configure({
    required bool enabled,
    required List<String> paths,
    Duration? interval,
  }) {
    if (_disposed) return;
    final nextInterval = interval ?? _interval;
    if (nextInterval <= Duration.zero) {
      throw ArgumentError.value(nextInterval, 'interval', 'Must be positive');
    }
    final nextEnabled = enabled && paths.isNotEmpty;
    final sameTargets = _samePaths(_paths, paths);
    final intervalChanged = nextInterval != _interval;
    if (_enabled == nextEnabled && sameTargets && !intervalChanged) return;
    // Frequency-only changes re-arm the periodic timer at the selected
    // interval without triggering another startup-delay fetch.
    final nextDelay = _enabled == nextEnabled && sameTargets && intervalChanged
        ? nextInterval
        : _startupDelay;
    _enabled = nextEnabled;
    _paths = List<String>.unmodifiable(paths);
    _interval = nextInterval;
    _timer?.cancel();
    _timer = null;
    _failedRounds = 0;
    if (_running) {
      _changedDuringRun = true;
      _changeDelay = nextDelay;
    } else {
      _changedDuringRun = false;
      if (_enabled) _schedule(nextDelay);
    }
  }

  /// Testable manual trigger. Concurrent invocations cannot overlap fetches.
  Future<void> fetchNow() async {
    if (_disposed || !_enabled || _running) return;
    _timer?.cancel();
    _timer = null;
    _running = true;
    var failed = false;
    final paths = List<String>.from(_paths);
    try {
      for (final path in paths) {
        if (_disposed || !_enabled) break;
        if (!_paths.contains(path)) continue;
        try {
          await _fetch(path);
          if (!_disposed) _onFetched(path);
        } catch (error, stackTrace) {
          failed = true;
          if (!_disposed) _onError(path, error, stackTrace);
        }
      }
    } finally {
      _running = false;
      if (!_disposed && _enabled) {
        if (_changedDuringRun) {
          _changedDuringRun = false;
          _schedule(_changeDelay);
        } else {
          _failedRounds = failed ? _failedRounds + 1 : 0;
          // Offline/broken remotes must not hammer the network repeatedly.
          final multiplier = 1 << _failedRounds.clamp(0, 3).toInt();
          final delay = _interval * multiplier;
          // Avoid retrying more often than the user's selected interval.
          final maximumDelay = _interval > _maxRetryDelay
              ? _interval
              : _maxRetryDelay;
          _schedule(delay > maximumDelay ? maximumDelay : delay);
        }
      }
    }
  }

  void dispose() {
    _disposed = true;
    _enabled = false;
    _timer?.cancel();
    _timer = null;
  }

  void _schedule(Duration delay) {
    _timer?.cancel();
    _timer = _createTimer(delay, () => unawaited(fetchNow()));
  }

  static bool _samePaths(List<String> oldPaths, List<String> newPaths) {
    if (oldPaths.length != newPaths.length) return false;
    for (var i = 0; i < oldPaths.length; i++) {
      if (oldPaths[i] != newPaths[i]) return false;
    }
    return true;
  }
}
