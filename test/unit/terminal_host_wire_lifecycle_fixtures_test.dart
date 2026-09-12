import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:alera/src/features/workbench/infra/terminal_host/terminal_host_client.dart';
import 'package:alera/src/platform/runtime_host/protocol/terminal_host_protocol.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

/// The host-lifecycle fixtures under `test/fixtures/wire/` are the shared
/// contract: `wire_fixture_tests_lifecycle.rs` in `rust/alera-cli` produces
/// them through the real terminal host, and this suite decodes the same
/// files through [SocketTerminalHostClient].
Map<String, Object?> _wireFixture(String name) {
  final file = File(p.join('test', 'fixtures', 'wire', name));
  return Map<String, Object?>.from(jsonDecode(file.readAsStringSync()) as Map);
}

/// A loopback socket server that replays fixture frames verbatim, rewriting
/// only the response `id` to match the request it answers.
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

  Map<String, Object?> payloadFor(String type) {
    return Map<String, Object?>.from(
      requests
          .where((request) => request['type'] == type)
          .map((request) => request['payload']! as Map)
          .last,
    );
  }

  /// Answers [type] with [responseFixture].
  void respondWith(String type, String responseFixture) {
    _responses[type] = _FixtureResponse(_wireFixture(responseFixture));
  }

  /// Answers [type] with an in-memory [frame] instead of a fixture file,
  /// used to inject fields no fixture carries.
  void respondWithFrame(String type, Map<String, Object?> frame) {
    _responses[type] = _FixtureResponse(frame);
  }

  /// Sends a JSON fixture frame as one newline-delimited line.
  void send(String fixtureName) {
    _clients.last.writeln(jsonEncode(_wireFixture(fixtureName)));
  }

  /// Sends the raw bytes of `frameBase64` in [fixtureName] without any line
  /// framing, exercising the binary-frame reader path.
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
    if (response != null) {
      final frame = Map<String, Object?>.from(response.frame)
        ..['id'] = request['id'];
      socket.writeln(jsonEncode(frame));
      return;
    }
    socket.writeln(
      jsonEncode(<String, Object?>{
        'id': request['id'],
        'ok': true,
        'payload': const <String, Object?>{},
      }),
    );
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
  const _FixtureResponse(this.frame);
  final Map<String, Object?> frame;
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
    'alera-wire-lifecycle-fixtures-',
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

void main() {
  test('shutdownRuntime decodes the shared shutdown reply', () async {
    final harness = await _connectFixtureClient();
    harness.server.respondWith(
      'host.shutdown',
      'response.ok.host_shutdown.json',
    );
    // Establish the connection first; shutdownRuntime refuses to launch.
    await harness.client.runtimeRequest('status.get');

    final result = await harness.client.shutdownRuntime();

    expect(
      harness.server.payloadFor('host.shutdown'),
      _wireFixture('request.host_shutdown.json')['payload'],
    );
    expect(result.stopped, isTrue);
    expect(result.forced, isFalse);
    expect(result.activeSessions, 0);
    expect(result.activeAgents, 0);
  });

  test('shutdownRuntime decodes the forced and minimal replies', () async {
    final harness = await _connectFixtureClient();
    harness.server.respondWith(
      'host.shutdown',
      'response.ok.host_shutdown.force.json',
    );
    await harness.client.runtimeRequest('status.get');

    final forced = await harness.client.shutdownRuntime(force: true);

    expect(
      harness.server.payloadFor('host.shutdown'),
      _wireFixture('request.host_shutdown.force.json')['payload'],
    );
    expect(forced.stopped, isTrue);
    expect(forced.forced, isTrue);
    expect(forced.activeSessions, 1);

    harness.server.respondWith(
      'host.shutdown',
      'response.ok.host_shutdown.minimal.json',
    );
    // The minimal pin is what a pre-activity-count host sent: the missing
    // counters must fall back to zero instead of breaking the decode.
    final minimal = await harness.client.shutdownRuntime(force: true);
    expect(minimal.stopped, isTrue);
    expect(minimal.forced, isTrue);
    expect(minimal.activeSessions, 0);
  });

  test('host.restart round-trips the shared request and response', () async {
    final harness = await _connectFixtureClient();
    harness.server.respondWith('host.restart', 'response.ok.host_restart.json');
    final requestFixture = _wireFixture('request.host_restart.json');

    final payload = await harness.client.runtimeRequest(
      requestFixture['type']! as String,
      Map<String, Object?>.from(requestFixture['payload']! as Map),
    );

    expect(
      harness.server.payloadFor('host.restart'),
      requestFixture['payload'],
    );
    final response = _wireFixture('response.ok.host_restart.json');
    expect(payload, response['payload']);
  });

  test('a response carrying unknown fields still decodes', () async {
    final harness = await _connectFixtureClient();
    // The request fixture pins the host tolerating unknown request fields;
    // the symmetric client contract is that unknown response fields are
    // ignored, so a newer host cannot break an older client.
    final fixture = _wireFixture('request.unknown_fields.json');
    final response = <String, Object?>{
      ..._wireFixture('response.ok.empty.json'),
      'extraTopLevel': fixture['extraTopLevel'],
      'payload': <String, Object?>{
        'futureField': (fixture['payload']! as Map)['futureField'],
      },
    };
    harness.server.respondWithFrame('tab.find', response);

    final payload = await harness.client.runtimeRequest('tab.find');

    expect(payload, <String, Object?>{
      'futureField': <String, Object?>{'nested': true},
    });
  });

  test('binary frame fixtures surface as a session output event', () async {
    final harness = await _connectFixtureClient(binaryFrames: true);
    // Handshake first so the connection exists before the server pushes.
    await harness.client.runtimeRequest('status.get');
    final events = <TerminalHostEvent>[];
    final subscription = harness.client
        .eventsForSession('s1')
        .listen(events.add);
    addTearDown(subscription.cancel);

    harness.server.send('event.binary_frames_enabled.json');
    harness.server.sendBinaryFrame('terminal.output.binary.json');
    final deadline = DateTime.now().add(const Duration(seconds: 5));
    while (events.isEmpty) {
      if (DateTime.now().isAfter(deadline)) {
        fail('expected a session output event');
      }
      await Future<void>.delayed(const Duration(milliseconds: 5));
    }

    // Binary output frames are decoded to text inside the socket reader
    // isolate, so the event surfaces as TerminalHostOutputTextEvent with
    // malformed bytes mapped to U+FFFD.
    final output = events.single;
    expect(output, isA<TerminalHostOutputTextEvent>());
    expect(output.sessionId, 's1');
    final fixture = _wireFixture('terminal.output.binary.json');
    expect(
      (output as TerminalHostOutputTextEvent).text,
      utf8.decode(
        base64Decode(fixture['dataBase64']! as String),
        allowMalformed: true,
      ),
    );
  });

  test(
    'mobile error fixtures decode to StateError with the wire message',
    () async {
      final harness = await _connectFixtureClient();
      harness.server.respondWith(
        'host.shutdown',
        'response.error.mobile_shutdown_denied.json',
      );
      harness.server.respondWith(
        'mobile.hello',
        'response.error.mobile_version_mismatch.json',
      );

      await expectLater(
        harness.client.runtimeRequest('host.shutdown'),
        throwsA(
          isA<StateError>().having(
            (error) => error.message,
            'message',
            _wireFixture('response.error.mobile_shutdown_denied.json')['error'],
          ),
        ),
      );
      await expectLater(
        harness.client.runtimeRequest('mobile.hello'),
        throwsA(
          isA<StateError>().having(
            (error) => error.message,
            'message',
            _wireFixture(
              'response.error.mobile_version_mismatch.json',
            )['error'],
          ),
        ),
      );
    },
  );

  test('mobile hello fixtures keep the negotiated contract keys', () {
    final current = _wireFixture('response.ok.mobile_hello.json');
    expect(current['ok'], isTrue);
    final payload = Map<String, Object?>.from(current['payload']! as Map);
    expect(payload['protocolVersion'], isA<int>());
    expect(payload['runtime'], 'alera');
    expect(payload['runtimeCapabilities'], isA<List>());
    expect(payload['authenticated'], isTrue);
    expect(
      Map<String, Object?>.from(payload['device']! as Map)['id'],
      'device-1',
    );

    // The legacy pin drops binaryFrames/supportedTabKinds entirely; a modern
    // client must not require them to exist.
    final legacy = Map<String, Object?>.from(
      _wireFixture('response.ok.mobile_hello.legacy.json')['payload']! as Map,
    );
    expect(legacy.containsKey('binaryFrames'), isFalse);
    expect(legacy['authenticated'], isTrue);
  });
}
