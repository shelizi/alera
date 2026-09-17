import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:alera/src/features/workbench/infra/terminal_host/terminal_host_client.dart';
import 'package:alera/src/platform/runtime_host/protocol/terminal_host_protocol.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

/// The output-stream fixtures under `test/fixtures/wire/` are the shared
/// contract: `wire_fixture_tests_stream.rs` in `rust/alera-cli` produces them
/// through the real terminal host, and this suite decodes the same files
/// through [SocketTerminalHostClient].
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

  void send(String fixtureName) {
    _clients.last.writeln(jsonEncode(_wireFixture(fixtureName)));
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
_connectFixtureClient() async {
  final tempDir = await Directory.systemTemp.createTemp(
    'alera-wire-stream-fixtures-',
  );
  addTearDown(() async {
    if (await tempDir.exists()) {
      await tempDir.delete(recursive: true);
    }
  });
  final server = await _WireFixtureServer.start();
  addTearDown(server.dispose);
  server.respondWith('hello', 'response.ok.hello.json');
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
  test('setOutputPaused sends the shared resume request and decodes a delta '
      'reply', () async {
    final harness = await _connectFixtureClient();
    harness.server.respondWith(
      'setOutputPaused',
      'response.ok.output_resumed_delta.json',
    );
    final requestFixture = _wireFixture(
      'request.set_output_paused.resume.json',
    );
    final requestPayload = Map<String, Object?>.from(
      requestFixture['payload']! as Map,
    );

    final resume = await harness.client.setOutputPaused(
      sessionId: requestPayload['sessionId']! as String,
      paused: requestPayload['paused']! as bool,
    );

    // The serialized request must be the fixture's payload verbatim.
    expect(
      harness.server.payloadFor('setOutputPaused'),
      requestFixture['payload'],
    );
    expect(resume.isDelta, isTrue);
    // A delta resume carries no bytes: the host pushes the gap on the
    // output lane instead.
    expect(resume.snapshot, isEmpty);
    expect(resume.resetInteractionModes, isFalse);
  });

  test('setOutputPaused decodes the snapshot resend reply', () async {
    final harness = await _connectFixtureClient();
    harness.server.respondWith(
      'setOutputPaused',
      'response.ok.output_resumed_snapshot.json',
    );
    final requestPayload = Map<String, Object?>.from(
      _wireFixture('request.set_output_paused.resume.json')['payload']! as Map,
    );

    final resume = await harness.client.setOutputPaused(
      sessionId: requestPayload['sessionId']! as String,
      paused: requestPayload['paused']! as bool,
    );

    expect(resume.isDelta, isFalse);
    expect(resume.snapshot, isEmpty);
    expect(resume.snapshotText, 'efgh');
    expect(resume.resetInteractionModes, isTrue);
  });

  test('the legacy snapshot reply still decodes as a full replace', () async {
    final harness = await _connectFixtureClient();
    // The legacy pin is what a pre-snapshot-metadata host sent: no delta
    // flag, no dims, no resetInteractionModes. An absent delta must still
    // mean "replace the emulator".
    harness.server.respondWith(
      'setOutputPaused',
      'response.ok.output_resumed_snapshot.legacy.json',
    );

    final resume = await harness.client.setOutputPaused(
      sessionId: 's1',
      paused: false,
    );

    expect(resume.isDelta, isFalse);
    expect(resume.snapshot, isEmpty);
    expect(resume.snapshotText, 'efgh');
    expect(resume.resetInteractionModes, isFalse);
  });

  test('session_not_attached decodes to StateError', () async {
    final harness = await _connectFixtureClient();
    harness.server.respondWith(
      'setOutputPaused',
      'response.error.session_not_attached.json',
    );

    await expectLater(
      harness.client.setOutputPaused(sessionId: 's-missing', paused: false),
      throwsA(
        isA<StateError>().having(
          (error) => error.message,
          'message',
          'Terminal session is not attached: s-missing',
        ),
      ),
    );
  });

  test(
    'output and outputResyncRequired fixtures surface as session events',
    () async {
      final harness = await _connectFixtureClient();
      harness.server.respondWith(
        'setOutputPaused',
        'response.ok.output_resumed_delta.json',
      );
      // Handshake first so the connection exists before the server pushes.
      await harness.client.setOutputPaused(sessionId: 's1', paused: true);
      final events = <TerminalHostEvent>[];
      final subscription = harness.client
          .eventsForSession('s1')
          .listen(events.add);
      addTearDown(subscription.cancel);

      harness.server.send('event.output.json');
      harness.server.send('event.output_resync_required.json');
      final deadline = DateTime.now().add(const Duration(seconds: 5));
      while (events.length < 2) {
        if (DateTime.now().isAfter(deadline)) {
          fail('expected 2 session events, got ${events.length}');
        }
        await Future<void>.delayed(const Duration(milliseconds: 5));
      }

      final output = events[0];
      expect(output, isA<TerminalHostOutputEvent>());
      expect(output.sessionId, 's1');
      expect(
        (output as TerminalHostOutputEvent).data,
        base64Decode('G1swbf8A'),
      );
      final resync = events[1];
      expect(resync, isA<TerminalHostOutputResyncRequiredEvent>());
      expect(resync.sessionId, 's1');
    },
  );

  test(
    'broadcast change fixtures surface on runtimeEvents with exact payloads',
    () async {
      final harness = await _connectFixtureClient();
      // Handshake first; the synthetic runtimeHostConnected event emitted
      // during it is excluded by subscribing afterwards.
      await harness.client.runtimeRequest('status.get');
      final events = <RuntimeHostEvent>[];
      final subscription = harness.client.runtimeEvents.listen(events.add);
      addTearDown(subscription.cancel);

      harness.server.send('event.projects_changed.json');
      harness.server.send('event.project_configs_changed.json');
      harness.server.send('event.mobile_devices_changed.json');
      harness.server.send('event.workspace_tabs_changed.wildcard.json');
      final deadline = DateTime.now().add(const Duration(seconds: 5));
      while (events.length < 4) {
        if (DateTime.now().isAfter(deadline)) {
          fail('expected 4 runtime events, got ${events.length}');
        }
        await Future<void>.delayed(const Duration(milliseconds: 5));
      }

      expect(events.map((event) => event.name).toList(), <String>[
        'projectsChanged',
        'projectConfigsChanged',
        'mobileDevicesChanged',
        'workspaceTabsChanged',
      ]);
      for (final event in events) {
        expect(event.payload, <String, Object?>{});
      }
    },
  );
}
