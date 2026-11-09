// Headless Dart contract test for the Classic Host session router.
//
// Unlike Flutter test, this script needs no pub get, xterm submodule,
// Native Assets builder or Flutter engine:
//   dart desktop_gpui/tests/terminal_event_hub_smoke.dart
import 'dart:async';

import '../../lib/src/features/workbench/infra/terminal_host/terminal_host_event_hub.dart';

final class Event {
  const Event(this.sessionId, this.kind);
  final String sessionId;
  final String kind;
}

void check(bool condition, String label) {
  if (!condition) {
    throw StateError(label);
  }
}

Future<void> main() async {
  final global = <Event>[];
  final a = <Event>[];
  final oldB = <Event>[];
  final newB = <Event>[];
  final hub = TerminalHostEventHub<Event>(
    onGlobalEvent: global.add,
    onConnectionError: (sessionId, _) => Event(sessionId, 'disconnect'),
  );
  final subA = hub.eventsForSession('a').listen(a.add);
  final subOldB = hub.eventsForSession('b').listen(oldB.add);
  try {
    hub.emit('a', const Event('a', 'first'));
    hub.emit('b', const Event('b', 'first'));
    await Future<void>.delayed(Duration.zero);
    check(
      a.length == 1 && oldB.length == 1 && global.length == 2,
      'per-session delivery must not fan out to unrelated subscribers',
    );

    hub.releaseSession('b');
    final subNewB = hub.eventsForSession('b').listen(newB.add);
    try {
      hub.emit('b', const Event('b', 'new-generation'));
      await Future<void>.delayed(Duration.zero);
      check(
        oldB.length == 1 && newB.length == 1,
        'released subscribers must not receive events for a reused session',
      );

      hub.releaseSession('b');
      hub.emitConnectionError(StateError('socket closed'));
      await Future<void>.delayed(Duration.zero);
      check(
        a.last.kind == 'disconnect' && a.length == 2,
        'live session must receive transport failure',
      );
      check(
        newB.length == 1,
        'released session must not receive transport failure',
      );

      hub.close();
      hub.emit('a', const Event('a', 'late'));
      await Future<void>.delayed(Duration.zero);
      check(
        a.length == 2 && global.length == 4,
        'disposed hub must reject late events',
      );
    } finally {
      await subNewB.cancel();
    }
  } finally {
    await subA.cancel();
    await subOldB.cancel();
    hub.close();
  }
  print('PASS: 4 headless TerminalHostEventHub contracts');
}
