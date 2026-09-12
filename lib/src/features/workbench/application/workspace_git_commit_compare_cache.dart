import 'package:alera/src/shared/infra/git/git_backend.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_exception.dart';

/// Caches commit comparisons only within the currently bound repository scope.
final class WorkspaceGitCommitCompareCache {
  factory WorkspaceGitCommitCompareCache({
    required GitBackend backend,
    required String scopePath,
  }) => WorkspaceGitCommitCompareCache._(backend, scopePath);

  WorkspaceGitCommitCompareCache._(this._backend, this._scopePath);

  final GitBackend _backend;
  final Map<String, GitCommitCompareResult> _results =
      <String, GitCommitCompareResult>{};
  final Map<String, Future<GitCommitCompareResult>> _inFlight =
      <String, Future<GitCommitCompareResult>>{};

  String _scopePath;
  int _generation = 0;

  String get scopePath => _scopePath;

  Future<GitCommitCompareResult> compareFor(String commitId) async {
    final cached = _results[commitId];
    if (cached != null) {
      return cached;
    }
    final current = _inFlight[commitId];
    if (current != null) {
      return current;
    }

    final requestPath = _scopePath;
    final requestGeneration = _generation;
    final future = _backend.commitCompare(
      path: requestPath,
      commitId: commitId,
    );
    _inFlight[commitId] = future;
    try {
      final result = await future;
      if (result.summary.status != GitCommitCompareStatus.ready) {
        throw GitInternalException(
          result.summary.errorMessage ?? 'Failed to load commit diff.',
        );
      }
      if (_isCurrent(commitId, future, requestPath, requestGeneration)) {
        _results[commitId] = result;
      }
      return result;
    } finally {
      if (_isCurrent(commitId, future, requestPath, requestGeneration)) {
        _inFlight.remove(commitId);
      }
    }
  }

  void clear() {
    _generation += 1;
    _results.clear();
    _inFlight.clear();
  }

  void rebind(String newPath) {
    if (_scopePath == newPath) {
      return;
    }
    _scopePath = newPath;
    clear();
  }

  bool _isCurrent(
    String commitId,
    Future<GitCommitCompareResult> future,
    String requestPath,
    int requestGeneration,
  ) {
    return _scopePath == requestPath &&
        _generation == requestGeneration &&
        identical(_inFlight[commitId], future);
  }
}
