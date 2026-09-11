import 'dart:async';

import 'package:alera/src/features/workbench/application/workbench_view_prefs_repository.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';

final class WorkbenchViewPrefsPersistenceQueue {
  Future<void>? _tail;

  Future<void> save({
    required WorkbenchViewPrefsRepository repository,
    required WorkbenchViewPrefs prefs,
  }) async {
    final previous = _tail;
    final gate = Completer<void>();
    final gateFuture = gate.future;
    _tail = gateFuture;
    if (previous != null) {
      await previous;
    }
    try {
      await repository.save(prefs);
    } finally {
      gate.complete();
      if (identical(_tail, gateFuture)) {
        _tail = null;
      }
    }
  }
}
