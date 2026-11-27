import 'dart:async';

import 'package:alera/src/features/workbench/application/workbench_background_task_pool.dart';
import 'package:alera/src/shared/infra/git/git_explorer_status.dart';

/// Coalesces concurrent Explorer Git snapshot requests without caching a
/// pre-mutation result for a request made after the scan started.
final class WorkspaceExplorerGitSnapshotRefresh {
  WorkspaceExplorerGitSnapshotRefresh({
    required this._load,
    required this._publish,
  });

  final Future<GitExplorerStatusSnapshot> Function(String) _load;
  final void Function(GitExplorerStatusSnapshot) _publish;
  final _pool = WorkbenchBackgroundTaskPool(maxConcurrent: 4);
  _RefreshScope? _scope;
  bool _disposed = false;

  Future<void> refresh(String workspacePath) {
    if (_disposed) return Future<void>.value();
    if (_scope?.workspacePath != workspacePath) {
      clear();
      _scope = _RefreshScope(workspacePath);
    }
    final scope = _scope!;
    final pending = scope.pending ??= Completer<void>();
    if (!scope.scheduled) {
      scope.scheduled = true;
      // Coalesce synchronous callers before claiming a native read slot.
      scheduleMicrotask(() {
        if (identical(_scope, scope)) {
          unawaited(_pool.run(scope, () => _drain(scope)));
        }
      });
    }
    return pending.future;
  }

  Future<void> _drain(_RefreshScope scope) async {
    try {
      while (identical(_scope, scope) && scope.pending != null) {
        final active = scope.pending!;
        scope.pending = null;
        scope.active = active;
        try {
          GitExplorerStatusSnapshot snapshot;
          try {
            snapshot = await _load(scope.workspacePath);
          } catch (_) {
            // Preserve Explorer's best-effort empty snapshot fallback.
            snapshot = const GitExplorerStatusSnapshot.empty();
          }
          if (identical(_scope, scope)) {
            _publish(snapshot);
          }
        } finally {
          if (!active.isCompleted) active.complete();
          scope.active = null;
        }
      }
    } finally {
      scope.scheduled = false;
    }
  }

  /// Retire all waiting callers without canceling IO already dispatched.
  /// Only the current scope can publish. Retired reads retain their slots
  /// until completion; queued obsolete scopes never start.
  void clear() {
    final scope = _scope;
    _scope = null;
    if (scope == null) return;
    _pool.cancelPending(scope);
    final pending = scope.pending;
    scope.pending = null;
    if (pending != null && !pending.isCompleted) pending.complete();
    final active = scope.active;
    if (active != null && !active.isCompleted) active.complete();
  }

  void dispose() {
    _disposed = true;
    clear();
  }
}

final class _RefreshScope {
  _RefreshScope(this.workspacePath);

  final String workspacePath;
  Completer<void>? active;
  Completer<void>? pending;
  bool scheduled = false;
}
