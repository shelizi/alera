import 'dart:async';

import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workspace_section_repository.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';

final class WorkbenchRootSubscriptionRegistry {
  final _SubscriptionSlot<WorkspaceSectionSnapshot> _sections =
      _SubscriptionSlot<WorkspaceSectionSnapshot>();
  final _SubscriptionSlot<List<Project>> _projects =
      _SubscriptionSlot<List<Project>>();
  final _SubscriptionSlot<WorkbenchViewPrefs> _viewPrefs =
      _SubscriptionSlot<WorkbenchViewPrefs>();

  bool get hasSections => _sections.active;

  bool get hasProjects => _projects.active;

  bool get hasViewPrefs => _viewPrefs.active;

  void watchSections(
    Stream<WorkspaceSectionSnapshot> stream, {
    required void Function(WorkspaceSectionSnapshot) onData,
    void Function(Object error)? onError,
  }) {
    _sections.watch(stream, onData: onData, onError: onError);
  }

  void watchProjects(
    Stream<List<Project>> stream, {
    required void Function(List<Project>) onData,
    void Function(Object error)? onError,
  }) {
    _projects.watch(stream, onData: onData, onError: onError);
  }

  void watchViewPrefs(
    Stream<WorkbenchViewPrefs> stream, {
    required void Function(WorkbenchViewPrefs) onData,
    void Function(Object error)? onError,
  }) {
    _viewPrefs.watch(stream, onData: onData, onError: onError);
  }

  void cancelAll() {
    _sections.cancel();
    _projects.cancel();
    _viewPrefs.cancel();
  }
}

final class _SubscriptionSlot<T> {
  StreamSubscription<T>? _subscription;
  Object? _token;

  bool get active => _subscription != null;

  void watch(
    Stream<T> stream, {
    required void Function(T) onData,
    void Function(Object error)? onError,
  }) {
    cancel();
    final token = Object();
    _token = token;
    final subscription = stream.listen(
      onData,
      onError: onError,
      onDone: () => _clearIfCurrent(token),
      cancelOnError: false,
    );
    if (identical(_token, token)) {
      _subscription = subscription;
    } else {
      unawaited(subscription.cancel());
    }
  }

  void cancel() {
    final subscription = _subscription;
    _subscription = null;
    _token = null;
    if (subscription != null) {
      unawaited(subscription.cancel());
    }
  }

  void _clearIfCurrent(Object token) {
    if (identical(_token, token)) {
      _subscription = null;
      _token = null;
    }
  }
}
