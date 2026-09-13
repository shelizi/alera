import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:alera/src/features/runtime_host/domain/runtime_host_status.dart';
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
  final Map<String, Map<String, Object?>> _responses =
      <String, Map<String, Object?>>{};
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

  void respondWith(String type, String fixtureName) {
    _responses[type] = _wireFixture(fixtureName);
  }

  Map<String, Object?> payloadFor(String type) {
    return Map<String, Object?>.from(
      requests
          .where((request) => request['type'] == type)
          .map((request) => request['payload']! as Map)
          .last,
    );
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
    final fixture = _responses[request['type']];
    final frame = fixture == null
        ? <String, Object?>{
            'id': request['id'],
            'ok': true,
            'payload': const <String, Object?>{},
          }
        : <String, Object?>{...fixture, 'id': request['id']};
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
    'alera-wire-shutdown-fixtures-',
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

Future<void> _expectStateError(
  Future<Object?> operation,
  String fixtureName,
) async {
  await expectLater(
    operation,
    throwsA(
      isA<StateError>().having(
        (error) => error.message,
        'message',
        _wireFixture(fixtureName)['error'],
      ),
    ),
  );
}

void main() {
  test(
    'desktop lifecycle error fixtures retain shutdown and restart variants',
    () async {
      final harness = await _connectFixtureClient();
      await harness.client.runtimeRequest('status.get');

      harness.server.respondWith(
        'host.restart',
        'response.error.host_restart_busy.json',
      );
      await _expectStateError(
        harness.client.runtimeRequest(
          'host.restart',
          Map<String, Object?>.from(
            _wireFixture('request.host_restart.busy.json')['payload']! as Map,
          ),
        ),
        'response.error.host_restart_busy.json',
      );

      harness.server.respondWith(
        'host.restart',
        'response.ok.host_restart.force.json',
      );
      final forced = await harness.client.runtimeRequest(
        'host.restart',
        Map<String, Object?>.from(
          _wireFixture('request.host_restart.force.json')['payload']! as Map,
        ),
      );
      expect(
        forced,
        _wireFixture('response.ok.host_restart.force.json')['payload'],
      );

      harness.server.respondWith(
        'host.shutdown',
        'response.error.host_shutdown.unauthenticated.json',
      );
      await _expectStateError(
        harness.client.runtimeRequest(
          'host.shutdown',
          Map<String, Object?>.from(
            _wireFixture(
                  'request.host_shutdown.unauthenticated.json',
                )['payload']!
                as Map,
          ),
        ),
        'response.error.host_shutdown.unauthenticated.json',
      );

      harness.server.respondWith(
        'host.restart',
        'response.error.host_restart.unauthenticated.json',
      );
      await _expectStateError(
        harness.client.runtimeRequest(
          'host.restart',
          Map<String, Object?>.from(
            _wireFixture(
                  'request.host_restart.unauthenticated.json',
                )['payload']!
                as Map,
          ),
        ),
        'response.error.host_restart.unauthenticated.json',
      );

      harness.server.respondWith(
        'host.shutdown',
        'response.error.host_shutdown.mutation_busy.json',
      );
      await expectLater(
        harness.client.shutdownRuntime(),
        throwsA(
          isA<RuntimeHostBusyException>()
              .having((error) => error.activeAgents, 'activeAgents', 0)
              .having((error) => error.activeSessions, 'activeSessions', 0)
              .having((error) => error.activeJobs, 'activeJobs', 1)
              .having(
                (error) => error.activePushSubscriptions,
                'activePushSubscriptions',
                0,
              ),
        ),
      );

      harness.server.respondWith(
        'host.restart',
        'response.error.host_restart.mutation_busy.json',
      );
      await _expectStateError(
        harness.client.runtimeRequest(
          'host.restart',
          Map<String, Object?>.from(
            _wireFixture('request.host_restart.mutation_busy.json')['payload']!
                as Map,
          ),
        ),
        'response.error.host_restart.mutation_busy.json',
      );

      expect(harness.server.payloadFor('host.shutdown'), <String, Object?>{
        'force': false,
      });
      expect(harness.server.payloadFor('host.restart'), <String, Object?>{
        'force': false,
      });
    },
  );
}
