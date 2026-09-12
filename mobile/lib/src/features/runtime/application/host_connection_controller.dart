import 'dart:async';

import 'package:alera_mobile/src/features/runtime/domain/host_retry_policy.dart';
import 'package:alera_mobile/src/features/runtime/application/host_connection_health.dart';
import 'package:alera_mobile/src/features/runtime/domain/connection_attempt.dart';

import 'package:alera_mobile/src/app/lifecycle/app_lifecycle_controller.dart';
import 'package:alera_mobile/src/features/runtime/application/host_connection_client_factory.dart';
import 'package:alera_mobile/src/features/runtime/domain/host_reachability.dart';
import 'package:alera_mobile/src/features/runtime/domain/runtime_restart_result.dart';
import 'package:alera_mobile/src/features/runtime/infra/mobile_runtime_client.dart';
import 'package:flutter/widgets.dart';
import 'package:logging/logging.dart';
import 'package:riverpod_annotation/riverpod_annotation.dart';

export 'host_connection_reader.dart';

export 'package:alera_mobile/src/features/runtime/domain/host_reachability.dart'
    show HostUnreachableException, RuntimeConnectionLost;

part 'host_connection_controller.g.dart';

const Duration _runtimeRestartReconnectDelay = Duration(milliseconds: 300);
const Duration _connectionCleanupTimeout = Duration(seconds: 2);

/// Owns the WebSocket connection to one paired runtime host. The client is
/// connected and authenticated before it is exposed. Transport failures recover
/// while the app is in the foreground, and leaving every host surface disposes
/// the client and stops retry work.
@riverpod
class HostConnectionController extends _$HostConnectionController {
  final Logger _logger = Logger('HostConnectionController');
  MobileRuntimeClient? _client;
  StreamSubscription<(Object, StackTrace?)>? _closeSub;
  Timer? _retryTimer;
  Timer? _healthyTimer;
  ConnectionAttempt? _openingAttempt;
  Completer<void>? _buildAttempt;
  Future<void>? _connectionAttempt;
  int _lifecycleEpoch = 0;
  AppLifecycleState _lifecycleState = .resumed;
  int _retryIndex = 0;
  bool _building = false;
  bool _disposed = false;

  @override
  Future<MobileRuntimeClient> build(String hostId) async {
    final buildAttempt = Completer<void>();
    _buildAttempt = buildAttempt;
    _building = true;
    _lifecycleState = ref.read(appLifecycleControllerProvider);
    ref.listen(appLifecycleControllerProvider, _handleLifecycleChange);
    ref.listen(hostConnectionHealthControllerProvider(hostId), (_, _) {});
    ref.onDispose(_dispose);
    try {
      final client = await _openClient();
      await _bindClient(client);
      return client;
    } catch (error, stackTrace) {
      _scheduleRetry(error);
      Error.throwWithStackTrace(error, stackTrace);
    } finally {
      _building = false;
      buildAttempt.complete();
      if (identical(_buildAttempt, buildAttempt)) {
        _buildAttempt = null;
      }
    }
  }

  Future<MobileRuntimeClient> requireUsableClient() async {
    await _buildAttempt?.future;
    var client = _client;
    if (client != null && client.isConnectionUsable) {
      return client;
    }
    await reconnectNow();
    client = _client;
    if (client != null && client.isConnectionUsable) {
      return client;
    }
    final error = state.error;
    if (error != null) {
      throw error;
    }
    throw const RuntimeConnectionLost();
  }

  Future<void> reconnectNow() {
    _retryIndex = 0;
    _retryTimer?.cancel();
    _retryTimer = null;
    return _runReconnect();
  }

  Future<RuntimeRestartResult> restartRuntime({bool force = false}) async {
    // state.value retains the client a reconnect already disposed, so only
    // the bound live client may carry a restart.
    final client = _client;
    if (client == null) {
      throw StateError('Host is not connected.');
    }
    try {
      final result = await client.restartRuntime(force: force);
      await _closeSub?.cancel();
      _closeSub = null;
      _client = null;
      state = const AsyncLoading<MobileRuntimeClient>();
      unawaited(_recoverAfterRuntimeRestart(client));
      return result;
    } on RuntimeRestartBusyException {
      rethrow;
    } on Object catch (error, stackTrace) {
      _logger.warning(
        'could not restart runtime host $hostId',
        error,
        stackTrace,
      );
      rethrow;
    }
  }

  Future<void> _recoverAfterRuntimeRestart(MobileRuntimeClient client) async {
    try {
      await client.dispose();
    } on Object catch (error, stackTrace) {
      _logger.warning(
        'could not close the previous runtime connection for $hostId',
        error,
        stackTrace,
      );
    }
    await Future.pause(_runtimeRestartReconnectDelay);
    if (!_disposed) {
      await reconnectNow();
    }
  }

  Future<MobileRuntimeClient> _openClient() async {
    _health(const HostConnectionHealth());
    final attempt = ConnectionAttempt();
    _openingAttempt = attempt;
    final factory = HostConnectionClientFactory(ref: ref, hostId: hostId);
    try {
      return await attempt.run(() => factory.openWithin(attempt));
    } finally {
      attempt.finish();
      if (identical(_openingAttempt, attempt)) _openingAttempt = null;
    }
  }

  Future<void> _bindClient(MobileRuntimeClient client) async {
    if (_disposed) {
      await client.dispose();
      return;
    }
    final previousClient = _client;
    _client = null;
    final previousCloseSub = _closeSub;
    _closeSub = null;
    await _cancelCloseSubscription(previousCloseSub);
    if (previousClient != null && !identical(previousClient, client)) {
      await previousClient.dispose();
    }
    if (_disposed) {
      await client.dispose();
      return;
    }
    if (!client.isConnectionUsable) {
      await client.dispose();
      throw const RuntimeConnectionLost();
    }
    _client = client;
    _health(
      HostConnectionHealth(
        phase: 'connected',
        transport: client.transport,
        connectedAt: DateTime.now().toUtc(),
      ),
    );
    _healthyTimer?.cancel();
    _healthyTimer = Timer(const Duration(seconds: 30), () {
      if (identical(_client, client) && client.isConnectionUsable) {
        _retryIndex = 0;
      }
    });
    final closeSub = client.connectionFailures.listen(
      (failure) => _handleClientEnded(
        client,
        failure.$1,
        failure.$2 ?? StackTrace.current,
      ),
      onDone: () =>
          _handleClientEnded(client, const RuntimeConnectionLost(), .current),
      cancelOnError: false,
    );
    _closeSub = closeSub;
  }

  void _handleClientEnded(
    MobileRuntimeClient client,
    Object error,
    StackTrace stackTrace,
  ) {
    if (_disposed || !identical(_client, client)) {
      return;
    }
    _client = null;
    final closeSub = _closeSub;
    _healthyTimer?.cancel();
    _closeSub = null;
    unawaited(_cancelCloseSubscription(closeSub));
    // Failure delivery is synchronous; finish it before closing its stream.
    unawaited(Future<void>.microtask(client.dispose));
    state = AsyncError(error, stackTrace);
    _logger.warning(
      'connection to host $hostId ended; scheduling recovery',
      error,
      stackTrace,
    );
    _scheduleRetry(error, immediate: true);
  }

  void _handleLifecycleChange(
    AppLifecycleState? previous,
    AppLifecycleState next,
  ) {
    _lifecycleState = next;
    _lifecycleEpoch += 1;
    if (next != AppLifecycleState.resumed) {
      if (next == AppLifecycleState.paused) _openingAttempt?.cancel();
      _retryTimer?.cancel();
      _retryTimer = null;
      return;
    }
    if (previous == AppLifecycleState.resumed || _building) {
      return;
    }
    final client = _client;
    if (client == null) {
      if (!state.hasError || isRetryableHostFailure(state.error!)) {
        unawaited(_runReconnect());
      }
      return;
    }
    unawaited(_probe(client));
  }

  Future<void> _probe(MobileRuntimeClient client) async {
    final epoch = _lifecycleEpoch;
    try {
      await client.probeConnection();
    } on Object catch (error, stackTrace) {
      if (_disposed ||
          !identical(_client, client) ||
          epoch != _lifecycleEpoch ||
          _lifecycleState != AppLifecycleState.resumed) {
        return;
      }
      _logger.warning(
        'foreground probe failed for host $hostId; reconnecting',
        error,
        stackTrace,
      );
      await _runReconnect();
    }
  }

  void _scheduleRetry(Object error, {bool immediate = false}) {
    if (!_disposed && !isRetryableHostFailure(error)) {
      _health(
        HostConnectionHealth(phase: 'blocked', error: hostFailureCause(error)),
      );
    }
    if (_disposed ||
        _lifecycleState != AppLifecycleState.resumed ||
        !isRetryableHostFailure(error) ||
        _retryTimer != null) {
      return;
    }
    final delay = hostRetryDelay(_retryIndex, error, immediate: immediate);
    if (_retryIndex < 5) {
      _retryIndex += 1;
    }
    _health(
      HostConnectionHealth(
        phase: 'retrying',
        error: hostFailureCause(error),
        nextRetryAt: DateTime.now().toUtc().add(delay),
      ),
    );
    _retryTimer = Timer(delay, () {
      _retryTimer = null;
      unawaited(_runReconnect());
    });
  }

  Future<void> _runReconnect() {
    final buildAttempt = _buildAttempt;
    if (buildAttempt != null) return buildAttempt.future;
    final activeAttempt = _connectionAttempt;
    if (activeAttempt != null) {
      return activeAttempt;
    }
    if (_disposed || _lifecycleState != AppLifecycleState.resumed) {
      return Future<void>.value();
    }
    late final Future<void> attempt;
    attempt = _performReconnect().whenComplete(() {
      if (identical(_connectionAttempt, attempt)) {
        _connectionAttempt = null;
      }
    });
    _connectionAttempt = attempt;
    return attempt;
  }

  Future<void> _performReconnect() async {
    _retryTimer?.cancel();
    _retryTimer = null;
    final previousClient = _client;
    _client = null;
    final previousCloseSub = _closeSub;
    _closeSub = null;
    if (!_disposed) {
      state = const AsyncLoading<MobileRuntimeClient>();
    }
    await _cancelCloseSubscription(previousCloseSub);
    if (previousClient != null) {
      await previousClient.dispose();
    }
    if (_disposed) {
      return;
    }
    try {
      final client = await _openClient();
      await _bindClient(client);
      if (!_disposed) {
        state = AsyncData(client);
      }
    } on Object catch (error, stackTrace) {
      if (_disposed) {
        return;
      }
      _logger.warning('could not reconnect to host $hostId', error, stackTrace);
      state = AsyncError(error, stackTrace);
      _scheduleRetry(error);
    }
  }

  Future<void> _cancelCloseSubscription(
    StreamSubscription<(Object, StackTrace?)>? subscription,
  ) async {
    if (subscription == null) {
      return;
    }
    try {
      await subscription.cancel().timeout(_connectionCleanupTimeout);
    } on Object catch (error, stackTrace) {
      _logger.warning(
        'runtime event subscription did not cancel cleanly for $hostId',
        error,
        stackTrace,
      );
    }
  }

  void _dispose() {
    _disposed = true;
    _openingAttempt?.cancel();
    _healthyTimer?.cancel();
    _retryTimer?.cancel();
    _retryTimer = null;
    final closeSub = _closeSub;
    _closeSub = null;
    final client = _client;
    _client = null;
    if (closeSub != null) {
      unawaited(closeSub.cancel());
    }
    if (client != null) {
      unawaited(client.dispose());
    }
  }

  void _health(HostConnectionHealth value) {
    scheduleMicrotask(() {
      if (!_disposed && ref.mounted) {
        ref
            .read(hostConnectionHealthControllerProvider(hostId).notifier)
            .update(value);
      }
    });
  }
}
