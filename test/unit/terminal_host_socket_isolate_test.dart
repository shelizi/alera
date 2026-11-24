import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:isolate';
import 'dart:typed_data';

import 'package:alera/src/platform/runtime_host/transport/terminal_host_frame_codec.dart';
import 'package:alera/src/platform/runtime_host/transport/terminal_host_socket_isolate.dart';
import 'package:flutter_test/flutter_test.dart';

/// Drives the isolate against a real loopback socket and collects what it
/// sends back to the main isolate.
class _IsolateHarness(
  final ServerSocket server,
  final StreamController<List<Object?>> messages,
  final Future<Socket> ready,
);

Future<_IsolateHarness> _startIsolate() async {
  final server = await ServerSocket.bind(InternetAddress.loopbackIPv4, 0);
  final accepted = server.first;
  final messages = StreamController<List<Object?>>.broadcast();
  final fromIsolate = ReceivePort()
    ..listen((Object? message) {
      if (message is List) {
        messages.add(message);
      }
    });
  await Isolate.spawn<TerminalHostSocketIsolateConfig>(
    terminalHostSocketIsolateMain,
    TerminalHostSocketIsolateConfig(
      host: InternetAddress.loopbackIPv4.address,
      port: server.port,
      toMain: fromIsolate.sendPort,
      connectTimeoutMillis: 2000,
    ),
    debugName: 'alera-terminal-host-socket-test',
  );
  addTearDown(() async {
    fromIsolate.close();
    await messages.close();
    await server.close();
  });
  return _IsolateHarness(server, messages, accepted);
}

String _sentinelLine() {
  return '${jsonEncode(<String, Object?>{'event': terminalHostBinaryFramesEnabledLine, 'payload': const <String, Object?>{}})}\n';
}

TerminalHostOutputFrame _outputFrame(String sessionId, String text) =>
    TerminalHostOutputFrame(sessionId, utf8.encode(text));

List<List<Object?>> _expandOutput(List<Object?> message) {
  if (message.first != terminalHostIsolateOutputBatch) return [message];
  return [
    for (var i = 1; i < message.length; i += 2)
      <Object?>[terminalHostIsolateOutput, message[i], message[i + 1]],
  ];
}

void main() {
  group('bounded socket isolate output envelopes', () {
    test('2048 frames need only 32 port messages without changing events', () {
      final messages = <List<Object?>>[];
      final dispatcher = TerminalHostSocketFrameDispatcher(messages.add);
      const count = 2048;
      final frames = [
        for (var i = 0; i < count; i++)
          _outputFrame('session-${i % 4}', '繁體中文😀 $i\r\n'),
      ];
      dispatcher.dispatch(frames);
      expect(messages, hasLength(32));
      expect(
        messages.every(
          (message) =>
              message.first == terminalHostIsolateOutputBatch &&
              message.length ==
                  1 + 2 * TerminalHostSocketFrameDispatcher.maxBatchEvents,
        ),
        isTrue,
      );
      final expanded = messages.expand(_expandOutput).toList();
      expect(expanded, hasLength(count));
      for (var i = 0; i < count; i++) {
        expect(expanded[i], <Object?>[
          terminalHostIsolateOutput,
          'session-${i % 4}',
          '繁體中文😀 $i\r\n',
        ]);
      }
    });

    test('control frames are barriers and each read flushes immediately', () {
      final messages = <List<Object?>>[];
      final dispatcher = TerminalHostSocketFrameDispatcher(messages.add);
      dispatcher.dispatch([
        _outputFrame('a', 'before-1'),
        _outputFrame('b', 'before-2'),
        const TerminalHostJsonFrame(
          '{"event":"outputResyncRequired","payload":{"sessionId":"a"}}',
        ),
        _outputFrame('a', 'after-1'),
        _outputFrame('a', 'after-2'),
      ]);
      expect(messages, [
        [terminalHostIsolateOutputBatch, 'a', 'before-1', 'b', 'before-2'],
        [
          terminalHostIsolateLine,
          {
            'event': 'outputResyncRequired',
            'payload': {'sessionId': 'a'},
          },
        ],
        [terminalHostIsolateOutputBatch, 'a', 'after-1', 'a', 'after-2'],
      ]);
      dispatcher.dispatch([_outputFrame('a', 'interactive')]);
      expect(messages.last, [terminalHostIsolateOutput, 'a', 'interactive']);
      expect(messages, hasLength(4));
    });

    test(
      'characters cap batches and large individual frames remain intact',
      () {
        final messages = <List<Object?>>[];
        final dispatcher = TerminalHostSocketFrameDispatcher(messages.add);
        const limit = TerminalHostSocketFrameDispatcher.maxBatchChars;
        final half = 'x' * (limit ~/ 2);
        final oversized = 'z' * (limit + 1);
        dispatcher.dispatch([
          _outputFrame('a', half),
          _outputFrame('b', half),
          _outputFrame('a', 'small'),
          _outputFrame('a', oversized),
          _outputFrame('b', 'last'),
        ]);
        expect(messages, [
          [terminalHostIsolateOutputBatch, 'a', half, 'b', half],
          [terminalHostIsolateOutput, 'a', 'small'],
          [terminalHostIsolateOutput, 'a', oversized],
          [terminalHostIsolateOutput, 'b', 'last'],
        ]);
      },
    );

    test(
      'split UTF-8 state survives batches independently for each session',
      () {
        final messages = <List<Object?>>[];
        final dispatcher = TerminalHostSocketFrameDispatcher(messages.add);
        final chinese = utf8.encode('中');
        final emoji = utf8.encode('😀');
        dispatcher.dispatch([
          TerminalHostOutputFrame('a', chinese.sublist(0, 1)),
          TerminalHostOutputFrame('b', emoji.sublist(0, 2)),
          _outputFrame('c', 'first'),
        ]);
        expect(messages, [
          [terminalHostIsolateOutput, 'c', 'first'],
        ]);
        dispatcher.dispatch([
          TerminalHostOutputFrame('b', emoji.sublist(2)),
          TerminalHostOutputFrame('a', chinese.sublist(1)),
          _outputFrame('c', 'last'),
        ]);
        expect(messages.last, [
          terminalHostIsolateOutputBatch,
          'b',
          '😀',
          'a',
          '中',
          'c',
          'last',
        ]);
      },
    );
  });

  test('loopback burst preserves every frame and control barrier', () async {
    final harness = await _startIsolate();
    final socket = await harness.ready;
    final messages = <List<Object?>>[];
    final done = Completer<void>();
    final sub = harness.messages.stream.listen((message) {
      messages.add(message);
      if (message.first == terminalHostIsolateLine &&
          message[1] is Map &&
          (message[1] as Map)['id'] == 99 &&
          !done.isCompleted) {
        done.complete();
      }
    });
    addTearDown(sub.cancel);
    addTearDown(socket.destroy);

    const count = 130;
    final wire = BytesBuilder(copy: false)..add(utf8.encode(_sentinelLine()));
    for (var i = 0; i < count; i++) {
      if (i == 65) {
        wire.add(encodeTerminalHostJsonFrame('{"event":"barrier"}'));
      }
      wire.add(
        encodeTerminalHostOutputFrame(
          'session-${i % 2}',
          utf8.encode('中文😀 $i'),
        ),
      );
    }
    wire.add(encodeTerminalHostJsonFrame('{"id":99,"ok":true}'));
    socket.add(wire.takeBytes());
    await socket.flush();
    await done.future.timeout(const Duration(seconds: 5));

    final actual = messages
        .where(
          (message) =>
              message.first == terminalHostIsolateOutput ||
              message.first == terminalHostIsolateOutputBatch ||
              (message.first == terminalHostIsolateLine &&
                  message[1] is Map &&
                  (message[1] as Map)['event'] == 'barrier'),
        )
        .expand(_expandOutput)
        .toList();
    expect(actual, [
      for (var i = 0; i < count; i++) ...[
        if (i == 65)
          [
            terminalHostIsolateLine,
            {'event': 'barrier'},
          ],
        [terminalHostIsolateOutput, 'session-${i % 2}', '中文😀 $i'],
      ],
    ]);
  });

  test('reader reports a protocol fault before closing the socket', () async {
    final harness = await _startIsolate();
    final socket = await harness.ready;
    final seen = <List<Object?>>[];
    final closed = Completer<void>();
    final sub = harness.messages.stream.listen((message) {
      seen.add(message);
      if (message.first == terminalHostIsolateClosed && !closed.isCompleted) {
        closed.complete();
      }
    });
    addTearDown(sub.cancel);

    socket.write(_sentinelLine());
    socket.add(<int>[1, 0x7f, 0xff, 0xff, 0xff]);
    await closed.future.timeout(const Duration(seconds: 5));

    expect(
      seen.any(
        (event) =>
            event.length == 2 &&
            event[0] == terminalHostIsolateError &&
            event[1] == 'protocol',
      ),
      isTrue,
    );
  });

  test('delivers control lines before the upgrade', () async {
    final harness = await _startIsolate();
    final socket = await harness.ready;
    final line = harness.messages.stream
        .where((message) => message.first == terminalHostIsolateLine)
        .first;

    socket.write('{"id":1,"ok":true}\n');

    // Parsed here, not on the UI isolate: an attach reply carries the whole
    // scrollback base64'd inside it.
    expect((await line)[1], <String, Object?>{'id': 1, 'ok': true});
  });

  test('passes a line it cannot parse through untouched', () async {
    final harness = await _startIsolate();
    final socket = await harness.ready;
    final line = harness.messages.stream
        .where((message) => message.first == terminalHostIsolateLine)
        .first;

    socket.write('not json\n');

    // The owner reports a malformed line the way it always did.
    expect((await line)[1], 'not json');
  });

  test('decodes output off the main isolate once framed', () async {
    final harness = await _startIsolate();
    final socket = await harness.ready;
    final output = harness.messages.stream
        .where((message) => message.first == terminalHostIsolateOutput)
        .first;

    socket.write(_sentinelLine());
    socket.add(
      encodeTerminalHostOutputFrame('session-1', utf8.encode('hola ñ')),
    );

    final message = await output;
    expect(message[1], 'session-1');
    // Text, not bytes: the UI isolate gets something it can hand to xterm.
    expect(message[2], 'hola ñ');
  });

  test('reassembles a code point split across frames', () async {
    // The reason the decoder has to live in the isolate: it is the only place
    // that sees every chunk of a session in order.
    final harness = await _startIsolate();
    final socket = await harness.ready;
    final collected = StringBuffer();
    final done = Completer<void>();
    harness.messages.stream
        .where((message) => message.first == terminalHostIsolateOutput)
        .listen((message) {
          collected.write(message[2] as String);
          if (collected.toString().endsWith('!') && !done.isCompleted) {
            done.complete();
          }
        });

    socket.write(_sentinelLine());
    final encoded = utf8.encode('ñ');
    socket.add(
      encodeTerminalHostOutputFrame('session-1', <int>[encoded.first]),
    );
    socket.add(
      encodeTerminalHostOutputFrame('session-1', <int>[encoded.last, 0x21]),
    );
    await done.future.timeout(const Duration(seconds: 5));

    expect(collected.toString(), 'ñ!');
  });

  test('reports closure when the peer goes away', () async {
    final harness = await _startIsolate();
    final socket = await harness.ready;
    final closed = harness.messages.stream
        .where((message) => message.first == terminalHostIsolateClosed)
        .first;

    await socket.close();
    socket.destroy();

    await expectLater(closed, completes);
  });

  test('a refused connection reports an error, not a hang', () async {
    final probe = await ServerSocket.bind(InternetAddress.loopbackIPv4, 0);
    final deadPort = probe.port;
    await probe.close();
    final messages = StreamController<List<Object?>>.broadcast();
    final fromIsolate = ReceivePort()
      ..listen((Object? message) {
        if (message is List) {
          messages.add(message);
        }
      });
    addTearDown(() async {
      fromIsolate.close();
      await messages.close();
    });
    final error = messages.stream
        .where((message) => message.first == terminalHostIsolateError)
        .first;

    await Isolate.spawn<TerminalHostSocketIsolateConfig>(
      terminalHostSocketIsolateMain,
      TerminalHostSocketIsolateConfig(
        host: InternetAddress.loopbackIPv4.address,
        port: deadPort,
        toMain: fromIsolate.sendPort,
        connectTimeoutMillis: 1000,
      ),
      debugName: 'alera-terminal-host-socket-dead',
    );

    await expectLater(error, completes);
  });
}
