import 'dart:async';

import 'package:alera/src/features/workbench/domain/workspace.dart';

final class WorkbenchWorkspaceSubscriptionRegistry {
  final Map<String, StreamSubscription<List<Workspace>>> _subscriptions =
      <String, StreamSubscription<List<Workspace>>>{};

  bool contains(String projectId) => _subscriptions.containsKey(projectId);

  List<String> get projectIds => _subscriptions.keys.toList(growable: false);

  bool watch({
    required String projectId,
    required Stream<List<Workspace>> stream,
    required void Function(List<Workspace>) onData,
  }) {
    if (contains(projectId)) {
      return false;
    }

    late final StreamSubscription<List<Workspace>> subscription;
    subscription = stream.listen(
      onData,
      onError: (Object _) {},
      onDone: () => _forgetIfCurrent(projectId, subscription),
      cancelOnError: false,
    );
    _subscriptions[projectId] = subscription;
    return true;
  }

  void cancelProject(String projectId) {
    final subscription = _subscriptions.remove(projectId);
    if (subscription != null) {
      unawaited(subscription.cancel());
    }
  }

  void cancelAll() {
    final subscriptions = _subscriptions.values.toList(growable: false);
    _subscriptions.clear();
    for (final subscription in subscriptions) {
      unawaited(subscription.cancel());
    }
  }

  void _forgetIfCurrent(
    String projectId,
    StreamSubscription<List<Workspace>> subscription,
  ) {
    if (identical(_subscriptions[projectId], subscription)) {
      _subscriptions.remove(projectId);
    }
  }
}
