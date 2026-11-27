import 'dart:async';

import 'package:alera/src/features/workbench/application/workbench_background_task_pool.dart';
import 'package:alera/src/features/workbench/application/workspace_file_service.dart';
import 'package:alera/src/rust/api/workspace_files.dart' as native;

/// Low-overhead watcher for files that are currently open in editor tabs.
///
/// The Rust watcher is event-driven and non-recursive. We watch only the
/// parent directories of open files, coalesce event batches, and perform a
/// cheap metadata-token check before asking a clean editor to reload.
class WorkspaceEditorFileWatcher {
  WorkspaceEditorFileWatcher({
    required WorkspaceFileService workspaceFiles,
    required EditorSessionRegistry editorSessions,
  }) : _workspaceFiles = workspaceFiles,
       _editorSessions = editorSessions;

  final WorkspaceFileService _workspaceFiles;
  final EditorSessionRegistry _editorSessions;

  native.WorkspaceExplorerWatcherHandle? _handle;
  StreamSubscription<native.WorkspaceExplorerWatchBatch>? _subscription;
  String? _workspacePath;
  Set<String> _watchedDirectories = <String>{};
  // Native batches are bounded individually, but another batch can arrive
  // while Dart awaits metadata/reload IO. Bound that downstream backlog too.
  static const _maxPendingChangedPaths = 512;
  final Set<String> _pendingChangedPaths = <String>{};
  // A retired read can finish after the next workspace starts. Only the
  // current owner may drain its pending paths or publish notifications.
  // Reuse the bounded pool so rapid switching cannot start unlimited IO.
  final _reloadPool = WorkbenchBackgroundTaskPool(maxConcurrent: 4);
  Object _reloadOwner = Object();
  bool _reloadInFlight = false;
  bool _hasStartedWatcher = false;
  bool _needsActivationReconcile = false;
  bool _disposed = false;
  // Directory registrations are snapshots of the desired open tabs, not
  // incremental mutations. Retain only the latest target behind active IO.
  ({String workspacePath, Set<String> directories})? _pendingUpdate;
  Future<void>? _updateDrain;
  Future<void>? _disposeFuture;

  Future<void> update({
    required String workspacePath,
    required Iterable<String> openRelativePaths,
  }) {
    if (_disposed) return Future<void>.value();
    final directories = _parentDirectories(openRelativePaths);
    if (_updateDrain == null &&
        (directories.isEmpty
            ? _handle == null
            : _workspacePath == workspacePath &&
                  _handle != null &&
                  !_needsActivationReconcile &&
                  _sameSet(_watchedDirectories, directories))) {
      return Future<void>.value();
    }
    _pendingUpdate = (workspacePath: workspacePath, directories: directories);
    // Defer the first application by one microtask so a synchronous burst
    // also collapses, and the drain future is installed before it can finish.
    return _updateDrain ??= Future<void>.microtask(_drainUpdates);
  }

  Future<void> _drainUpdates() async {
    try {
      while (!_disposed && _pendingUpdate != null) {
        final target = _pendingUpdate!;
        _pendingUpdate = null;
        try {
          await _applyUpdate(target.workspacePath, target.directories);
        } catch (_) {
          // A superseded registration failure must not prevent the latest
          // target from being attempted. A failure for the final target still
          // reaches the callers, and a later update can retry it.
          if (_disposed || _pendingUpdate == null) rethrow;
        }
      }
    } finally {
      _updateDrain = null;
    }
  }

  Future<void> _applyUpdate(
    String workspacePath,
    Set<String> directories,
  ) async {
    if (directories.isEmpty) {
      await _stop();
      return;
    }
    if (_workspacePath != workspacePath || _handle == null) {
      await _stop();
      if (_disposed || _pendingUpdate != null) return;
      final handle = await _workspaceFiles.startExplorerWatcher(
        workspacePath: workspacePath,
      );
      if (_disposed) {
        await _workspaceFiles.stopExplorerWatcher(handle: handle);
        return;
      }
      _workspacePath = workspacePath;
      _handle = handle;
      _needsActivationReconcile = _hasStartedWatcher;
      _hasStartedWatcher = true;
      final owner = _reloadOwner;
      _subscription = _workspaceFiles
          .watchExplorerEvents(handle: handle)
          .listen((batch) {
            if (_isCurrent(owner)) {
              _scheduleReload(batch.changedRelativePaths);
            }
          }, onError: (_) {});
      // The next pass either reuses this handle for the latest directories
      // or retires it without registering obsolete paths.
      if (_pendingUpdate != null) return;
    }
    if (!_sameSet(_watchedDirectories, directories)) {
      try {
        await _workspaceFiles.updateExplorerWatcher(
          handle: _handle!,
          watchedRelativePaths: directories.toList(growable: false),
        );
      } catch (_) {
        // Native watch/unwatch may have partially succeeded. Discard the
        // handle so retrying even the previous target cannot reuse it.
        await _stop();
        rethrow;
      }
      _watchedDirectories = directories;
    }
    // A newly registered watcher may have missed events while inactive.
    // Do not start catch-up IO for a target that is about to be superseded.
    if (_needsActivationReconcile && !_disposed && _pendingUpdate == null) {
      _needsActivationReconcile = false;
      _scheduleReload(const ['']);
    }
  }

  Future<void> dispose() {
    _disposed = true;
    _pendingUpdate = null;
    return _disposeFuture ??= _disposeAfterUpdate();
  }

  Future<void> _disposeAfterUpdate() async {
    try {
      await _updateDrain;
    } catch (_) {
      // Even a failed registration must not prevent native handle cleanup.
    }
    await _stop();
  }

  bool _isCurrent(Object owner) => !_disposed && identical(owner, _reloadOwner);

  void _scheduleReload(List<String> changedPaths) {
    if (_disposed || changedPaths.isEmpty) {
      return;
    }
    if (!_pendingChangedPaths.contains('')) {
      for (final path in changedPaths) {
        if (path.isEmpty ||
            (_pendingChangedPaths.length >= _maxPendingChangedPaths &&
                !_pendingChangedPaths.contains(path))) {
          // Preserve the native root-rescan contract. The root dominates
          // subsequent detail until drained; open documents and viewers
          // are reconciled rather than dropping a potentially changed file.
          _pendingChangedPaths
            ..clear()
            ..add('');
          break;
        }
        _pendingChangedPaths.add(path);
      }
    }
    _startReloadIfReady();
  }

  void _startReloadIfReady() {
    final workspacePath = _workspacePath;
    if (_disposed ||
        _reloadInFlight ||
        workspacePath == null ||
        _pendingChangedPaths.isEmpty) {
      return;
    }
    final owner = _reloadOwner;
    _reloadInFlight = true;
    unawaited(
      _reloadPool.run(owner, () => _drainReloads(owner, workspacePath)),
    );
  }

  Future<void> _drainReloads(Object owner, String workspacePath) async {
    try {
      while (_isCurrent(owner) && _pendingChangedPaths.isNotEmpty) {
        final changed = _pendingChangedPaths.toList(growable: false);
        _pendingChangedPaths.clear();
        try {
          await _editorSessions.reloadExternallyChangedCleanFiles(
            workspaceFiles: _workspaceFiles,
            workspacePath: workspacePath,
            relativePaths: changed,
            isCurrent: () => _isCurrent(owner),
          );
        } catch (_) {
          // Best-effort live reload. Save-time content-token validation remains
          // the final guard against overwriting a file changed on disk.
        } finally {
          if (_isCurrent(owner)) {
            _editorSessions.notifyExternalFileChanges(
              workspacePath: workspacePath,
              relativePaths: changed,
            );
          }
        }
      }
    } finally {
      if (_isCurrent(owner)) {
        _reloadInFlight = false;
        _startReloadIfReady();
      }
    }
  }

  Future<void> _stop() async {
    final subscription = _subscription;
    final handle = _handle;
    _subscription = null;
    _handle = null;
    _workspacePath = null;
    _watchedDirectories = <String>{};
    _needsActivationReconcile = false;
    final retiredOwner = _reloadOwner;
    _reloadOwner = Object();
    _reloadInFlight = false;
    _pendingChangedPaths.clear();
    _reloadPool.cancelPending(retiredOwner);
    await subscription?.cancel();
    if (handle != null) {
      await _workspaceFiles.stopExplorerWatcher(handle: handle);
    }
  }

  static Set<String> _parentDirectories(Iterable<String> relativePaths) {
    final directories = <String>{};
    for (final path in relativePaths) {
      if (path.isEmpty) {
        continue;
      }
      final normalized = path.replaceAll('\\', '/');
      final separator = normalized.lastIndexOf('/');
      directories.add(separator < 0 ? '' : normalized.substring(0, separator));
    }
    return directories;
  }

  static bool _sameSet(Set<String> left, Set<String> right) {
    return left.length == right.length && left.containsAll(right);
  }
}
