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

  void watchProjectsRecovering(
    Stream<List<Project>> Function() streamFactory, {
    required void Function(List<Project>) onData,
    void Function(Object error)? onError,
    Duration restartDelay = const Duration(milliseconds: 250),
    Duration maxRestartDelay = const Duration(seconds: 5),
  }) {
    _projects.watchRecovering(
      streamFactory,
      onData: onData,
      onError: onError,
      restartDelay: restartDelay,
      maxRestartDelay: maxRestartDelay,
    );
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
  Timer? _restartTimer;
  Object? _token;
  var _consecutiveRestarts = 0;

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

  void watchRecovering(
    Stream<T> Function() streamFactory, {
    required void Function(T) onData,
    void Function(Object error)? onError,
    required Duration restartDelay,
    required Duration maxRestartDelay,
  }) {
    cancel();
    final token = Object();
    _token = token;

    void listen() {
      if (!identical(_token, token)) {
        return;
      }
      _restartTimer = null;
      var emittedData = false;
      Stream<T> stream;
      try {
        stream = streamFactory();
      } catch (error) {
        onError?.call(error);
        _scheduleRestart(
          token,
          listen,
          restartDelay: restartDelay,
          maxRestartDelay: maxRestartDelay,
        );
        return;
      }
      final subscription = stream.listen(
        (value) {
          emittedData = true;
          _consecutiveRestarts = 0;
          onData(value);
        },
        onError: onError,
        onDone: () {
          if (!identical(_token, token)) {
            return;
          }
          _subscription = null;
          if (emittedData) {
            _consecutiveRestarts = 0;
          }
          _scheduleRestart(
            token,
            listen,
            restartDelay: restartDelay,
            maxRestartDelay: maxRestartDelay,
          );
        },
        cancelOnError: false,
      );
      if (identical(_token, token)) {
        _subscription = subscription;
      } else {
        unawaited(subscription.cancel());
      }
    }

    listen();
  }

  void cancel() {
    final subscription = _subscription;
    _subscription = null;
    _restartTimer?.cancel();
    _restartTimer = null;
    _token = null;
    _consecutiveRestarts = 0;
    if (subscription != null) {
      unawaited(subscription.cancel());
    }
  }

  void _scheduleRestart(
    Object token,
    void Function() listen, {
    required Duration restartDelay,
    required Duration maxRestartDelay,
  }) {
    if (!identical(_token, token)) {
      return;
    }
    final delay = _consecutiveRestarts == 0
        ? Duration.zero
        : _scaledDelay(restartDelay, maxRestartDelay, _consecutiveRestarts - 1);
    _consecutiveRestarts += 1;
    _restartTimer?.cancel();
    _restartTimer = Timer(delay, listen);
  }

  Duration _scaledDelay(Duration base, Duration max, int exponent) {
    var micros = base.inMicroseconds;
    final maxMicros = max.inMicroseconds;
    for (var index = 0; index < exponent && micros < maxMicros; index += 1) {
      micros = (micros * 2).clamp(0, maxMicros);
    }
    return Duration(microseconds: micros.clamp(0, maxMicros));
  }

  void _clearIfCurrent(Object token) {
    if (identical(_token, token)) {
      _subscription = null;
      _token = null;
    }
  }
}
