import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:alera/src/features/workbench/infra/terminal_host/terminal_host_client.dart';
import 'package:alera/src/platform/runtime_host/protocol/terminal_host_protocol.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

Map<String, Object?> _wireFixture(String name) {
  final file = File(p.join('test', 'fixtures', 'wire', name));
  return Map<String, Object?>.from(jsonDecode(file.readAsStringSync()) as Map);
}

final class _WireFixtureServer {
  _WireFixtureServer._(this._listener);

  final ServerSocket _listener;
  final Map<String, _FixtureResponse> _responses = <String, _FixtureResponse>{};
  final List<Map<String, Object?>> requests = <Map<String, Object?>>[];
  final List<Socket> _clients = <Socket>[];
  StreamSubscription<Socket>? _subscription;

  static Future<_WireFixtureServer> start() async {
    final listener = await ServerSocket.bind(InternetAddress.loopbackIPv4, 0);
    final server = _WireFixtureServer._(listener);
    server._subscription = listener.listen(server._accept, onError: (_) {});
    return server;
  }

  int get port => _listener.port;

  void respondWith(
    String type,
    String responseFixture, {
    List<String> pushFixtures = const <String>[],
  }) {
    _responses[type] = _FixtureResponse(
      frame: _wireFixture(responseFixture),
      pushFirst: <Map<String, Object?>>[
        for (final fixture in pushFixtures) _wireFixture(fixture),
      ],
    );
  }

  void respondWithBinary(String type, String frameFixture) {
    final fixture = _wireFixture(frameFixture);
    _responses[type] = _FixtureResponse.binary(
      base64Decode(fixture['frameBase64']! as String),
    );
  }

  void respondWithOutputBeforeBinaryResponse(
    String type,
    String outputFixture,
    String responseFixture,
  ) {
    final output = _wireFixture(outputFixture);
    final response = _wireFixture(responseFixture);
    _responses[type] = _FixtureResponse.interleaved(
      base64Decode(output['frameBase64']! as String),
      base64Decode(response['frameBase64']! as String),
    );
  }

  Map<String, Object?> payloadFor(String type) {
    return Map<String, Object?>.from(
      requests
          .where((request) => request['type'] == type)
          .map((request) => request['payload']! as Map)
          .last,
    );
  }

  void send(String fixtureName) {
    _clients.last.writeln(jsonEncode(_wireFixture(fixtureName)));
  }

  void sendBinaryFrame(String fixtureName) {
    final fixture = _wireFixture(fixtureName);
    _clients.last.add(base64Decode(fixture['frameBase64']! as String));
  }

  void _accept(Socket socket) {
    _clients.add(socket);
    socket
        .cast<List<int>>()
        .transform(utf8.decoder)
        .transform(const LineSplitter())
        .listen(
          (line) => _handleLine(socket, line),
          onError: (_) {},
          onDone: () => _clients.remove(socket),
        );
  }

  void _handleLine(Socket socket, String line) {
    final request = Map<String, Object?>.from(jsonDecode(line) as Map);
    request['payload'] = Map<String, Object?>.from(request['payload'] as Map);
    requests.add(request);
    final response = _responses[request['type']];
    if (response == null) {
      socket.writeln(
        jsonEncode(<String, Object?>{
          'id': request['id'],
          'ok': true,
          'payload': const <String, Object?>{},
        }),
      );
      return;
    }
    if (response.binaryFrame case final frame?) {
      socket.add(frame);
      return;
    }
    if (response.interleavedOutput case final interleaved?) {
      socket.add(interleaved.output);
      socket.add(interleaved.response);
      return;
    }
    for (final push in response.pushFirst) {
      socket.writeln(jsonEncode(push));
    }
    final frame = Map<String, Object?>.from(response.frame!)
      ..['id'] = request['id'];
    socket.writeln(jsonEncode(frame));
  }

  Future<void> dispose() async {
    await _subscription?.cancel();
    for (final socket in _clients) {
      socket.destroy();
    }
    await _listener.close();
  }
}

final class _FixtureResponse {
  _FixtureResponse({required this.frame, required this.pushFirst})
    : binaryFrame = null,
      interleavedOutput = null;

  _FixtureResponse.binary(this.binaryFrame)
    : frame = null,
      pushFirst = const <Map<String, Object?>>[],
      interleavedOutput = null;

  _FixtureResponse.interleaved(List<int> output, List<int> response)
    : frame = null,
      pushFirst = const <Map<String, Object?>>[],
      binaryFrame = null,
      interleavedOutput = _BinaryInterleaving(output, response);

  final Map<String, Object?>? frame;
  final List<Map<String, Object?>> pushFirst;
  final List<int>? binaryFrame;
  final _BinaryInterleaving? interleavedOutput;
}

final class _BinaryInterleaving {
  const _BinaryInterleaving(this.output, this.response);

  final List<int> output;
  final List<int> response;
}

final class _NoopTerminalHostLauncher implements TerminalHostProcessLauncher {
  @override
  Future<void> start({
    required String runtimeDir,
    required String controlFilePath,
    required String token,
    required TerminalHostConfig config,
  }) async {}
}

Future<({SocketTerminalHostClient client, _WireFixtureServer server})>
_connectFixtureClient({bool binaryFrames = false}) async {
  final tempDir = await Directory.systemTemp.createTemp(
    'alera-wire-binary-fixtures-',
  );
  addTearDown(() async {
    if (await tempDir.exists()) {
      await tempDir.delete(recursive: true);
    }
  });
  final server = await _WireFixtureServer.start();
  addTearDown(server.dispose);
  server.respondWith(
    'hello',
    binaryFrames ? 'response.ok.hello.binary.json' : 'response.ok.hello.json',
  );
  final runtimeDir = Directory(p.join(tempDir.path, 'terminal_host'));
  await runtimeDir.create(recursive: true);
  await File(p.join(runtimeDir.path, 'host.json')).writeAsString(
    jsonEncode(<String, Object?>{
      'protocolVersion': aleraTerminalHostProtocolVersion,
      'port': server.port,
      'token': 'token-1',
      'runtimeCapabilities': <String>[
        aleraRuntimeHostCapability,
        aleraRuntimeHostBootstrapCapability,
        aleraRuntimeHostManagedWorkspaceCapability,
        if (binaryFrames) aleraRuntimeHostBinaryFramesCapability,
      ],
    }),
  );
  final client = SocketTerminalHostClient(
    launcher: _NoopTerminalHostLauncher(),
    applicationSupportDirectory: () async => tempDir,
  );
  addTearDown(client.dispose);
  return (client: client, server: server);
}

Future<void> _waitForSessionEvents(
  List<TerminalHostEvent> events,
  int count,
) async {
  final deadline = DateTime.now().add(const Duration(seconds: 5));
  while (events.length < count) {
    if (DateTime.now().isAfter(deadline)) {
      fail('expected $count session events, got ${events.length}');
    }
    await Future<void>.delayed(const Duration(milliseconds: 5));
  }
}

void main() {
  test(
    'binary resync fixtures preserve framed control and output order',
    () async {
      final harness = await _connectFixtureClient(binaryFrames: true);
      await harness.client.runtimeRequest('status.get');
      final events = <TerminalHostEvent>[];
      final subscription = harness.client
          .eventsForSession('s1')
          .listen(events.add);
      addTearDown(subscription.cancel);

      harness.server.send('event.binary_frames_enabled.json');
      harness.server.sendBinaryFrame(
        'frame.output_resync_required.binary.json',
      );
      await _waitForSessionEvents(events, 1);
      expect(events.single, isA<TerminalHostOutputResyncRequiredEvent>());
      expect(events.single.sessionId, 's1');

      harness.server.respondWithBinary(
        'setOutputPaused',
        'frame.output_resumed_snapshot.binary.json',
      );
      final resume = await harness.client.setOutputPaused(
        sessionId: 's1',
        paused: false,
      );
      final resumeFixture = _wireFixture(
        'response.ok.output_resumed_snapshot.binary.json',
      );
      final resumePayload = resumeFixture['payload']! as Map;
      expect(resume.isDelta, isFalse);
      expect(resume.resetInteractionModes, isTrue);
      expect(
        resume.snapshot,
        orderedEquals(base64Decode(resumePayload['snapshotBase64']! as String)),
      );

      harness.server.respondWithOutputBeforeBinaryResponse(
        'configure',
        'terminal.output.binary.resync.json',
        'frame.empty.binary_interleaved.json',
      );
      final configurePayload = await harness.client.runtimeRequest('configure');
      expect(configurePayload, <String, Object?>{});
      await _waitForSessionEvents(events, 2);
      final output = events[1];
      expect(output, isA<TerminalHostOutputTextEvent>());
      expect(
        (output as TerminalHostOutputTextEvent).text,
        utf8.decode(
          base64Decode(
            _wireFixture('terminal.output.binary.resync.json')['dataBase64']!
                as String,
          ),
          allowMalformed: true,
        ),
      );
      expect(harness.server.payloadFor('setOutputPaused'), <String, Object?>{
        'sessionId': 's1',
        'paused': false,
      });
    },
  );

  test(
    'a client without the binary capability keeps the legacy output event',
    () async {
      final harness = await _connectFixtureClient();
      await harness.client.runtimeRequest('status.get');
      final hello = harness.server.requests.firstWhere(
        (request) => request['type'] == 'hello',
      );
      expect((hello['payload']! as Map).containsKey('binaryFrames'), isFalse);
      final events = <TerminalHostEvent>[];
      final subscription = harness.client
          .eventsForSession('s1')
          .listen(events.add);
      addTearDown(subscription.cancel);

      harness.server.send('event.output.legacy.json');
      await _waitForSessionEvents(events, 1);
      expect(events.single, isA<TerminalHostOutputEvent>());
      final fixture = _wireFixture('event.output.legacy.json');
      expect(
        (events.single as TerminalHostOutputEvent).data,
        orderedEquals(
          base64Decode((fixture['payload']! as Map)['dataBase64']! as String),
        ),
      );
    },
  );
}
