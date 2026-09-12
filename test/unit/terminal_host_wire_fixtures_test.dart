import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/infra/runtime_workbench_repository.dart';
import 'package:alera/src/features/workbench/infra/terminal_host/terminal_host_client.dart';
import 'package:alera/src/platform/runtime_host/protocol/terminal_host_protocol.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

/// The JSON documents under `test/fixtures/wire/` are the shared contract:
/// `wire_fixture_tests.rs` in `rust/alera-cli` produces them through the real
/// terminal host, and this suite decodes the same files through
/// [SocketTerminalHostClient] and [RuntimeWorkbenchRepository].
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

  /// Answers [type] with [responseFixture]. [pushFixtures] are written first,
  /// mirroring the host emitting change events ahead of a mutation's
  /// response on the same lane.
  void respondWith(
    String type,
    String responseFixture, {
    List<String> pushFixtures = const <String>[],
  }) {
    _responses[type] = _FixtureResponse(
      _wireFixture(responseFixture),
      <Map<String, Object?>>[
        for (final name in pushFixtures) _wireFixture(name),
      ],
    );
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
  final tempDir = await Directory.systemTemp.createTemp('alera-wire-fixtures-');
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
  test('listWorkspaceTabs decodes the shared desktop projection', () async {
    final harness = await _connectFixtureClient();
    harness.server.respondWith('tab.list', 'response.tab.list.desktop.json');
    final repository = RuntimeWorkbenchRepository(harness.client);
    final fixture = _wireFixture('response.tab.list.desktop.json');
    final fixtureTab =
        (fixture['payload']! as List).single as Map<String, Object?>;

    final tabs = await repository.listWorkspaceTabs('workspace-1');

    expect(harness.server.payloadFor('tab.list'), <String, Object?>{
      'workspaceId': 'workspace-1',
    });
    expect(tabs, hasLength(1));
    final tab = tabs.single;
    expect(tab.id, fixtureTab['id']);
    expect(tab.workspaceId, fixtureTab['workspaceId']);
    expect(tab.kind, WorkspaceTabKind.terminal);
    expect(tab.title, fixtureTab['title']);
    expect(
      tab.createdAt,
      DateTime.parse(fixtureTab['createdAt']! as String).toUtc(),
    );
    expect(tab.payload, fixtureTab['payload']);
    // Host-owned recovery keys stay off the wire.
    expect(tab.payload.containsKey('initialPrompt'), isFalse);
    expect(tab.payload.containsKey('pendingAgentPrompt'), isFalse);
    expect(tab.payload.containsKey('agentTitleStateV1'), isFalse);
    expect(tab.payload['terminalPulse'], isNotNull);
  });

  test(
    'upsertWorkspaceTab round-trips the shared request and response',
    () async {
      final harness = await _connectFixtureClient();
      harness.server.respondWith(
        'tab.upsert',
        'response.tab.upsert.desktop.json',
        pushFixtures: const <String>['event.workspace_tabs_changed.json'],
      );
      final repository = RuntimeWorkbenchRepository(harness.client);
      final requestFixture = _wireFixture('request.tab.upsert.json');
      final requestPayload = Map<String, Object?>.from(
        requestFixture['payload']! as Map,
      );
      final record = WorkspaceTabRecord(
        id: requestPayload['id']! as String,
        workspaceId: requestPayload['workspaceId']! as String,
        kind: WorkspaceTabKind.fromJson(requestPayload['kind']),
        title: requestPayload['title']! as String,
        createdAt: DateTime.parse(requestPayload['createdAt']! as String),
        updatedAt: DateTime.parse(requestPayload['updatedAt']! as String),
        payload: Map<String, Object?>.from(requestPayload['payload']! as Map),
      );
      // The client emits a synthetic runtimeHostConnected on auth; only the
      // pushed fixture event is interesting here.
      final tabEvent = harness.client.runtimeEvents.firstWhere(
        (event) => event.name == 'workspaceTabsChanged',
      );

      final upserted = await repository.upsertWorkspaceTab(record);

      // The serialized request must be the fixture's payload verbatim.
      expect(
        harness.server.payloadFor('tab.upsert'),
        requestFixture['payload'],
      );
      final responseFixture = _wireFixture('response.tab.upsert.desktop.json');
      final responsePayload = Map<String, Object?>.from(
        responseFixture['payload']! as Map,
      );
      expect(upserted.title, 'Flutter (renamed)');
      expect(upserted.payload, responsePayload['payload']);
      final event = await tabEvent;
      expect(event.name, 'workspaceTabsChanged');
      expect(event.payload, <String, Object?>{'workspaceId': 'workspace-1'});
    },
  );

  test(
    'typed conflict responses decode to TerminalHostConflictException',
    () async {
      final harness = await _connectFixtureClient();
      harness.server.respondWith(
        'agentProfile.remove',
        'response.error.conflict.json',
      );
      final request = _wireFixture(
        'request.agent_profile_remove.conflict.json',
      );
      final fixture = _wireFixture('response.error.conflict.json');

      await expectLater(
        harness.client.runtimeRequest(
          request['type']! as String,
          Map<String, Object?>.from(request['payload']! as Map),
        ),
        throwsA(
          isA<TerminalHostConflictException>()
              .having((error) => error.code, 'code', fixture['errorCode'])
              .having((error) => error.message, 'message', fixture['error'])
              .having(
                (error) => error.details,
                'details',
                fixture['errorDetails'],
              ),
        ),
      );
    },
  );

  test(
    'untyped error fixtures decode to StateError with the wire message',
    () async {
      final harness = await _connectFixtureClient();
      harness.server.respondWith('tab.list', 'response.error.format.json');
      harness.server.respondWith(
        'workbenchViewPrefs.update',
        'response.error.mobile_prefs_conflict.json',
      );
      harness.server.respondWith(
        'workspace.list',
        'response.error.unauthenticated.json',
      );

      await expectLater(
        harness.client.runtimeRequest('tab.list'),
        throwsA(
          isA<StateError>().having(
            (error) => error.message,
            'message',
            _wireFixture('response.error.format.json')['error'],
          ),
        ),
      );
      // The mobile view-prefs conflict currently crosses the wire untyped; the
      // desktop client surfaces it as a plain StateError.
      await expectLater(
        harness.client.runtimeRequest(
          'workbenchViewPrefs.update',
          Map<String, Object?>.from(
            _wireFixture(
                  'request.workbench_view_prefs.update.mobile.json',
                )['payload']!
                as Map,
          ),
        ),
        throwsA(
          isA<StateError>().having(
            (error) => error.message,
            'message',
            'Workbench view preferences changed on desktop. Refresh and retry.',
          ),
        ),
      );
      await expectLater(
        harness.client.runtimeRequest('workspace.list'),
        throwsA(
          isA<StateError>().having(
            (error) => error.message,
            'message',
            'Terminal host client is not authenticated.',
          ),
        ),
      );
    },
  );

  test('mobile tab.upsert denial decodes to StateError', () async {
    final harness = await _connectFixtureClient();
    harness.server.respondWith(
      'tab.upsert',
      'response.error.mobile_tab_upsert_denied.json',
    );

    await expectLater(
      harness.client.runtimeRequest(
        'tab.upsert',
        Map<String, Object?>.from(
          _wireFixture('request.tab.upsert.json')['payload']! as Map,
        ),
      ),
      throwsA(
        isA<StateError>().having(
          (error) => error.message,
          'message',
          'Mobile clients cannot call terminal host request: tab.upsert',
        ),
      ),
    );
  });

  test(
    'runtime event fixtures surface on runtimeEvents with exact payloads',
    () async {
      final harness = await _connectFixtureClient();
      // Handshake first so the connection exists before the server pushes. The
      // synthetic runtimeHostConnected event is emitted during it, so this
      // subscription only sees the pushed fixtures.
      await harness.client.runtimeRequest('status.get');
      final events = <RuntimeHostEvent>[];
      final subscription = harness.client.runtimeEvents.listen(events.add);
      addTearDown(subscription.cancel);

      harness.server.send('event.workspaces_changed.json');
      harness.server.send('event.workspaces_changed.wildcard.json');
      harness.server.send('event.project_clone_jobs_changed.json');
      harness.server.send('event.workspace_tabs_changed.json');
      final deadline = DateTime.now().add(const Duration(seconds: 5));
      while (events.length < 3) {
        if (DateTime.now().isAfter(deadline)) {
          fail('expected 3 runtime events, got ${events.length}');
        }
        await Future<void>.delayed(const Duration(milliseconds: 5));
      }

      // projectCloneJobsChanged is not a desktop runtime event today: the
      // client drops it, which is the pinned contract for this client.
      expect(events.map((event) => event.name).toList(), <String>[
        'workspacesChanged',
        'workspacesChanged',
        'workspaceTabsChanged',
      ]);
      expect(events[0].payload, <String, Object?>{'projectId': 'project-1'});
      expect(events[1].payload, <String, Object?>{});
      expect(events[2].payload, <String, Object?>{
        'workspaceId': 'workspace-1',
      });
    },
  );
}
