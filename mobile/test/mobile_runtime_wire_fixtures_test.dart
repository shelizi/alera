import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:alera_mobile/src/features/diagnostics/infra/crash_reporting.dart';
import 'package:alera_mobile/src/features/runtime/infra/mobile_runtime_client.dart';
import 'package:alera_mobile/src/features/workbench/domain/mobile_view_prefs.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

/// The JSON documents under `test/fixtures/wire/` at the repository root are
/// the shared contract: `wire_fixture_tests.rs` in `rust/alera-cli` produces
/// them through the real terminal host, and this suite decodes the same files
/// through [MobileRuntimeClient].
Map<String, Object?> _wireFixture(String name) {
  final file = File(
    p.join(Directory.current.path, '..', 'test', 'fixtures', 'wire', name),
  );
  return Map<String, Object?>.from(jsonDecode(file.readAsStringSync()) as Map);
}

/// A loopback WebSocket server that replays fixture frames verbatim,
/// rewriting only the response `id` to match the request it answers.
final class _WireFixtureServer {
  _WireFixtureServer._(this._listener);

  final HttpServer _listener;
  final Map<String, Map<String, Object?>> _responses =
      <String, Map<String, Object?>>{};
  final List<WebSocket> _sockets = <WebSocket>[];
  final List<Map<String, Object?>> requests = <Map<String, Object?>>[];
  StreamSubscription<HttpRequest>? _subscription;

  static Future<_WireFixtureServer> start() async {
    final listener = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    final server = _WireFixtureServer._(listener);
    server._subscription = listener.listen((request) async {
      final socket = await WebSocketTransformer.upgrade(request);
      server._sockets.add(socket);
      socket.listen((raw) => server._handle(socket, raw), onError: (_) {});
    });
    return server;
  }

  int get port => _listener.port;

  InternetAddress get address => _listener.address;

  void respondWith(String type, String fixtureName) {
    _responses[type] = _wireFixture(fixtureName);
  }

  Map<String, Object?>? requestFor(String type) {
    for (final request in requests.reversed) {
      if (request['type'] == type) {
        return request;
      }
    }
    return null;
  }

  void send(String fixtureName) {
    _sockets.last.add(jsonEncode(_wireFixture(fixtureName)));
  }

  void _handle(WebSocket socket, Object? raw) {
    final request = Map<String, Object?>.from(jsonDecode(raw as String) as Map);
    request['payload'] = Map<String, Object?>.from(request['payload'] as Map);
    requests.add(request);
    if (request['type'] == 'mobile.hello') {
      socket.add(
        jsonEncode(<String, Object?>{
          'id': request['id'],
          'ok': true,
          'payload': <String, Object?>{
            'runtimeCapabilities': <String>['workspaceSectionsV1'],
          },
        }),
      );
      return;
    }
    final fixture = _responses[request['type']];
    if (fixture != null) {
      socket.add(
        jsonEncode(<String, Object?>{...fixture, 'id': request['id']}),
      );
      return;
    }
    socket.add(
      jsonEncode(<String, Object?>{
        'id': request['id'],
        'ok': true,
        'payload': const <String, Object?>{},
      }),
    );
  }

  Future<void> dispose() async {
    await _subscription?.cancel();
    for (final socket in _sockets) {
      await socket.close();
    }
    await _listener.close(force: true);
  }
}

Future<({MobileRuntimeClient client, _WireFixtureServer server})>
_connectFixtureClient() async {
  final server = await _WireFixtureServer.start();
  addTearDown(server.dispose);
  final client = await MobileRuntimeClient.connect(
    'ws://${server.address.address}:${server.port}',
  );
  addTearDown(client.dispose);
  await client.authenticate(
    deviceId: 'device-1',
    deviceToken: 'token-1',
    cloudDeviceId: 'cloud-installation-1',
  );
  return (client: client, server: server);
}

void main() {
  tearDown(CrashReporting.resetForTesting);

  test('listTabs decodes the shared mobile projection', () async {
    final harness = await _connectFixtureClient();
    harness.server.respondWith('tab.list', 'response.tab.list.mobile.json');
    final fixture = _wireFixture('response.tab.list.mobile.json');
    final fixtureTab =
        (fixture['payload']! as List).single as Map<String, Object?>;

    final tabs = await harness.client.listTabs('workspace-1');

    final request = harness.server.requestFor('tab.list');
    expect(request?['payload'], <String, Object?>{
      'workspaceId': 'workspace-1',
    });
    expect(tabs, hasLength(1));
    final tab = tabs.single;
    expect(tab.id, 'terminal-1');
    expect(tab.workspaceId, 'workspace-1');
    expect(tab.kind, 'terminal');
    expect(tab.title, 'Flutter');
    expect(tab.runtimeTitle, isNull);
    expect(tab.payload, fixtureTab['payload']);
    // Host-owned recovery keys and terminal pulse stay off the mobile wire.
    expect(tab.payload.containsKey('terminalPulse'), isFalse);
    expect(tab.payload.containsKey('initialPrompt'), isFalse);
    expect(tab.payload.containsKey('pendingAgentPrompt'), isFalse);
    expect(tab.payload.containsKey('agentTitleStateV1'), isFalse);
    expect(tab.payload['shell'], 'zsh');
  });

  test('updateWorkbenchViewPrefs sends the shared request and decodes the '
      'shared conflict error', () async {
    final harness = await _connectFixtureClient();
    harness.server.respondWith(
      'workbenchViewPrefs.update',
      'response.error.mobile_prefs_conflict.json',
    );
    final requestFixture = _wireFixture(
      'request.workbench_view_prefs.update.mobile.json',
    );

    await expectLater(
      harness.client.updateWorkbenchViewPrefs(
        const MobileViewPrefs(revision: 7),
      ),
      throwsA(
        isA<StateError>().having(
          (error) => error.message,
          'message',
          _wireFixture('response.error.mobile_prefs_conflict.json')['error'],
        ),
      ),
    );

    expect(
      harness.server.requestFor('workbenchViewPrefs.update')?['payload'],
      requestFixture['payload'],
    );
  });

  test(
    'event fixtures arrive as MobileRuntimeEvent with exact payloads',
    () async {
      final harness = await _connectFixtureClient();
      final events = <MobileRuntimeEvent>[];
      final subscription = harness.client.events.listen(events.add);
      addTearDown(subscription.cancel);

      harness.server.send('event.workspaces_changed.json');
      harness.server.send('event.project_clone_jobs_changed.json');
      harness.server.send('event.workspace_tabs_changed.json');
      harness.server.send('event.workspaces_changed.wildcard.json');
      final deadline = DateTime.now().add(const Duration(seconds: 5));
      while (events.length < 4) {
        if (DateTime.now().isAfter(deadline)) {
          fail('expected 4 runtime events, got ${events.length}');
        }
        await Future<void>.delayed(const Duration(milliseconds: 5));
      }

      expect(events.map((event) => event.name).toList(), <String>[
        'workspacesChanged',
        'projectCloneJobsChanged',
        'workspaceTabsChanged',
        'workspacesChanged',
      ]);
      expect(events[0].payload, <String, Object?>{'projectId': 'project-1'});
      expect(events[1].payload, <String, Object?>{'id': 'job-1'});
      expect(events[2].payload, <String, Object?>{
        'workspaceId': 'workspace-1',
      });
      expect(events[3].payload, <String, Object?>{});
    },
  );
}
