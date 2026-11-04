import 'dart:async';

import 'package:riverpod_annotation/riverpod_annotation.dart';

part 'workspace_explorer_reveal.g.dart';

/// One-shot request to reveal a workspace-relative path in Explorer.
///
/// Source Control queues this, switches to the Explorer tab, and Explorer
/// consumes it after the tree is ready. The request is not persisted.
class const WorkspaceExplorerRevealRequest({
  required final String workspaceId,
  required final String relativePath,
  required final int generation,
});

abstract interface class WorkbenchWorkspaceExplorerRevealSink {
  void reveal({required String workspaceId, required String relativePath});
}

@Riverpod(keepAlive: true)
class WorkspaceExplorerRevealController
    extends _$WorkspaceExplorerRevealController
    implements WorkbenchWorkspaceExplorerRevealSink {
  int _generation = 0;
  int _lastConsumedGeneration = 0;
  final Map<int, Completer<bool>> _pendingConfirmations =
      <int, Completer<bool>>{};

  @override
  WorkspaceExplorerRevealRequest? build() => null;

  @override
  void reveal({required String workspaceId, required String relativePath}) {
    for (final pending in _pendingConfirmations.values) {
      if (!pending.isCompleted) pending.complete(false);
    }
    _pendingConfirmations.clear();
    _generation += 1;
    state = WorkspaceExplorerRevealRequest(
      workspaceId: workspaceId,
      relativePath: relativePath,
      generation: _generation,
    );
  }

  void consume(WorkspaceExplorerRevealRequest request) {
    if (state?.generation == request.generation) {
      _lastConsumedGeneration = request.generation;
      state = null;
      final confirmation = _pendingConfirmations.remove(request.generation);
      if (confirmation != null && !confirmation.isCompleted) {
        confirmation.complete(true);
      }
    }
  }

  /// Resolves only after the mounted Explorer has completed its actual tree
  /// reveal/scroll, not merely after the Workbench requested a reveal.
  Future<bool> waitForConsumed(
    WorkspaceExplorerRevealRequest request, {
    Duration timeout = const Duration(seconds: 8),
  }) async {
    if (_lastConsumedGeneration == request.generation) return true;
    if (state?.generation != request.generation) return false;
    final completer = _pendingConfirmations.putIfAbsent(
      request.generation,
      () => Completer<bool>(),
    );
    try {
      return await completer.future.timeout(timeout, onTimeout: () => false);
    } finally {
      if (identical(_pendingConfirmations[request.generation], completer)) {
        _pendingConfirmations.remove(request.generation);
      }
    }
  }
}
