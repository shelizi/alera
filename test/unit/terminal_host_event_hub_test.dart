import 'dart:async';

import 'package:test/test.dart';

import '../../lib/src/features/workbench/infra/terminal_host/terminal_host_event_hub.dart';

// The event router has no Flutter or actual Host/socket dependency.
typedef TerminalHostEvent = _TestEvent;
typedef TerminalHostExitEvent = _TestExit;
typedef TerminalHostErrorEvent = _TestError;

abstract class _TestEvent {
  const _TestEvent(this.sessionId);
  final String sessionId;
}

final class _TestExit extends _TestEvent {
  const _TestExit(super.sessionId, this.exitCode);
  final int exitCode;
}

final class _TestError extends _TestEvent {
  const _TestError(super.sessionId, this.error);
  final Object error;
}

TerminalHostEventHub<TerminalHostEvent> makeHub(
  List<TerminalHostEvent> global,
) => TerminalHostEventHub<TerminalHostEvent>(
  onGlobalEvent: global.add,
  onConnectionError: (sessionId, error) =>
      TerminalHostErrorEvent(sessionId, error),
);

void main() {
  test(
    'each Host output reaches only its session, with one global copy',
    () async {
      final global = <TerminalHostEvent>[];
      final a = <TerminalHostEvent>[];
      final b = <TerminalHostEvent>[];
      final hub = makeHub(global);
      final subA = hub.eventsForSession('session-a').listen(a.add);
      final subB = hub.eventsForSession('session-b').listen(b.add);
      addTearDown(() async {
        await subA.cancel();
        await subB.cancel();
        hub.close();
      });

      final eventA = TerminalHostExitEvent('session-a', 0);
      final eventB = TerminalHostExitEvent('session-b', 1);
      hub.emit('session-a', eventA);
      hub.emit('session-b', eventB);
      await Future<void>.delayed(Duration.zero);

      expect(a, <TerminalHostEvent>[eventA]);
      expect(b, <TerminalHostEvent>[eventB]);
      expect(global, <TerminalHostEvent>[eventA, eventB]);
    },
  );

  test(
    'release invalidates stale listeners before reused session ID',
    () async {
      final global = <TerminalHostEvent>[];
      final old = <TerminalHostEvent>[];
      final replacement = <TerminalHostEvent>[];
      final hub = makeHub(global);
      final oldSubscription = hub.eventsForSession('session-a').listen(old.add);
      hub.releaseSession('session-a');
      final newSubscription = hub
          .eventsForSession('session-a')
          .listen(replacement.add);
      addTearDown(() async {
        await oldSubscription.cancel();
        await newSubscription.cancel();
        hub.close();
      });

      final event = TerminalHostExitEvent('session-a', 2);
      hub.emit('session-a', event);
      await Future<void>.delayed(Duration.zero);

      expect(old, isEmpty);
      expect(replacement, <TerminalHostEvent>[event]);
      expect(global, <TerminalHostEvent>[event]);
    },
  );

  test('disconnect affects only currently registered sessions', () async {
    final global = <TerminalHostEvent>[];
    final active = <TerminalHostEvent>[];
    final removed = <TerminalHostEvent>[];
    final hub = makeHub(global);
    final activeSub = hub.eventsForSession('active').listen(active.add);
    final removedSub = hub.eventsForSession('removed').listen(removed.add);
    hub.releaseSession('removed');
    addTearDown(() async {
      await activeSub.cancel();
      await removedSub.cancel();
      hub.close();
    });

    hub.emitConnectionError(StateError('disconnected'));
    await Future<void>.delayed(Duration.zero);

    expect(active, hasLength(1));
    expect(active.single, isA<TerminalHostErrorEvent>());
    expect(active.single.sessionId, 'active');
    expect(removed, isEmpty);
    expect(global, hasLength(1));
  });

  test(
    'closed hub ignores later events without recreating subscriptions',
    () async {
      final global = <TerminalHostEvent>[];
      final hub = makeHub(global);
      hub.close();

      final received = <TerminalHostEvent>[];
      final subscription = hub
          .eventsForSession('session-a')
          .listen(received.add);
      addTearDown(subscription.cancel);
      hub.emit('session-a', TerminalHostExitEvent('session-a', 0));
      hub.emitConnectionError(StateError('late'));
      await Future<void>.delayed(Duration.zero);
      expect(received, isEmpty);
      expect(global, isEmpty);
    },
  );
}
