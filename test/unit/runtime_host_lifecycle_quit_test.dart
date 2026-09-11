import 'dart:async';

import 'package:alera/src/features/runtime_host/application/runtime_host_lifecycle_service.dart';
import 'package:alera/src/features/runtime_host/domain/runtime_host_quit_decision.dart';
import 'package:alera/src/features/runtime_host/domain/runtime_host_status.dart';
import 'package:alera/src/platform/runtime_host/protocol/terminal_host_protocol.dart';
import 'package:flutter_test/flutter_test.dart';

import 'runtime_host_lifecycle_fakes.dart';

void main() {
  group('RuntimeHostLifecycleService quit', () {
    test(
      'prepareAppQuit skips shutdown when keepRuntimeOpen is true',
      () async {
        final client = FakeRuntimeHostLifecycleClient(
          status: <String, Object?>{
            'runtimeHostVersion': '1.2.0',
            'persistent': false,
          },
        );
        final service = RuntimeHostLifecycleService(
          client: client,
          bundledVersionProbe: FakeBundledSidecarVersionProbe(
            const BundledSidecarVersion(version: '1.2.0'),
          ),
          readConfig: () => TerminalHostConfig.defaults,
        );

        final allowed = await service.prepareAppQuit(keepRuntimeOpen: true);

        expect(allowed, isTrue);
        expect(client.shutdownCalls, isEmpty);
        expect(client.appQuitEvents, <String>['begin', 'commit']);
      },
    );

    test('prepareAppQuit skips shutdown for persistent hosts', () async {
      final client = FakeRuntimeHostLifecycleClient(
        status: <String, Object?>{
          'runtimeHostVersion': '1.2.0',
          'persistent': true,
        },
      );
      final service = RuntimeHostLifecycleService(
        client: client,
        bundledVersionProbe: FakeBundledSidecarVersionProbe(
          const BundledSidecarVersion(version: '1.2.0'),
        ),
        readConfig: () => TerminalHostConfig.defaults,
      );

      final allowed = await service.prepareAppQuit(keepRuntimeOpen: false);

      expect(allowed, isTrue);
      expect(client.shutdownCalls, isEmpty);
    });

    test(
      'prepareAppQuit leaves push-only runtime running and commits visual quit',
      () async {
        final client = FakeRuntimeHostLifecycleClient(
          status: <String, Object?>{
            'runtimeHostVersion': '1.2.0',
            'persistent': false,
            'activePushSubscriptions': 1,
          },
          softStopBusy: const RuntimeHostBusyException(
            message: 'Runtime host has 1 active push subscription(s).',
            activePushSubscriptions: 1,
          ),
        );
        final service = RuntimeHostLifecycleService(
          client: client,
          bundledVersionProbe: FakeBundledSidecarVersionProbe(
            const BundledSidecarVersion(version: '1.2.0'),
          ),
          readConfig: () => TerminalHostConfig.defaults,
        );
        var confirmCalls = 0;
        var visualQuitCommitted = false;

        final allowed = await service.prepareAppQuit(
          keepRuntimeOpen: false,
          confirmBusyQuit:
              ({required String title, required String message}) async {
                confirmCalls += 1;
                return RuntimeHostQuitDecision.cancel;
              },
          onBusyQuitCommitted: () {
            visualQuitCommitted = true;
          },
        );

        expect(allowed, isTrue);
        expect(confirmCalls, 0);
        expect(visualQuitCommitted, isTrue);
        expect(client.shutdownCalls, <bool>[false]);
        expect(await client.probeRuntimeStatus(), isNotNull);
      },
    );

    test('active push does not suppress busy terminal confirmation', () async {
      final client = FakeRuntimeHostLifecycleClient(
        status: <String, Object?>{
          'runtimeHostVersion': '1.2.0',
          'persistent': false,
          'activeSessions': 1,
          'activePushSubscriptions': 1,
        },
        softStopBusy: const RuntimeHostBusyException(
          message: 'Runtime host has 1 active terminal session(s) and 1 active push subscription(s).',
          activeSessions: 1,
          activePushSubscriptions: 1,
        ),
      );
      final service = RuntimeHostLifecycleService(
        client: client,
        bundledVersionProbe: FakeBundledSidecarVersionProbe(
          const BundledSidecarVersion(version: '1.2.0'),
        ),
        readConfig: () => TerminalHostConfig.defaults,
      );
      var confirmCalls = 0;
      var visualQuitCommitted = false;

      final allowed = await service.prepareAppQuit(
        keepRuntimeOpen: false,
        confirmBusyQuit:
            ({required String title, required String message}) async {
              confirmCalls += 1;
              return RuntimeHostQuitDecision.leaveRuntimeOpen;
            },
        onBusyQuitCommitted: () {
          visualQuitCommitted = true;
        },
      );

      expect(allowed, isTrue);
      expect(confirmCalls, 1);
      expect(visualQuitCommitted, isTrue);
      expect(client.shutdownCalls, <bool>[false]);
      expect(await client.probeRuntimeStatus(), isNotNull);
    });

    test('prepareAppQuit soft-stops when status probe fails', () async {
      final client = FakeRuntimeHostLifecycleClient(
        status: <String, Object?>{
          'runtimeHostVersion': '1.2.0',
          'persistent': false,
        },
        probeThrows: true,
      );
      final service = RuntimeHostLifecycleService(
        client: client,
        bundledVersionProbe: FakeBundledSidecarVersionProbe(
          const BundledSidecarVersion(version: '1.2.0'),
        ),
        readConfig: () => TerminalHostConfig.defaults,
        shutdownSettleTimeout: const Duration(milliseconds: 50),
      );

      final allowed = await service.prepareAppQuit(keepRuntimeOpen: false);

      expect(allowed, isTrue);
      expect(client.shutdownCalls, <bool>[false]);
    });

    test('prepareAppQuit soft-stops an idle sidecar', () async {
      final client = FakeRuntimeHostLifecycleClient(
        status: <String, Object?>{
          'runtimeHostVersion': '1.2.0',
          'persistent': false,
        },
      );
      final service = RuntimeHostLifecycleService(
        client: client,
        bundledVersionProbe: FakeBundledSidecarVersionProbe(
          const BundledSidecarVersion(version: '1.2.0'),
        ),
        readConfig: () => TerminalHostConfig.defaults,
        shutdownSettleTimeout: const Duration(milliseconds: 50),
      );

      final allowed = await service.prepareAppQuit(keepRuntimeOpen: false);

      expect(allowed, isTrue);
      expect(client.shutdownCalls, <bool>[false]);
    });

    test('prepareAppQuit does not wait for detached host cleanup', () async {
      final client = FakeRuntimeHostLifecycleClient(
        status: <String, Object?>{
          'runtimeHostVersion': '1.2.0',
          'persistent': false,
        },
        shutdownLeavesHostRunning: true,
      );
      final service = RuntimeHostLifecycleService(
        client: client,
        bundledVersionProbe: FakeBundledSidecarVersionProbe(
          const BundledSidecarVersion(version: '1.2.0'),
        ),
        readConfig: () => TerminalHostConfig.defaults,
        shutdownSettleTimeout: const Duration(seconds: 5),
      );

      final allowed = await service.prepareAppQuit(keepRuntimeOpen: false);

      expect(allowed, isTrue);
      expect(client.shutdownCalls, <bool>[false]);
      expect(await client.probeRuntimeStatus(), isNotNull);
    });

    test('prepareAppQuit cancels when busy quit is declined', () async {
      final client = FakeRuntimeHostLifecycleClient(
        status: <String, Object?>{
          'runtimeHostVersion': '1.2.0',
          'persistent': false,
        },
        busyOnSoftStop: true,
      );
      final service = RuntimeHostLifecycleService(
        client: client,
        bundledVersionProbe: FakeBundledSidecarVersionProbe(
          const BundledSidecarVersion(version: '1.2.0'),
        ),
        readConfig: () => TerminalHostConfig.defaults,
      );

      final allowed = await service.prepareAppQuit(
        keepRuntimeOpen: false,
        confirmBusyQuit: ({
          required String title,
          required String message,
        }) async => RuntimeHostQuitDecision.cancel,
      );

      expect(allowed, isFalse);
      expect(client.shutdownCalls, <bool>[false]);
      expect(client.appQuitEvents, <String>['begin', 'cancel']);
    });

    test('prepareAppQuit cancels transport when shutdown fails', () async {
      final error = StateError('shutdown failed');
      final client = FakeRuntimeHostLifecycleClient(
        status: <String, Object?>{
          'runtimeHostVersion': '1.2.0',
          'persistent': false,
        },
        shutdownErrorOnSoft: error,
      );
      final service = RuntimeHostLifecycleService(
        client: client,
        bundledVersionProbe: FakeBundledSidecarVersionProbe(
          const BundledSidecarVersion(version: '1.2.0'),
        ),
        readConfig: () => TerminalHostConfig.defaults,
      );

      await expectLater(
        service.prepareAppQuit(keepRuntimeOpen: false),
        throwsA(same(error)),
      );

      expect(client.appQuitEvents, <String>['begin', 'cancel']);
    });

    test(
      'prepareAppQuit leaves host running when busy quit chooses leave',
      () async {
        final client = FakeRuntimeHostLifecycleClient(
          status: <String, Object?>{
            'runtimeHostVersion': '1.2.0',
            'persistent': false,
          },
          busyOnSoftStop: true,
        );
        final service = RuntimeHostLifecycleService(
          client: client,
          bundledVersionProbe: FakeBundledSidecarVersionProbe(
            const BundledSidecarVersion(version: '1.2.0'),
          ),
          readConfig: () => TerminalHostConfig.defaults,
        );

        final allowed = await service.prepareAppQuit(
          keepRuntimeOpen: false,
          confirmBusyQuit: ({
            required String title,
            required String message,
          }) async => RuntimeHostQuitDecision.leaveRuntimeOpen,
        );

        expect(allowed, isTrue);
        expect(client.shutdownCalls, <bool>[false]);
        expect(await client.probeRuntimeStatus(), isNotNull);
      },
    );

    test('busy leave commits visual quit before returning', () async {
      final client = FakeRuntimeHostLifecycleClient(
        status: <String, Object?>{
          'runtimeHostVersion': '1.2.0',
          'persistent': false,
        },
        busyOnSoftStop: true,
      );
      final service = RuntimeHostLifecycleService(
        client: client,
        bundledVersionProbe: FakeBundledSidecarVersionProbe(
          const BundledSidecarVersion(version: '1.2.0'),
        ),
        readConfig: () => TerminalHostConfig.defaults,
      );
      var visualQuitCommitted = false;

      final allowed = await service.prepareAppQuit(
        keepRuntimeOpen: false,
        confirmBusyQuit: ({
          required String title,
          required String message,
        }) async => RuntimeHostQuitDecision.leaveRuntimeOpen,
        onBusyQuitCommitted: () {
          visualQuitCommitted = true;
        },
      );

      expect(allowed, isTrue);
      expect(visualQuitCommitted, isTrue);
      expect(client.shutdownCalls, <bool>[false]);
    });

    test('busy cancel does not commit visual quit', () async {
      final client = FakeRuntimeHostLifecycleClient(
        status: <String, Object?>{
          'runtimeHostVersion': '1.2.0',
          'persistent': false,
        },
        busyOnSoftStop: true,
      );
      final service = RuntimeHostLifecycleService(
        client: client,
        bundledVersionProbe: FakeBundledSidecarVersionProbe(
          const BundledSidecarVersion(version: '1.2.0'),
        ),
        readConfig: () => TerminalHostConfig.defaults,
      );
      var visualQuitCommitted = false;

      final allowed = await service.prepareAppQuit(
        keepRuntimeOpen: false,
        confirmBusyQuit: ({
          required String title,
          required String message,
        }) async => RuntimeHostQuitDecision.cancel,
        onBusyQuitCommitted: () {
          visualQuitCommitted = true;
        },
      );

      expect(allowed, isFalse);
      expect(visualQuitCommitted, isFalse);
    });

    test('busy force commits visual quit before slow force shutdown', () async {
      final forceShutdownStarted = Completer<void>();
      final finishForceShutdown = Completer<void>();
      final client = FakeRuntimeHostLifecycleClient(
        status: <String, Object?>{
          'runtimeHostVersion': '1.2.0',
          'persistent': false,
        },
        busyOnSoftStop: true,
        forceShutdownStarted: forceShutdownStarted,
        forceShutdownBarrier: finishForceShutdown.future,
      );
      final service = RuntimeHostLifecycleService(
        client: client,
        bundledVersionProbe: FakeBundledSidecarVersionProbe(
          const BundledSidecarVersion(version: '1.2.0'),
        ),
        readConfig: () => TerminalHostConfig.defaults,
      );
      final visualQuitCommitted = Completer<void>();
      var quitCompleted = false;

      final quit = service
          .prepareAppQuit(
            keepRuntimeOpen: false,
            confirmBusyQuit: ({
              required String title,
              required String message,
            }) async => RuntimeHostQuitDecision.forceStop,
            onBusyQuitCommitted: () {
              visualQuitCommitted.complete();
            },
          )
          .whenComplete(() => quitCompleted = true);

      await visualQuitCommitted.future;
      await forceShutdownStarted.future;
      expect(quitCompleted, isFalse);

      finishForceShutdown.complete();
      expect(await quit, isTrue);
    });

    test('prepareAppQuit force-stops when busy quit chooses force', () async {
      final client = FakeRuntimeHostLifecycleClient(
        status: <String, Object?>{
          'runtimeHostVersion': '1.2.0',
          'persistent': false,
        },
        busyOnSoftStop: true,
      );
      final service = RuntimeHostLifecycleService(
        client: client,
        bundledVersionProbe: FakeBundledSidecarVersionProbe(
          const BundledSidecarVersion(version: '1.2.0'),
        ),
        readConfig: () => TerminalHostConfig.defaults,
        shutdownSettleTimeout: const Duration(milliseconds: 50),
      );

      final allowed = await service.prepareAppQuit(
        keepRuntimeOpen: false,
        confirmBusyQuit: ({
          required String title,
          required String message,
        }) async => RuntimeHostQuitDecision.forceStop,
      );

      expect(allowed, isTrue);
      expect(client.shutdownCalls, <bool>[false, true]);
    });

    test('prepareAppQuit treats a shutdown disconnect as success', () async {
      final client = FakeRuntimeHostLifecycleClient(
        status: <String, Object?>{
          'runtimeHostVersion': '1.2.0',
          'persistent': false,
        },
        shutdownErrorOnSoft: const RuntimeHostLifecycleTransportException(),
      );
      final service = RuntimeHostLifecycleService(
        client: client,
        bundledVersionProbe: FakeBundledSidecarVersionProbe(
          const BundledSidecarVersion(version: '1.2.0'),
        ),
        readConfig: () => TerminalHostConfig.defaults,
      );

      final allowed = await service.prepareAppQuit(keepRuntimeOpen: false);

      expect(allowed, isTrue);
      expect(client.shutdownCalls, <bool>[false]);
    });
  });
}
