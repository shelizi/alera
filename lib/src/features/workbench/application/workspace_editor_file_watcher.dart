import 'dart:async';

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
  final Set<String> _pendingChangedPaths = <String>{};
  bool _reloadInFlight = false;
  bool _disposed = false;
  Future<void> _mutation = Future<void>.value();

  Future<void> update({
    required String workspacePath,
    required Iterable<String> openRelativePaths,
  }) {
    final directories = _parentDirectories(openRelativePaths);
    return _enqueue(() async {
      if (_disposed) {
        return;
      }
      if (directories.isEmpty) {
        await _stop();
        return;
      }
      if (_workspacePath != workspacePath || _handle == null) {
        await _stop();
        final handle = await _workspaceFiles.startExplorerWatcher(
          workspacePath: workspacePath,
        );
        if (_disposed) {
          await _workspaceFiles.stopExplorerWatcher(handle: handle);
          return;
        }
        _workspacePath = workspacePath;
        _handle = handle;
        _subscription = _workspaceFiles
            .watchExplorerEvents(handle: handle)
            .listen(_scheduleReload, onError: (_) {});
      }
      if (_sameSet(_watchedDirectories, directories)) {
        return;
      }
      await _workspaceFiles.updateExplorerWatcher(
        handle: _handle!,
        watchedRelativePaths: directories.toList(growable: false),
      );
      _watchedDirectories = directories;
    });
  }

  Future<void> dispose() {
    _disposed = true;
    return _enqueue(_stop);
  }

  Future<void> _enqueue(Future<void> Function() action) {
    final next = _mutation.then((_) => action(), onError: (_) => action());
    _mutation = next.catchError((_) {});
    return next;
  }

  void _scheduleReload(native.WorkspaceExplorerWatchBatch batch) {
    if (_disposed || batch.changedRelativePaths.isEmpty) {
      return;
    }
    _pendingChangedPaths.addAll(batch.changedRelativePaths);
    if (_reloadInFlight) {
      return;
    }
    _reloadInFlight = true;
    unawaited(_drainReloads());
  }

  Future<void> _drainReloads() async {
    try {
      while (!_disposed && _pendingChangedPaths.isNotEmpty) {
        final workspacePath = _workspacePath;
        if (workspacePath == null) {
          _pendingChangedPaths.clear();
          return;
        }
        final changed = _pendingChangedPaths.toList(growable: false);
        _pendingChangedPaths.clear();
        try {
          await _editorSessions.reloadExternallyChangedCleanFiles(
            workspaceFiles: _workspaceFiles,
            workspacePath: workspacePath,
            relativePaths: changed,
          );
        } catch (_) {
          // Best-effort live reload. Save-time content-token validation remains
          // the final guard against overwriting a file changed on disk.
        }
      }
    } finally {
      _reloadInFlight = false;
      if (!_disposed && _pendingChangedPaths.isNotEmpty) {
        _reloadInFlight = true;
        unawaited(_drainReloads());
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
    _pendingChangedPaths.clear();
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
