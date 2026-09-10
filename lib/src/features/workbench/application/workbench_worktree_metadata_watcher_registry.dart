import 'dart:async';

import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/git_worktree_metadata_watcher.dart';
import 'package:path/path.dart' as p;

abstract interface class WorkbenchWorktreeMetadataWatcherHandle {
  String get repoPath;

  void start();

  Future<void> suspendRefresh();

  void resumeRefresh();

  Future<void> dispose();
}

typedef WorkbenchWorktreeMetadataWatcherFactory =
    WorkbenchWorktreeMetadataWatcherHandle Function({
      required String repoPath,
      required Future<void> Function() onRefresh,
    });

final class WorkbenchWorktreeMetadataWatcherRegistry {
  WorkbenchWorktreeMetadataWatcherRegistry({
    WorkbenchWorktreeMetadataWatcherFactory? factory,
  }) : _factory = factory ?? _createGitWatcher;

  final WorkbenchWorktreeMetadataWatcherFactory _factory;
  final Map<String, WorkbenchWorktreeMetadataWatcherHandle> _watchers =
      <String, WorkbenchWorktreeMetadataWatcherHandle>{};

  void sync(
    Project project, {
    required Future<void> Function(String projectId) onRefresh,
  }) {
    final existing = _watchers[project.id];
    if (!project.supportsLinkedWorkspaces) {
      _remove(project.id);
      return;
    }
    if (existing != null && p.equals(existing.repoPath, project.repoPath)) {
      return;
    }

    _remove(project.id);
    final watcher = _factory(
      repoPath: project.repoPath,
      onRefresh: () => onRefresh(project.id),
    );
    _watchers[project.id] = watcher;
    watcher.start();
  }

  Future<T> withRefreshSuspended<T>(
    String projectId,
    Future<T> Function() action,
  ) async {
    final watcher = _watchers[projectId];
    await watcher?.suspendRefresh();
    try {
      return await action();
    } finally {
      watcher?.resumeRefresh();
    }
  }

  void prune(Set<String> validProjectIds) {
    final removedProjectIds = _watchers.keys
        .where((projectId) => !validProjectIds.contains(projectId))
        .toList(growable: false);
    for (final projectId in removedProjectIds) {
      _remove(projectId);
    }
  }

  void disposeAll() {
    final watchers = _watchers.values.toList(growable: false);
    _watchers.clear();
    for (final watcher in watchers) {
      unawaited(watcher.dispose());
    }
  }

  void _remove(String projectId) {
    final watcher = _watchers.remove(projectId);
    if (watcher != null) {
      unawaited(watcher.dispose());
    }
  }
}

WorkbenchWorktreeMetadataWatcherHandle _createGitWatcher({
  required String repoPath,
  required Future<void> Function() onRefresh,
}) {
  return _GitWorktreeMetadataWatcherHandle(
    GitWorktreeMetadataWatcher(repoPath: repoPath, onRefresh: onRefresh),
  );
}

final class _GitWorktreeMetadataWatcherHandle
    implements WorkbenchWorktreeMetadataWatcherHandle {
  _GitWorktreeMetadataWatcherHandle(this._watcher);

  final GitWorktreeMetadataWatcher _watcher;

  @override
  String get repoPath => _watcher.repoPath;

  @override
  void start() => _watcher.start();

  @override
  Future<void> suspendRefresh() => _watcher.suspendRefresh();

  @override
  void resumeRefresh() => _watcher.resumeRefresh();

  @override
  Future<void> dispose() => _watcher.dispose();
}
