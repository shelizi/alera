import 'dart:async';

import 'package:alera/src/platform/runtime_host/protocol/terminal_host_protocol.dart';
import 'package:alera/src/shared/infra/runtime/runtime_change_coalescer.dart';

/// Timeout for the bulk list calls behind a snapshot stream.
///
/// The host actor is single-threaded, so a background refresh legitimately
/// queues behind a coordinator sweep or a burst of PTY flushes. The default
/// interactive timeout is too tight for that, and a premature timeout only
/// buys a retry that queues up again.
const Duration runtimeSnapshotRequestTimeout = Duration(seconds: 30);

final class const RuntimeSnapshotReadFailure({
  required this.error,
  required this.recoverable,
}) {
  final Object error;
  final bool recoverable;
}

/// Protocol/data-shape failures will not improve by spinning a timer. Unknown
/// failures remain recoverable for backward compatibility with older hosts that
/// reported transport failures as untyped errors.
bool runtimeSnapshotReadErrorIsRecoverable(Object error) {
  return error is! TerminalHostConflictException &&
      error is! FormatException &&
      error is! ArgumentError &&
      error is! UnsupportedError &&
      error is! TypeError;
}

/// Matches an event payload that carries no scope, i.e. every watcher refreshes.
bool _matchesAnyScope(Map<String, Object?> payload) => true;

/// Builds a scope predicate that accepts an event when it targets [ownId] or
/// when it carries no scope at all.
///
/// A missing or empty scope means wildcard on purpose: an older host broadcasts
/// change events with an empty payload, so the wildcard keeps a new app correct
/// against a host that is already running.
bool Function(Map<String, Object?>) runtimeScopeMatcher(
  String field,
  String ownId,
) {
  return (payload) {
    final scope = payload[field];
    return scope is! String || scope.isEmpty || scope == ownId;
  };
}

/// A stream of snapshots that survives terminal host connection loss.
///
/// The stream never emits an error and never completes on its own. That is the
/// whole point: an `async*` body that throws while reading a snapshot is dead
/// for good, and its listener has no way to tell recoverable IPC failure from
/// intentional completion. Recoverable read failures schedule a retry; protocol
/// and data-shape failures stay subscribed but wait for a later runtime event.
///
/// The retry loop doubles as the reconnect driver. Reading a snapshot goes
/// through [RuntimeHostClient.runtimeRequest], which lazily reopens the socket,
/// so a retry is what brings the connection back. Reconnecting then emits
/// [aleraRuntimeHostConnectedEvent], which force-refreshes every other watcher.
/// One watcher recovering heals the whole tree, so this coupling is deliberate.
Stream<T> runtimeSnapshotStream<T>({
  required RuntimeHostClient client,
  required Set<String> eventNames,
  required Future<T> Function() readSnapshot,
  required String coalesceKey,
  required RuntimeChangeCoalescer coalescer,
  bool Function(Map<String, Object?> payload) matchesScope = _matchesAnyScope,
  Duration retryDelay = const Duration(seconds: 1),
  Duration maxRetryDelay = const Duration(seconds: 15),
  bool Function(Object error) isRecoverableError =
      runtimeSnapshotReadErrorIsRecoverable,
  void Function(RuntimeSnapshotReadFailure failure)? onReadFailure,
}) {
  final coalesceOwner = Object();
  late final StreamController<T> controller;
  StreamSubscription<RuntimeHostEvent>? eventSub;
  Timer? retryTimer;
  var backoff = retryDelay;
  var refreshRunning = false;
  var refreshQueued = false;

  Future<void> refresh() async {
    if (controller.isClosed) {
      return;
    }
    if (refreshRunning) {
      refreshQueued = true;
      return;
    }

    refreshRunning = true;
    try {
      do {
        refreshQueued = false;
        retryTimer?.cancel();
        retryTimer = null;
        try {
          final value = await readSnapshot();
          if (controller.isClosed) {
            return;
          }
          controller.add(value);
          backoff = retryDelay;
        } on Object catch (error) {
          if (controller.isClosed) {
            return;
          }
          final recoverable = isRecoverableError(error);
          onReadFailure?.call(
            RuntimeSnapshotReadFailure(error: error, recoverable: recoverable),
          );
          if (recoverable) {
            retryTimer = Timer(backoff, () => unawaited(refresh()));
            final next = backoff * 2;
            backoff = next > maxRetryDelay ? maxRetryDelay : next;
          }
        }
      } while (refreshQueued && !controller.isClosed);
    } finally {
      refreshRunning = false;
    }
  }

  controller = StreamController<T>(
    onListen: () {
      eventSub = client.runtimeEvents.listen(
        (event) {
          if (event.name == aleraRuntimeHostConnectedEvent) {
            retryTimer?.cancel();
            backoff = retryDelay;
            coalescer.schedule(coalesceKey, coalesceOwner, refresh);
            return;
          }
          if (eventNames.contains(event.name) && matchesScope(event.payload)) {
            coalescer.schedule(coalesceKey, coalesceOwner, refresh);
          }
        },
        onError: (Object _) {},
        cancelOnError: false,
      );
      unawaited(refresh());
    },
    onCancel: () {
      retryTimer?.cancel();
      retryTimer = null;
      coalescer.cancel(coalesceKey, coalesceOwner);
      final sub = eventSub;
      eventSub = null;
      return sub?.cancel();
    },
  );
  return controller.stream;
}
