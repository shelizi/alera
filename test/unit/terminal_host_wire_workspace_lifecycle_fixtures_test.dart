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

Object? _resolveFixture(Object? expected, Object? actual) {
  if (expected is String &&
      expected.startsWith('<') &&
      expected.endsWith('>')) {
    return actual;
  }
  if (expected is Map) {
    final actualMap = actual! as Map;
    return <String, Object?>{
      for (final entry in expected.entries)
        entry.key as String: _resolveFixture(entry.value, actualMap[entry.key]),
    };
  }
  if (expected is List) {
    final actualList = actual! as List;
    return <Object?>[
      for (var index = 0; index < expected.length; index++)
        _resolveFixture(expected[index], actualList[index]),
    ];
  }
  return expected;
}

void _expectFixture(Object? actual, Object? expected) {
  expect(actual, _resolveFixture(expected, actual));
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
      _wireFixture(responseFixture),
      <Map<String, Object?>>[
        for (final fixture in pushFixtures) _wireFixture(fixture),
      ],
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
      // Mutation broadcasts are written first on the host's response lane.
      for (final push in response.pushFirst) {
        socket.writeln(jsonEncode(push));
      }
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
  const _FixtureResponse(this.frame, this.pushFirst);

  final Map<String, Object?> frame;
  final List<Map<String, Object?>> pushFirst;
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
    'alera-wire-workspace-lifecycle-',
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

Future<void> _waitForEvents(List<RuntimeHostEvent> events, int count) async {
  final deadline = DateTime.now().add(const Duration(seconds: 5));
  while (events.length < count) {
    if (DateTime.now().isAfter(deadline)) {
      fail('expected $count runtime events, got ${events.length}');
    }
    await Future<void>.delayed(const Duration(milliseconds: 5));
  }
}

void main() {
  test(
    'workspace lifecycle fixtures preserve event order and response shapes',
    () async {
      final harness = await _connectFixtureClient();
      await harness.client.runtimeRequest('status.get');
      final events = <RuntimeHostEvent>[];
      final subscription = harness.client.runtimeEvents.listen(events.add);
      addTearDown(subscription.cancel);

      final createRequest = _wireFixture(
        'request.workspace_lifecycle_create.json',
      );
      harness.server.respondWith(
        'workspace.upsert',
        'response.ok.workspace_lifecycle_create.json',
        pushFixtures: const <String>[
          'event.workspaces_changed.workspace_lifecycle_scoped.json',
        ],
      );
      final createPayload = await harness.client.runtimeRequest(
        createRequest['type']! as String,
        Map<String, Object?>.from(createRequest['payload']! as Map),
      );
      _expectFixture(
        createPayload,
        _wireFixture('response.ok.workspace_lifecycle_create.json')['payload'],
      );

      final renameRequest = _wireFixture(
        'request.workspace_lifecycle_rename.json',
      );
      harness.server.respondWith(
        'workspace.rename',
        'response.ok.workspace_lifecycle_rename.json',
        pushFixtures: const <String>[
          'event.workspaces_changed.workspace_lifecycle_rename.json',
        ],
      );
      final renamePayload = await harness.client.runtimeRequest(
        renameRequest['type']! as String,
        Map<String, Object?>.from(renameRequest['payload']! as Map),
      );
      _expectFixture(
        renamePayload,
        _wireFixture('response.ok.workspace_lifecycle_rename.json')['payload'],
      );

      final removeRequest = _wireFixture(
        'request.workspace_lifecycle_remove.json',
      );
      harness.server.respondWith(
        'workspace.remove',
        'response.ok.workspace_lifecycle_remove.json',
        pushFixtures: const <String>[
          'event.workspaces_changed.workspace_lifecycle_remove.json',
          'event.workspace_tabs_changed.workspace_lifecycle_remove.json',
        ],
      );
      final removePayload = await harness.client.runtimeRequest(
        removeRequest['type']! as String,
        Map<String, Object?>.from(removeRequest['payload']! as Map),
      );
      _expectFixture(
        removePayload,
        _wireFixture('response.ok.workspace_lifecycle_remove.json')['payload'],
      );

      await _waitForEvents(events, 4);
      expect(events.map((event) => event.name).toList(), <String>[
        'workspacesChanged',
        'workspacesChanged',
        'workspacesChanged',
        'workspaceTabsChanged',
      ]);
      _expectFixture(
        events[0].payload,
        _wireFixture(
          'event.workspaces_changed.workspace_lifecycle_scoped.json',
        )['payload'],
      );
      _expectFixture(
        events[1].payload,
        _wireFixture(
          'event.workspaces_changed.workspace_lifecycle_rename.json',
        )['payload'],
      );
      _expectFixture(
        events[2].payload,
        _wireFixture(
          'event.workspaces_changed.workspace_lifecycle_remove.json',
        )['payload'],
      );
      _expectFixture(
        events[3].payload,
        _wireFixture(
          'event.workspace_tabs_changed.workspace_lifecycle_remove.json',
        )['payload'],
      );
      expect(
        harness.server.payloadFor('workspace.upsert'),
        createRequest['payload'],
      );
      expect(
        harness.server.payloadFor('workspace.rename'),
        renameRequest['payload'],
      );
      expect(
        harness.server.payloadFor('workspace.remove'),
        removeRequest['payload'],
      );
    },
  );
}
