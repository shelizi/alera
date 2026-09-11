import 'dart:async';

import 'package:alera/src/features/workbench/application/workbench_layout_repository.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';

final class WorkbenchSequencedLayoutRepository
    implements WorkbenchLayoutRepository {
  WorkbenchSequencedLayoutRepository(this._delegate);

  final WorkbenchLayoutRepository _delegate;
  final Map<String, Future<void>> _writeTails = <String, Future<void>>{};

  @override
  Future<WorkbenchLayout?> findWorkbenchLayout(String workspaceId) {
    return _delegate.findWorkbenchLayout(workspaceId);
  }

  @override
  Future<WorkbenchLayout> upsertWorkbenchLayout(WorkbenchLayout layout) async {
    final workspaceId = layout.workspaceId;
    final previous = _writeTails[workspaceId];
    final gate = Completer<void>();
    final gateFuture = gate.future;
    _writeTails[workspaceId] = gateFuture;

    if (previous != null) {
      await previous;
    }
    try {
      return await _delegate.upsertWorkbenchLayout(layout);
    } finally {
      gate.complete();
      if (identical(_writeTails[workspaceId], gateFuture)) {
        _writeTails.remove(workspaceId);
      }
    }
  }
}
