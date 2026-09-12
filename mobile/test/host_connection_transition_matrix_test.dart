import 'dart:async';
import 'dart:io';

import 'package:alera_mobile/src/features/accounts/application/cloud_account_providers.dart';
import 'package:alera_mobile/src/features/accounts/domain/cloud_account_session.dart';
import 'package:alera_mobile/src/features/hosts/application/host_providers.dart';
import 'package:alera_mobile/src/features/hosts/domain/paired_host_profile.dart';
import 'package:alera_mobile/src/features/runtime/application/host_connection_controller.dart';
import 'package:alera_mobile/src/features/runtime/application/host_connection_health.dart';
import 'package:alera_mobile/src/features/runtime/infra/mobile_runtime_client.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/direct_runtime_gateway.dart';
import 'support/memory_cloud_account_repository.dart';
import 'support/memory_host_repository.dart';
import 'support/relay_runtime_gateway.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  test('Direct fallback completes with a relay client', () async {
    final directPort = await _unusedPort();
    final relayGateway = await RelayRuntimeGateway.start();
    final relay = RelayCloudFixture(relayGateway);
    final repository = await _pairedHostRepository(
      'ws://127.0.0.1:$directPort',
    );
    final container = _createContainer(repository, relay: relay);
    final provider = hostConnectionControllerProvider('runtime-1');
    final listener = container.listen(provider, (_, _) {});
    addTearDown(listener.close);
    addTearDown(container.dispose);
    addTearDown(relayGateway.dispose);

    final client = await container.read(provider.future);
    await _waitUntil(
      () =>
          container
              .read(hostConnectionHealthControllerProvider('runtime-1'))
              .transport ==
          'relay',
    );

    expect(client.transport, 'relay');
    expect(client.isConnectionUsable, isTrue);
    expect(relay.discoveryCalls, 1);
    expect(relay.registrationCalls, 1);
    expect(relay.grantCalls, 1);
  });

  test('A relay drop reconnects through the direct endpoint', () async {
    final directPort = await _unusedPort();
    final relayGateway = await RelayRuntimeGateway.start();
    final relay = RelayCloudFixture(relayGateway);
    final repository = await _pairedHostRepository(
      'ws://127.0.0.1:$directPort',
    );
    final container = _createContainer(repository, relay: relay);
    final provider = hostConnectionControllerProvider('runtime-1');
    final listener = container.listen(provider, (_, _) {});
    addTearDown(listener.close);
    addTearDown(container.dispose);
    addTearDown(relayGateway.dispose);

    final firstClient = await container.read(provider.future);
    final directGateway = await DirectRuntimeGateway.start(
      port: directPort,
      response: _directResponse(),
    );
    addTearDown(directGateway.dispose);

    await relayGateway.closeSocket(0);
    await _waitUntil(() {
      final state = container.read(provider);
      return state.hasValue && state.requireValue.transport == 'direct';
    });

    final replacement = container.read(provider).requireValue;
    expect(firstClient.transport, 'relay');
    expect(firstClient.isConnectionUsable, isFalse);
    expect(replacement.transport, 'direct');
    expect(replacement.isConnectionUsable, isTrue);
    expect(
      directGateway.requests.where(
        (request) => request['type'] == 'mobile.hello',
      ),
      hasLength(1),
    );
  });

  test('Runtime restart reports disconnected during an in-flight reconnect', () async {
    final secondHello = Completer<Map<String, Object?>>();
    late DirectRuntimeGateway gateway;
    gateway = await DirectRuntimeGateway.start(
      response: (request, connectionNumber) {
        if (request['type'] == 'mobile.hello' && connectionNumber == 2) {
          if (!secondHello.isCompleted) secondHello.complete(request);
          return null;
        }
        return _directResponse()(request, connectionNumber);
      },
    );
    final repository = await _pairedHostRepository(gateway.endpoint);
    final container = _createContainer(repository);
    final provider = hostConnectionControllerProvider('runtime-1');
    final listener = container.listen(provider, (_, _) {});
    addTearDown(listener.close);
    addTearDown(container.dispose);
    addTearDown(gateway.dispose);

    await container.read(provider.future);
    await gateway.closeSocket(0);
    final stalledHello = await secondHello.future.timeout(
      const Duration(seconds: 5),
    );
    final stateWhileOpening = container.read(provider);
    expect(stateWhileOpening.isLoading, isTrue);
    // AsyncNotifier retains the previous value while loading, but that value
    // is the client the reconnect path already disposed; restart must refuse
    // it rather than reuse it.
    expect(stateWhileOpening.hasValue, isTrue);

    final controller = container.read(provider.notifier);
    await expectLater(
      controller.restartRuntime(),
      throwsA(isA<StateError>()),
    );

    gateway.reply(1, stalledHello, _directPayload(stalledHello));
    await _waitUntil(() {
      final state = container.read(provider);
      return state.hasValue &&
          !state.isLoading &&
          state.requireValue.isConnectionUsable;
    });
  });

  test('Disposing during opening cancels the client and retry work', () async {
    final helloReceived = Completer<void>();
    final gateway = await DirectRuntimeGateway.start(
      response: (request, _) {
        if (request['type'] == 'mobile.hello' && !helloReceived.isCompleted) {
          helloReceived.complete();
        }
        return null;
      },
    );
    final repository = await _pairedHostRepository(gateway.endpoint);
    final container = _createContainer(repository);
    final provider = hostConnectionControllerProvider('runtime-1');
    final listener = container.listen(provider, (_, _) {});
    addTearDown(listener.close);
    addTearDown(gateway.dispose);

    final opening = container.read(provider.future);
    final openingFailure = expectLater(
      opening,
      throwsA(isA<TimeoutException>()),
    );
    await helloReceived.future.timeout(const Duration(seconds: 5));
    container.dispose();

    await openingFailure;
    await gateway.waitForSocketClose(0).timeout(const Duration(seconds: 5));
    await Future<void>.delayed(const Duration(milliseconds: 200));
    expect(gateway.sockets, hasLength(1));
  });

  test('A stale relay renewal cannot replace a new direct client', () async {
    final directPort = await _unusedPort();
    final relayGateway = await RelayRuntimeGateway.start(supportsRenewal: true);
    final relay = RelayCloudFixture(
      relayGateway,
      delayRenewal: true,
      initialGrantSeconds: 31,
    );
    final repository = await _pairedHostRepository(
      'ws://127.0.0.1:$directPort',
    );
    final container = _createContainer(repository, relay: relay);
    final provider = hostConnectionControllerProvider('runtime-1');
    final listener = container.listen(provider, (_, _) {});
    addTearDown(listener.close);
    addTearDown(container.dispose);
    addTearDown(relayGateway.dispose);

    final relayClient = await container.read(provider.future);
    await relay.renewalGrantRequested.future.timeout(
      const Duration(seconds: 5),
    );
    final directGateway = await DirectRuntimeGateway.start(
      port: directPort,
      response: _directResponse(),
    );
    addTearDown(directGateway.dispose);

    await container.read(provider.notifier).reconnectNow();
    final directClient = container.read(provider).requireValue;
    expect(relayClient.isConnectionUsable, isFalse);
    expect(directClient.transport, 'direct');
    expect(directClient.isConnectionUsable, isTrue);

    relay.completeRenewal();
    await Future<void>.delayed(const Duration(milliseconds: 200));
    final state = container.read(provider);
    expect(state.hasError, isFalse);
    expect(state.requireValue, same(directClient));
  });

  test(
    'A late runtime restart completion cannot overwrite a replacement',
    () async {
      late DirectRuntimeGateway gateway;
      gateway = await DirectRuntimeGateway.start(
        response: (request, connectionNumber) {
          if (request['type'] == 'mobile.hello') {
            return <String, Object?>{
              'runtimeCapabilities': <String>[runtimeHostRestartCapability],
            };
          }
          if (request['type'] == 'host.restart') {
            unawaited(_closeLater(gateway, connectionNumber - 1));
            return <String, Object?>{
              'restarting': true,
              'forced':
                  request['payload'] is Map<String, Object?> &&
                  (request['payload']! as Map<String, Object?>)['force'] ==
                      true,
              'activeSessions': 0,
              'activeJobs': 0,
              'activeAgents': 0,
              'activePushSubscriptions': 0,
            };
          }
          return <String, Object?>{};
        },
      );
      final repository = await _pairedHostRepository(gateway.endpoint);
      final container = _createContainer(repository);
      final provider = hostConnectionControllerProvider('runtime-1');
      final states = <AsyncValue<MobileRuntimeClient>>[];
      final listener = container.listen(
        provider,
        (_, next) => states.add(next),
      );
      addTearDown(listener.close);
      addTearDown(container.dispose);
      addTearDown(gateway.dispose);

      final oldClient = await container.read(provider.future);
      final result = await container.read(provider.notifier).restartRuntime();
      expect(result.forced, isFalse);

      await _waitUntil(() {
        final state = container.read(provider);
        return state.hasValue && !identical(state.requireValue, oldClient);
      });
      await Future<void>.delayed(const Duration(milliseconds: 100));

      final state = container.read(provider);
      expect(oldClient.isConnectionUsable, isFalse);
      expect(state.hasError, isFalse);
      expect(states.where((value) => value.hasError), isEmpty);
      expect(
        gateway.requests.where((request) => request['type'] == 'mobile.hello'),
        hasLength(2),
      );
    },
  );
}

ProviderContainer _createContainer(
  MemoryHostRepository repository, {
  RelayCloudFixture? relay,
}) {
  return ProviderContainer(
    overrides: [
      hostRepositoryProvider.overrideWithValue(repository),
      cloudAccountRepositoryProvider.overrideWithValue(
        MemoryCloudAccountRepository(
          relay == null
              ? const <CloudAccountSession>[]
              : <CloudAccountSession>[_session()],
        ),
      ),
      if (relay != null) ...[
        cloudRelayIdentityRepositoryProvider.overrideWithValue(relay),
        aleraRelayCloudApiProvider.overrideWithValue(relay),
      ],
    ],
  );
}

Future<MemoryHostRepository> _pairedHostRepository(String endpoint) async {
  final repository = MemoryHostRepository();
  await repository.savePairedHost(
    PairedHostProfile(
      id: 'runtime-1',
      displayName: 'Alera Host',
      endpoint: endpoint,
      runtimeId: 'runtime-1',
      deviceId: 'device-1',
      pairedAt: DateTime.now().toUtc(),
    ),
    'token-1',
  );
  return repository;
}

CloudAccountSession _session() => CloudAccountSession(
  account: const CloudAccountProfile(
    id: 'account-1',
    email: 'owner@example.com',
  ),
  accessToken: 'access',
  refreshToken: 'refresh',
  accessTokenExpiresAt: DateTime.now().toUtc().add(const Duration(hours: 1)),
);

DirectGatewayResponse _directResponse() {
  return (request, _) => _directPayload(request);
}

Map<String, Object?> _directPayload(Map<String, Object?> request) {
  if (request['type'] == 'mobile.hello') {
    return const <String, Object?>{'runtimeCapabilities': <String>[]};
  }
  return const <String, Object?>{};
}

Future<void> _closeLater(DirectRuntimeGateway gateway, int index) async {
  await Future<void>.delayed(const Duration(milliseconds: 50));
  try {
    await gateway.closeSocket(index);
  } on Object {
    // The controller may close the old socket before the delayed server event.
  }
}

Future<int> _unusedPort() async {
  final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
  final port = server.port;
  await server.close(force: true);
  return port;
}

Future<void> _waitUntil(bool Function() condition) async {
  final deadline = DateTime.now().add(const Duration(seconds: 5));
  while (!condition()) {
    if (DateTime.now().isAfter(deadline)) {
      throw TimeoutException('Condition was not reached.');
    }
    await Future<void>.delayed(const Duration(milliseconds: 10));
  }
}
