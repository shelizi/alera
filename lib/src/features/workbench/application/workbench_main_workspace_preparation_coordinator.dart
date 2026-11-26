import 'dart:async';

import 'package:alera/src/features/projects/domain/project.dart';

final class WorkbenchMainWorkspacePreparationCoordinator {
  final Map<String, Future<void>> _preparing = <String, Future<void>>{};

  Future<void> prepare({
    required Project project,
    required Future<void> Function(Project project) reconcile,
    required void Function(Project project, Object error) onError,
  }) {
    final pending = _preparing[project.id];
    if (pending != null) return pending;
    // Reconciliation already ensures the primary workspace. One shared
    // future avoids duplicate Git/Host work and prevents callers from
    // treating an in-flight preparation as already completed.
    final future = Future<void>.microtask(() async {
      try {
        await reconcile(project);
      } catch (error) {
        onError(project, error);
      } finally {
        _preparing.remove(project.id);
      }
    });
    _preparing[project.id] = future;
    return future;
  }
}
