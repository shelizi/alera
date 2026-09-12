import 'dart:async';

import 'package:alera_mobile/src/features/accounts/application/cloud_account_providers.dart';
import 'package:alera_mobile/src/features/hosts/application/host_providers.dart';
import 'package:alera_mobile/src/features/hosts/application/paired_hosts_controller.dart';
import 'package:alera_mobile/src/features/hosts/domain/paired_host_profile.dart';
import 'package:alera_mobile/src/features/runtime/application/remote_runtime_connection_controller.dart';
import 'package:alera_mobile/src/features/runtime/domain/connection_attempt.dart';
import 'package:alera_mobile/src/features/runtime/infra/mobile_runtime_client.dart';
import 'package:riverpod_annotation/riverpod_annotation.dart';

class HostConnectionClientFactory({
  required final Ref ref,
  required final String hostId,
}) {
  Future<MobileRuntimeClient> openWithin(ConnectionAttempt attempt) async {
    final hosts = await ref.read(pairedHostsControllerProvider.future);
    attempt.check();
    final host = hosts.where((host) => host.id == hostId).firstOrNull;
    if (host != null) {
      try {
        return await _openPairedClient(host);
      } on Object catch (error, stackTrace) {
        if (!isRelayFallbackTransportFailure(error)) {
          rethrow;
        }
        final accountId = await _findRemoteAccountId();
        attempt.check();
        if (accountId == null) {
          Error.throwWithStackTrace(error, stackTrace);
        }
        return connectRuntimeThroughRelay(ref, accountId, hostId);
      }
    }
    final accountId = await _findRemoteAccountId();
    attempt.check();
    if (accountId == null) {
      throw StateError('Host is not paired or available remotely.');
    }
    return connectRuntimeThroughRelay(ref, accountId, hostId);
  }

  Future<String?> _findRemoteAccountId() async {
    final hosts = await ref.read(availableHostsProvider.future);
    return hosts
        .where((host) => host.runtimeId == hostId)
        .firstOrNull
        ?.accountId;
  }

  Future<MobileRuntimeClient> _openPairedClient(PairedHostProfile host) async {
    final attempt = ConnectionAttempt.current;
    final deviceToken = await ref
        .read(hostRepositoryProvider)
        .readDeviceToken(hostId);
    attempt?.check();
    if (deviceToken == null || deviceToken.trim().isEmpty) {
      throw StateError('Device token is missing.');
    }
    final cloudDeviceId = await ref
        .read(cloudAccountRepositoryProvider)
        .getOrCreateInstallationId();
    attempt?.check();
    final client = await MobileRuntimeClient.connect(
      host.endpoint,
      connectTimeout: const Duration(seconds: 3),
    );
    if (attempt != null) {
      unawaited(attempt.cancelled.then((_) => client.dispose()));
    }
    try {
      await client.authenticate(
        deviceId: host.deviceId,
        deviceToken: deviceToken,
        cloudDeviceId: cloudDeviceId,
      );
    } on Object {
      await client.dispose();
      rethrow;
    }
    return client;
  }
}
