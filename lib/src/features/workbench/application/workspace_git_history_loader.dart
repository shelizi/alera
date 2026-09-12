import 'dart:async';

import 'package:alera/src/shared/infra/git/git_backend.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';

/// Owns the repository history request and its scope-sensitive state.
final class WorkspaceGitHistoryLoader {
  factory WorkspaceGitHistoryLoader({
    required GitBackend backend,
    required String scopePath,
    required void Function() onChanged,
    int limit = 50,
  }) => WorkspaceGitHistoryLoader._(backend, scopePath, onChanged, limit);

  WorkspaceGitHistoryLoader._(
    this._backend,
    this._scopePath,
    this._onChanged,
    this.limit,
  );

  final GitBackend _backend;
  final void Function() _onChanged;
  final int limit;

  String _scopePath;
  GitHistoryResult? _result;
  Object? _error;
  Future<GitHistoryResult>? _inFlight;
  bool _refreshing = false;
  bool _dirty = false;
  int _generation = 0;
  bool _detached = false;

  String get scopePath => _scopePath;

  GitHistoryResult? get result => _result;

  Object? get error => _error;

  Future<GitHistoryResult>? get inFlight => _inFlight;

  bool get loading => _inFlight != null && _result == null;

  bool get refreshing => _refreshing;

  bool get dirty => _dirty;

  bool get needsLoad => _inFlight == null && (_dirty || _result == null);

  /// Loads history once for the current scope, coalescing overlapping calls.
  Future<GitHistoryResult> load() {
    if (_detached) {
      return Future<GitHistoryResult>.error(
        StateError('WorkspaceGitHistoryLoader is detached.'),
      );
    }
    final current = _inFlight;
    if (current != null) {
      return current;
    }

    final requestPath = _scopePath;
    final requestGeneration = _generation;
    final future = _backend.history(requestPath, limit: limit);
    _inFlight = future;
    _error = null;
    _refreshing = _result != null;
    _notifyChanged();

    unawaited(
      future.then<void>(
        (next) {
          if (!_isCurrent(future, requestPath, requestGeneration)) {
            return;
          }
          _result = next;
          _error = null;
          _dirty = false;
          _inFlight = null;
          _refreshing = false;
          _notifyChanged();
        },
        onError: (Object error, StackTrace stackTrace) {
          if (!_isCurrent(future, requestPath, requestGeneration)) {
            return;
          }
          _error = error;
          _inFlight = null;
          _refreshing = false;
          _notifyChanged();
        },
      ),
    );
    return future;
  }

  /// Marks the current history stale without starting work.
  void markStale() {
    if (_detached) {
      return;
    }
    _generation += 1;
    _inFlight = null;
    _refreshing = false;
    _dirty = true;
    _notifyChanged();
  }

  /// Switches the loader to a repository and drops all state from the old one.
  void rebind(String newPath) {
    if (_detached || _scopePath == newPath) {
      return;
    }
    _scopePath = newPath;
    _generation += 1;
    _result = null;
    _error = null;
    _inFlight = null;
    _refreshing = false;
    _dirty = false;
    _notifyChanged();
  }

  /// Detaches late futures from the presentation callback.
  void detach() {
    if (_detached) {
      return;
    }
    _detached = true;
    _generation += 1;
    _inFlight = null;
    _refreshing = false;
  }

  void dispose() => detach();

  bool _isCurrent(
    Future<GitHistoryResult> future,
    String requestPath,
    int requestGeneration,
  ) {
    return !_detached &&
        identical(_inFlight, future) &&
        _scopePath == requestPath &&
        _generation == requestGeneration;
  }

  void _notifyChanged() {
    if (!_detached) {
      _onChanged();
    }
  }
}
