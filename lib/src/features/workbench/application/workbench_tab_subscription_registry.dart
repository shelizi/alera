import 'dart:async';

import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

final class WorkbenchTabSubscriptionRegistry {
  final Map<String, StreamSubscription<List<WorkspaceTabRecord>>>
  _subscriptions = <String, StreamSubscription<List<WorkspaceTabRecord>>>{};
  final Map<String, String> _projectIdsByWorkspace = <String, String>{};

  bool contains(String workspaceId) => _subscriptions.containsKey(workspaceId);

  List<String> workspaceIdsForProject(String projectId) {
    return <String>[
      for (final entry in _projectIdsByWorkspace.entries)
        if (entry.value == projectId) entry.key,
    ];
  }

  bool watch({
    required String projectId,
    required String workspaceId,
    required Stream<List<WorkspaceTabRecord>> stream,
    required void Function(List<WorkspaceTabRecord>) onData,
  }) {
    if (contains(workspaceId)) {
      return false;
    }
    _projectIdsByWorkspace[workspaceId] = projectId;
    final subscription = stream.listen(
      onData,
      onError: (Object _) {},
      onDone: () => forget(workspaceId),
      cancelOnError: false,
    );
    _subscriptions[workspaceId] = subscription;
    return true;
  }

  void forget(String workspaceId) {
    _subscriptions.remove(workspaceId);
    _projectIdsByWorkspace.remove(workspaceId);
  }

  void cancelWorkspace(String workspaceId) {
    final subscription = _subscriptions.remove(workspaceId);
    _projectIdsByWorkspace.remove(workspaceId);
    if (subscription != null) {
      unawaited(subscription.cancel());
    }
  }

  void cancelAll() {
    final subscriptions = _subscriptions.values.toList(growable: false);
    _subscriptions.clear();
    _projectIdsByWorkspace.clear();
    for (final subscription in subscriptions) {
      unawaited(subscription.cancel());
    }
  }
}
