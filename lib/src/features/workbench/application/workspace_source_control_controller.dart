import 'dart:async';

import 'package:alera/src/features/workbench/application/source_control_watcher.dart';
import 'package:alera/src/rust/api/workspace_files.dart' as native;
import 'package:alera/src/shared/infra/git/git_backend.dart';
import 'package:alera/src/shared/infra/git/git_commit_ops_models.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_exception.dart';
import 'package:alera/src/shared/infra/git/git_providers.dart';
import 'package:riverpod_annotation/riverpod_annotation.dart';

part 'workspace_source_control_controller.g.dart';

class const WorkspaceSourceControlState({
  required final GitStatusResult status,
  required final GitRepositoryState repositoryState,
  required final List<GitStashEntry> stashes,
  final WorkspaceSourceControlAction? action,
}) {
  bool get isBusy => action != null;

  List<GitChangeEntry> get stagedEntries => status.entries
      .where((entry) => entry.area == GitChangeArea.staged)
      .toList(growable: false);

  bool get hasStagedChanges => stagedEntries.isNotEmpty;

  bool get hasStageableChanges =>
      status.entries.any((entry) => entry.canStageFromParent);

  bool get hasDiscardableChanges =>
      status.entries.any((entry) => entry.canDiscardFromParent);

  bool get hasChanges => status.entries.isNotEmpty;

  bool get hasUnstagedOrUntrackedChanges => status.entries.any(
    (entry) =>
        entry.area == GitChangeArea.unstaged ||
        entry.area == GitChangeArea.untracked,
  );

  bool get hasStashableChanges => status.entries.any(
    (entry) =>
        entry.area == GitChangeArea.staged ||
        (entry.area == GitChangeArea.unstaged &&
            !entry.isSubmoduleWorktreeOnly),
  );
}

enum WorkspaceSourceControlAction {
  refresh,
  stage,
  unstage,
  discard,
  commit,
  commitPush,
  commitSync,
  amend,
  fetch,
  pull,
  push,
  sync,
  stash,
  stashPop,
  revert,
  reset,
  checkout,
}

@riverpod
class WorkspaceSourceControlController
    extends _$WorkspaceSourceControlController {
  static const Duration _watcherReloadDebounce = Duration(milliseconds: 250);

  SourceControlWatcher? _watcher;
  StreamSubscription<native.SourceControlWatchSignal>? _watchSubscription;
  native.SourceControlWatcherHandle? _watcherHandle;
  Timer? _watcherReloadDebounceTimer;
  DateTime? _watcherReloadNotBefore;
  bool _watcherReloadInFlight = false;
  bool _watcherReloadQueued = false;
  int _queuedWatcherReloads = 0;
  bool _disposed = false;

  @override
  Future<WorkspaceSourceControlState> build(String workspacePath) {
    // Capture the watcher now: `onDispose` runs in a lifecycle where reading
    // providers via `ref` is disallowed.
    _watcher = ref.read(sourceControlWatcherProvider);
    ref.onDispose(_stopWatching);
    // Best-effort: file watching keeps the panel live without blocking the
    // initial load; manual refresh remains available if it fails to start.
    unawaited(_startWatching());
    return _load();
  }

  Future<void> refresh() => _run(.refresh, (_) async {});

  Future<void> stage(String? filePath) => _run(
    .stage,
    (backend) => backend.stage(path: workspacePath, filePath: filePath),
  );

  Future<void> stageArea(GitChangeArea area, {String? filePath}) => _run(
    .stage,
    (backend) =>
        backend.stageArea(path: workspacePath, area: area, filePath: filePath),
  );

  Future<void> stageEntry(GitChangeEntry entry) {
    if (!entry.canStageFromParent) {
      return Future<void>.value();
    }
    return _run(.stage, (backend) async {
      for (final filePath in _actionPaths(entry)) {
        await backend.stage(path: workspacePath, filePath: filePath);
      }
    });
  }

  Future<void> unstage(String? filePath) => _run(
    .unstage,
    (backend) => backend.unstage(path: workspacePath, filePath: filePath),
  );

  Future<void> unstageArea(GitChangeArea area, {String? filePath}) => _run(
    .unstage,
    (backend) => backend.unstageArea(
      path: workspacePath,
      area: area,
      filePath: filePath,
    ),
  );

  Future<void> unstageEntry(GitChangeEntry entry) {
    if (!entry.canUnstageFromParent) {
      return Future<void>.value();
    }
    return _run(.unstage, (backend) async {
      for (final filePath in _actionPaths(entry)) {
        await backend.unstage(path: workspacePath, filePath: filePath);
      }
    });
  }

  Future<void> discard(String? filePath) => _run(
    .discard,
    (backend) => backend.discard(path: workspacePath, filePath: filePath),
  );

  Future<void> discardArea(GitChangeArea area, {String? filePath}) => _run(
    .discard,
    (backend) => backend.discardArea(
      path: workspacePath,
      area: area,
      filePath: filePath,
    ),
  );

  Future<void> discardEntry(GitChangeEntry entry) {
    if (!entry.canDiscardFromParent) {
      return Future<void>.value();
    }
    return _run(.discard, (backend) async {
      for (final filePath in _actionPaths(entry)) {
        await backend.discard(path: workspacePath, filePath: filePath);
      }
    });
  }

  Future<void> commit(String message) => _run(
    .commit,
    (backend) => backend.commit(path: workspacePath, message: message.trim()),
  );

  Future<void> commitAndPush(String message) =>
      _run(.commitPush, (backend) async {
        await backend.commit(path: workspacePath, message: message.trim());
        await backend.push(workspacePath);
      });

  Future<void> commitAndSync(String message) =>
      _run(.commitSync, (backend) async {
        await backend.commit(path: workspacePath, message: message.trim());
        if (!(state.asData?.value.repositoryState.hasUpstream ?? false)) {
          throw const NoUpstreamException('set an upstream before syncing');
        }
        await backend.pull(workspacePath);
        await backend.push(workspacePath);
      });

  Future<void> amendCommit(String message) => _run(
    .amend,
    (backend) =>
        backend.amendCommit(path: workspacePath, message: message.trim()),
  );

  Future<void> checkoutCommit(String commitId) => _run(
    .checkout,
    (backend) =>
        backend.checkoutCommit(path: workspacePath, commitId: commitId),
  );

  Future<void> revertCommit(String commitId, {int? mainlineParent}) =>
      _run(.revert, (backend) async {
        await backend.revertCommit(
          path: workspacePath,
          commitId: commitId,
          mainlineParent: mainlineParent,
        );
      });

  Future<void> resetToCommit(String commitId, {required GitResetMode mode}) =>
      _run(
        .reset,
        (backend) => backend.resetToCommit(
          path: workspacePath,
          commitId: commitId,
          mode: mode,
        ),
      );

  Future<void> fetch() =>
      _run(.fetch, (backend) => backend.fetch(workspacePath));

  Future<void> pull() => _run(.pull, (backend) => backend.pull(workspacePath));

  Future<void> push() => _run(.push, (backend) => backend.push(workspacePath));

  Future<void> sync() => _run(.sync, (backend) async {
    if (!(state.asData?.value.repositoryState.hasUpstream ?? false)) {
      throw const NoUpstreamException('set an upstream before syncing');
    }
    await backend.pull(workspacePath);
    await backend.push(workspacePath);
  });

  Future<void> stash() =>
      _run(.stash, (backend) => backend.stash(workspacePath));

  Future<void> stashPop(int stashIndex) => _run(
    .stashPop,
    (backend) => backend.stashPop(path: workspacePath, stashIndex: stashIndex),
  );

  Future<WorkspaceSourceControlState> _load() async {
    final backend = ref.read(gitBackendProvider);
    final results = await Future.wait<Object>([
      backend.status(workspacePath),
      backend.repositoryState(workspacePath),
      backend.listStashes(workspacePath),
    ]);
    return _reconcileLoadedState(
      previous: state.asData?.value,
      status: results[0] as GitStatusResult,
      repositoryState: results[1] as GitRepositoryState,
      stashes: results[2] as List<GitStashEntry>,
    );
  }

  WorkspaceSourceControlState _reconcileLoadedState({
    required WorkspaceSourceControlState? previous,
    required GitStatusResult status,
    required GitRepositoryState repositoryState,
    required List<GitStashEntry> stashes,
  }) {
    if (previous == null) {
      return WorkspaceSourceControlState(
        status: status,
        repositoryState: repositoryState,
        stashes: stashes,
      );
    }

    final entries = reconcileGitChangeEntryInstances(
      previous.status.entries,
      status.entries,
    );
    // GitStatusResult.groups is derived by GitChangeGroup.fromEntries, so
    // preserving the old status is safe when those entry instances are all
    // unchanged.
    final reconciledStatus = identical(entries, previous.status.entries)
        ? previous.status
        : GitStatusResult(
            entries: entries,
            groups: GitChangeGroup.fromEntries(entries),
          );
    final reconciledRepositoryState =
        gitRepositoryStateValuesEqual(previous.repositoryState, repositoryState)
        ? previous.repositoryState
        : repositoryState;
    final reconciledStashes =
        gitStashEntriesValuesEqual(previous.stashes, stashes)
        ? previous.stashes
        : stashes;
    const nextAction = null;

    if (identical(reconciledStatus, previous.status) &&
        identical(reconciledRepositoryState, previous.repositoryState) &&
        identical(reconciledStashes, previous.stashes) &&
        previous.action == nextAction) {
      return previous;
    }
    return WorkspaceSourceControlState(
      status: reconciledStatus,
      repositoryState: reconciledRepositoryState,
      stashes: reconciledStashes,
      action: nextAction,
    );
  }

  Future<void> _run(
    WorkspaceSourceControlAction action,
    Future<void> Function(GitBackend backend) operation,
  ) async {
    final previous = state.asData?.value;
    if (previous?.isBusy ?? false) {
      return;
    }
    if (previous != null) {
      state = AsyncData(
        WorkspaceSourceControlState(
          status: previous.status,
          repositoryState: previous.repositoryState,
          stashes: previous.stashes,
          action: action,
        ),
      );
    }
    try {
      await operation(ref.read(gitBackendProvider));
      state = AsyncData(await _load());
    } catch (error, stackTrace) {
      final recovered = await _recoverAfterFailure(previous);
      if (recovered != null) {
        state = AsyncData(recovered);
      } else {
        state = AsyncError(error, stackTrace);
      }
      rethrow;
    } finally {
      _scheduleQueuedWatcherReload();
    }
  }

  Future<void> _startWatching() async {
    final watcher = _watcher;
    if (watcher == null) {
      return;
    }
    try {
      final handle = await watcher.start(workspacePath: workspacePath);
      if (_disposed) {
        await watcher.stop(handle: handle);
        return;
      }
      _watcherHandle = handle;
      _watchSubscription = watcher
          .events(handle: handle)
          .listen((_) => _scheduleWatcherReload(), onError: (_) {});
    } catch (_) {
      // File watching is best-effort; explicit refresh still works.
    }
  }

  void _stopWatching() {
    _disposed = true;
    _watcherReloadQueued = false;
    _watcherReloadNotBefore = null;
    _watcherReloadDebounceTimer?.cancel();
    _watcherReloadDebounceTimer = null;
    final subscription = _watchSubscription;
    _watchSubscription = null;
    unawaited(subscription?.cancel());
    final handle = _watcherHandle;
    _watcherHandle = null;
    final watcher = _watcher;
    if (handle != null && watcher != null) {
      unawaited(watcher.stop(handle: handle));
    }
  }

  void _scheduleWatcherReload() {
    _watcherReloadDebounceTimer?.cancel();
    var delay = _watcherReloadDebounce;
    final notBefore = _watcherReloadNotBefore;
    if (notBefore != null) {
      final remaining = notBefore.difference(DateTime.now());
      if (remaining > delay) {
        delay = remaining;
      }
    }
    _watcherReloadDebounceTimer = Timer(
      delay,
      () => unawaited(_reloadFromWatcher()),
    );
  }

  Future<void> _reloadFromWatcher() async {
    if (_disposed) {
      return;
    }
    if (_watcherReloadInFlight) {
      _watcherReloadQueued = true;
      return;
    }
    // Defer while an action runs: `_run` reloads when it finishes, and skipping
    // here avoids clobbering its optimistic busy state.
    if (state.asData?.value.isBusy ?? false) {
      _watcherReloadQueued = true;
      return;
    }
    _watcherReloadInFlight = true;
    try {
      final next = await _load();
      if (_disposed || (state.asData?.value.isBusy ?? false)) {
        return;
      }
      state = AsyncData(next);
    } catch (_) {
      // Best-effort background refresh: keep the current state on failure.
    } finally {
      _watcherReloadInFlight = false;
      if (!_watcherReloadQueued) {
        _queuedWatcherReloads = 0;
        _watcherReloadNotBefore = null;
      }
      _scheduleQueuedWatcherReload();
    }
  }

  void _scheduleQueuedWatcherReload() {
    if (_disposed || !_watcherReloadQueued) {
      return;
    }
    _watcherReloadQueued = false;
    // Signals already waiting when a reload finished mean changes are
    // arriving faster than scans complete; back off so sustained churn does
    // not pin a full status scan every few hundred milliseconds. The floor
    // also gates fresh signals, otherwise they would shortcut the cooldown.
    final delay =
        _watcherReloadDebounce * (1 << _queuedWatcherReloads.clamp(0, 3));
    _queuedWatcherReloads += 1;
    _watcherReloadNotBefore = DateTime.now().add(delay);
    _scheduleWatcherReload();
  }

  Future<WorkspaceSourceControlState?> _recoverAfterFailure(
    WorkspaceSourceControlState? previous,
  ) async {
    try {
      return await _load();
    } catch (_) {
      return previous;
    }
  }

  List<String> _actionPaths(GitChangeEntry entry) {
    final paths = <String>[];
    final oldPath = entry.oldPath;
    if (entry.status == GitChangeStatus.renamed &&
        oldPath != null &&
        oldPath != entry.path) {
      paths.add(oldPath);
    }
    paths.add(entry.path);
    return paths;
  }
}
