import 'dart:async';

import 'package:alera/src/features/runtime_host/application/runtime_host_quit_planner.dart';
import 'package:alera/src/features/runtime_host/domain/runtime_host_quit_decision.dart';
import 'package:alera/src/features/runtime_host/domain/runtime_host_status.dart';
import 'package:alera/src/features/runtime_host/infra/bundled_sidecar_version_probe.dart';
import 'package:alera/src/platform/runtime_host/protocol/terminal_host_protocol.dart';

typedef RuntimeHostForceConfirm = Future<bool> Function({
  required String title,
  required String message,
  required String confirmLabel,
});

typedef RuntimeHostBusyQuitConfirm = Future<RuntimeHostQuitDecision> Function({
  required String title,
  required String message,
});

typedef RuntimeHostBusyQuitCommitted = void Function();

final class const RuntimeHostLifecycleStoppedException([this.cause])
    implements Exception {
  final Object? cause;

  @override
  String toString() => cause?.toString() ?? 'Runtime host is already stopped.';
}

final class const RuntimeHostLifecycleTransportException([this.cause])
    implements Exception {
  final Object? cause;

  @override
  String toString() =>
      cause?.toString() ?? 'Runtime host connection was interrupted.';
}

final class const RuntimeHostLifecycleStartupException([this.cause])
    implements Exception {
  final Object? cause;

  @override
  String toString() => cause?.toString() ?? 'Runtime host failed to start.';
}

abstract interface class RuntimeHostLifecycleClient {
  void beginAppQuit();

  void cancelAppQuit();

  void commitAppQuit();

  Future<Map<String, Object?>?> probeRuntimeStatus();

  Future<RuntimeHostShutdownResult> shutdownRuntime({bool force = false});

  Future<void> ensureStarted({required TerminalHostConfig config});
}

final class RuntimeHostLifecycleService({
  required final RuntimeHostLifecycleClient _client,
  required final BundledSidecarVersionProbe _bundledVersionProbe,
  required final TerminalHostConfig Function() _readConfig,
  final RuntimeHostQuitPlanner _quitPlanner = const RuntimeHostQuitPlanner(),
  final Duration _shutdownSettleTimeout = const Duration(seconds: 8),
}) {
  Future<RuntimeHostStatusSnapshot> loadStatus() async {
    BundledSidecarVersion bundled;
    try {
      bundled = await _bundledVersionProbe.probe();
    } catch (error) {
      return RuntimeHostStatusSnapshot(
        running: false,
        bundledVersion: 'unknown',
        error: error.toString(),
      );
    }
    try {
      final status = await _client.probeRuntimeStatus();
      if (status == null) {
        return RuntimeHostStatusSnapshot(
          running: false,
          bundledVersion: bundled.version,
          bundledCommit: bundled.commit,
        );
      }
      return RuntimeHostStatusSnapshot(
        running: true,
        bundledVersion: bundled.version,
        bundledCommit: bundled.commit,
        runtimeHostVersion: status['runtimeHostVersion'] as String?,
        runtimeHostCommit: status['runtimeHostCommit'] as String?,
        persistent: status['persistent'] == true,
        activeSessions: status['activeSessions'] is int
            ? status['activeSessions'] as int
            : 0,
        activeAgents: status['activeAgents'] is int
            ? status['activeAgents'] as int
            : 0,
        activePushSubscriptions: status['activePushSubscriptions'] is int
            ? status['activePushSubscriptions'] as int
            : 0,
      );
    } catch (error) {
      return RuntimeHostStatusSnapshot(
        running: false,
        bundledVersion: bundled.version,
        bundledCommit: bundled.commit,
        error: error.toString(),
      );
    }
  }

  Future<void> start() async {
    await _client.ensureStarted(config: _readConfig());
  }

  Future<bool> stop({
    bool force = false,
    RuntimeHostForceConfirm? confirmForce,
  }) async {
    try {
      await _requestShutdown(force: force);
    } on RuntimeHostBusyException catch (busy) {
      if (force || confirmForce == null) {
        rethrow;
      }
      final confirmed = await confirmForce(
        title: 'Force Stop Runtime',
        message: '${runtimeHostBusyMessage(busy)} Force stop terminates them.',
        confirmLabel: 'Force Stop',
      );
      if (!confirmed) {
        return false;
      }
      await _requestShutdown(force: true);
    }
    await _waitUntilStopped();
    return true;
  }

  Future<void> updateIfAvailable({
    RuntimeHostForceConfirm? confirmForce,
  }) async {
    final status = await loadStatus();
    if (!status.updateAvailable) {
      return;
    }
    final stopped = await stop(confirmForce: confirmForce);
    if (!stopped) {
      return;
    }
    await start();
  }

  /// Prepare the local runtime for an intentional app quit.
  ///
  /// Persistent CLI hosts and the keep-open setting leave the host running.
  /// App-launched sidecars soft-stop when idle; when busy, [confirmBusyQuit]
  /// chooses cancel, leave-open, or force-stop. Returns `false` only when the
  /// user cancels so the window should stay open.
  Future<bool> prepareAppQuit({
    required bool keepRuntimeOpen,
    RuntimeHostBusyQuitConfirm? confirmBusyQuit,
    RuntimeHostBusyQuitCommitted? onBusyQuitCommitted,
  }) async {
    _client.beginAppQuit();
    var committed = false;
    try {
      final allowed = await _prepareAppQuitAfterQuiesce(
        keepRuntimeOpen: keepRuntimeOpen,
        confirmBusyQuit: confirmBusyQuit,
        onBusyQuitCommitted: onBusyQuitCommitted,
      );
      if (!allowed) {
        return false;
      }
      _client.commitAppQuit();
      committed = true;
      return true;
    } finally {
      if (!committed) {
        _client.cancelAppQuit();
      }
    }
  }

  Future<bool> _prepareAppQuitAfterQuiesce({
    required bool keepRuntimeOpen,
    RuntimeHostBusyQuitConfirm? confirmBusyQuit,
    RuntimeHostBusyQuitCommitted? onBusyQuitCommitted,
  }) async {
    Map<String, Object?>? liveStatus;
    var statusKnown = true;
    if (!keepRuntimeOpen) {
      try {
        liveStatus = await _client.probeRuntimeStatus();
      } catch (_) {
        // Probe failure must not skip shutdown: the host may still be live.
        statusKnown = false;
      }
    }

    final initialPlan = _quitPlanner.initialPlan(
      keepRuntimeOpen: keepRuntimeOpen,
      statusKnown: statusKnown,
      runtimeRunning: liveStatus != null,
      persistent: liveStatus?['persistent'] == true,
    );
    if (initialPlan.action == RuntimeHostQuitAction.allowClose) {
      return true;
    }

    // Do not decide from the status snapshot alone. A push subscription should
    // keep a push-only runtime alive, but it must not hide terminals, agents,
    // or background jobs from the busy-quit confirmation. The shutdown reply
    // carries the complete busy counts, including jobs not exposed by status.
    try {
      await _shutdownForAppQuit(force: false);
      return true;
    } on RuntimeHostBusyException catch (busy) {
      final busyState = RuntimeHostBusyState(
        activeAgents: busy.activeAgents,
        activeSessions: busy.activeSessions,
        activeJobs: busy.activeJobs,
        activePushSubscriptions: busy.activePushSubscriptions,
      );
      var plan = _quitPlanner.busyPlan(busyState);
      if (plan.action == RuntimeHostQuitAction.confirm) {
        if (confirmBusyQuit == null) {
          return false;
        }
        final decision = await confirmBusyQuit(
          title: 'Runtime Still Has Work',
          message:
              '${runtimeHostBusyMessage(busy)} '
              'You can quit and leave the runtime running, or force stop it.',
        );
        plan = _quitPlanner.busyPlan(busyState, decision: decision);
      }
      if (plan.commitVisualQuit) {
        onBusyQuitCommitted?.call();
      }
      switch (plan.action) {
        case RuntimeHostQuitAction.allowClose:
        case RuntimeHostQuitAction.leaveOpen:
          return true;
        case RuntimeHostQuitAction.cancel:
          return false;
        case RuntimeHostQuitAction.forceStop:
          await _shutdownForAppQuit(force: true);
          return true;
        case RuntimeHostQuitAction.softStop:
        case RuntimeHostQuitAction.confirm:
          throw StateError(
            'Runtime quit planner produced an invalid busy plan.',
          );
      }
    }
  }

  Future<void> _waitUntilStopped() async {
    final deadline = DateTime.now().add(_shutdownSettleTimeout);
    while (DateTime.now().isBefore(deadline)) {
      try {
        final status = await _client.probeRuntimeStatus();
        if (status == null) {
          return;
        }
      } catch (error) {
        if (_isHostGone(error)) {
          return;
        }
        if (!_isTransientShutdownDisconnect(error)) {
          rethrow;
        }
      }
      await Future.pause(const Duration(milliseconds: 100));
    }
    throw StateError('The runtime host did not stop in time.');
  }

  /// App quit only needs the shutdown request accepted. The sidecar is
  /// detached, so waiting for its cleanup would keep the native window open
  /// while it terminates terminal trees and flushes its stores.
  Future<void> _shutdownForAppQuit({required bool force}) {
    return _requestShutdown(force: force);
  }

  /// Ask the live host to stop.
  ///
  /// Force-stop closes client sockets while the sidecar is still tearing down
  /// sessions, so the RPC can finish as EOF or "connection closed" instead of
  /// a JSON reply. That still means shutdown was accepted.
  Future<void> _requestShutdown({required bool force}) async {
    try {
      await _client.shutdownRuntime(force: force);
    } catch (error) {
      if (_isExpectedShutdownDisconnect(error)) {
        return;
      }
      rethrow;
    }
  }

  bool _isExpectedShutdownDisconnect(Object error) {
    return _isHostGone(error) || _isTransientShutdownDisconnect(error);
  }

  bool _isHostGone(Object error) =>
      error is RuntimeHostLifecycleStoppedException;

  bool _isTransientShutdownDisconnect(Object error) =>
      error is RuntimeHostLifecycleTransportException;
}

String runtimeHostBusyMessage(RuntimeHostBusyException busy) {
  final parts = <String>[];
  if (busy.activeAgents > 0) {
    parts.add('${busy.activeAgents} open agent(s)');
  }
  if (busy.activeSessions > 0) {
    parts.add('${busy.activeSessions} active terminal session(s)');
  }
  if (busy.activeJobs > 0) {
    parts.add('${busy.activeJobs} active background job(s)');
  }
  if (busy.activePushSubscriptions > 0) {
    parts.add('${busy.activePushSubscriptions} active push subscription(s)');
  }
  if (parts.isEmpty) {
    return 'The runtime still has active work.';
  }
  if (parts.length == 1) {
    return 'The runtime has ${parts.single}.';
  }
  if (parts.length == 2) {
    return 'The runtime has ${parts[0]} and ${parts[1]}.';
  }
  return 'The runtime has ${parts[0]}, ${parts[1]}, and ${parts[2]}.';
}
