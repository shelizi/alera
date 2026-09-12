import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/projects/infra/runtime_project_management_client.dart';
import 'package:alera/src/features/runtime_host/domain/runtime_host_status.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/infra/runtime_managed_workspace_client.dart';
import 'package:alera/src/features/workbench/infra/terminal_host/terminal_host_client.dart';
import 'package:alera/src/platform/runtime_host/protocol/terminal_host_protocol.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

/// The request/error fixtures under `test/fixtures/wire/` are the shared
/// contract: `wire_fixture_tests_requests.rs` in `rust/alera-cli` produces
/// them through the real terminal host, and this suite decodes the same
/// files through [SocketTerminalHostClient] and the typed runtime clients.
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
  final Map<String, List<Map<String, Object?>>> _oneShots =
      <String, List<Map<String, Object?>>>{};
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

  /// Answers the next [type] request with [responseFixture], then falls back
  /// to the permanent response registered with [respondWith]. Used to pin
  /// retry behavior: the client consumes the failure, then retries.
  void respondOnce(String type, String responseFixture) {
    _oneShots
        .putIfAbsent(type, () => <Map<String, Object?>>[])
        .add(_wireFixture(responseFixture));
  }

  int requestCountFor(String type) =>
      requests.where((request) => request['type'] == type).length;

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
    final type = request['type'];
    final oneShots = _oneShots[type];
    final oneShot = oneShots == null || oneShots.isEmpty
        ? null
        : oneShots.removeAt(0);
    final response = oneShot ?? _responses[type]?.frame;
    if (response != null) {
      final frame = Map<String, Object?>.from(response)..['id'] = request['id'];
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
    'alera-wire-requests-fixtures-',
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
  test('registerProject round-trips the shared request and response', () async {
    final harness = await _connectFixtureClient();
    harness.server.respondWith(
      'project.register',
      'response.ok.project_register.json',
    );
    final management = RuntimeProjectManagementClient(harness.client);
    final requestFixture = _wireFixture('request.project_register.json');
    final requestPayload = Map<String, Object?>.from(
      requestFixture['payload']! as Map,
    );

    final project = await management.registerProject(
      path: requestPayload['path']! as String,
      name: requestPayload['name']! as String,
    );

    // The serialized request must be the fixture's payload verbatim.
    expect(
      harness.server.payloadFor('project.register'),
      requestFixture['payload'],
    );
    final response = _wireFixture('response.ok.project_register.json');
    final fixtureProject = Map<String, Object?>.from(
      (response['payload']! as Map)['project']! as Map,
    );
    expect(project.id, fixtureProject['id']);
    expect(project.name, fixtureProject['name']);
    expect(project.repoPath, fixtureProject['repoPath']);
    expect(project.kind, ProjectKind.folder);
  });

  test('createLinkedWorkspace sends the shared deferSetup payload and decodes '
      'the response', () async {
    final harness = await _connectFixtureClient();
    harness.server.respondWith(
      'workspace.createManaged',
      'response.ok.workspace_create_managed.deferred.json',
    );
    final managed = RuntimeManagedWorkspaceClient(harness.client);
    final requestFixture = _wireFixture(
      'request.workspace_create_managed.json',
    );
    final requestPayload = Map<String, Object?>.from(
      requestFixture['payload']! as Map,
    );
    final project = Project(
      id: requestPayload['projectId']! as String,
      name: 'Fixture Project',
      repoPath: '/fixture/repo',
      createdAt: DateTime.utc(2000),
      updatedAt: DateTime.utc(2000),
    );

    final result = await managed.createLinkedWorkspace(
      project: project,
      sourceBranch: requestPayload['sourceBranch']! as String,
      newBranchName: requestPayload['branch']! as String,
      reuseExistingBranch: requestPayload['reuseExistingBranch']! as bool,
      name: requestPayload['name']! as String,
    );

    // The desktop always sets deferSetup; the wire payload must equal the
    // fixture verbatim.
    expect(
      harness.server.payloadFor('workspace.createManaged'),
      requestFixture['payload'],
    );
    expect(result.workspace.id, 'workspace-2');
    expect(result.workspace.projectId, 'project-1');
    expect(result.workspace.kind, WorkspaceKind.linked);
    expect(result.deferredSetupCommand, isNotNull);

    harness.server.respondWith(
      'workspace.createManaged',
      'response.ok.workspace_create_managed.json',
    );
    final inline = await managed.createLinkedWorkspace(
      project: project,
      sourceBranch: 'main',
      newBranchName: 'feature/fixture',
      reuseExistingBranch: false,
      name: 'Feature Workspace',
    );
    expect(inline.deferredSetupCommand, isNull);
    expect(inline.setupReport.steps, isEmpty);
  });

  test('the legacy createManaged reply still decodes', () async {
    final harness = await _connectFixtureClient();
    // The legacy pin is the slim historical workspace shape: no kind, status,
    // hostId or instanceId. The tolerant decoder must keep working for hosts
    // that still emit it.
    harness.server.respondWith(
      'workspace.createManaged',
      'response.ok.workspace_create_managed.legacy.json',
    );
    final managed = RuntimeManagedWorkspaceClient(harness.client);
    final project = Project(
      id: 'project-1',
      name: 'Fixture Project',
      repoPath: '/fixture/repo',
      createdAt: DateTime.utc(2000),
      updatedAt: DateTime.utc(2000),
    );

    final result = await managed.createLinkedWorkspace(
      project: project,
      sourceBranch: 'main',
      newBranchName: 'feature/fixture',
      reuseExistingBranch: false,
      name: 'Feature Workspace',
    );

    expect(result.workspace.id, 'workspace-2');
    expect(result.workspace.name, 'Feature Workspace');
    expect(result.workspace.kind, WorkspaceKind.linked);
    expect(result.workspace.status, WorkspaceStatus.active);
    expect(result.deferredSetupCommand, isNull);
  });

  test('state errors decode to StateError with the wire message', () async {
    final harness = await _connectFixtureClient();
    for (final entry in <String, String>{
      'tab.find': 'response.error.tab_not_found.json',
      'workspace.list': 'response.error.workspace_not_found.json',
    }.entries) {
      harness.server.respondWith(entry.key, entry.value);
    }

    await expectLater(
      harness.client.runtimeRequest('tab.find'),
      throwsA(
        isA<StateError>().having(
          (error) => error.message,
          'message',
          _wireFixture('response.error.tab_not_found.json')['error'],
        ),
      ),
    );
    await expectLater(
      harness.client.runtimeRequest('workspace.list'),
      throwsA(
        isA<StateError>().having(
          (error) => error.message,
          'message',
          _wireFixture('response.error.workspace_not_found.json')['error'],
        ),
      ),
    );
  });

  test(
    'a runtime-mutation-busy reply is retried instead of surfaced',
    () async {
      final harness = await _connectFixtureClient();
      // The mutation-barrier error is the client's retry signal: the first
      // attempt fails busy, the retry succeeds. The fixture therefore pins
      // the retry contract, not an error path.
      harness.server.respondOnce(
        'project.register',
        'response.error.runtime_mutation_busy.json',
      );
      harness.server.respondWith(
        'project.register',
        'response.ok.project_register.json',
      );
      final management = RuntimeProjectManagementClient(harness.client);

      final project = await management.registerProject(path: '<repo-path>');

      expect(project.name, 'Fixture Project');
      expect(harness.server.requestCountFor('project.register'), 2);
    },
  );

  test('host-busy shutdown errors decode to RuntimeHostBusyException with '
      'parsed counts', () async {
    final harness = await _connectFixtureClient();
    harness.server.respondWith(
      'host.shutdown',
      'response.error.host_busy.json',
    );
    await harness.client.runtimeRequest('status.get');

    await expectLater(
      harness.client.shutdownRuntime(),
      throwsA(
        isA<RuntimeHostBusyException>()
            .having((error) => error.activeSessions, 'activeSessions', 1)
            .having((error) => error.activeJobs, 'activeJobs', 0)
            .having((error) => error.activeAgents, 'activeAgents', 0)
            .having(
              (error) => error.activePushSubscriptions,
              'activePushSubscriptions',
              0,
            ),
      ),
    );

    // The legacy pin only carried session and job counts; the decoder must
    // still surface it as a busy error with the missing counts at zero.
    harness.server.respondWith(
      'host.shutdown',
      'response.error.host_busy.legacy.json',
    );
    await expectLater(
      harness.client.shutdownRuntime(),
      throwsA(
        isA<RuntimeHostBusyException>()
            .having((error) => error.activeSessions, 'activeSessions', 1)
            .having((error) => error.activeJobs, 'activeJobs', 0)
            .having((error) => error.activeAgents, 'activeAgents', 0),
      ),
    );
  });

  test('the legacy mobile prefs update request keeps its wire shape', () {
    final request = _wireFixture(
      'request.workbench_view_prefs.update.mobile.legacy.json',
    );
    final payload = Map<String, Object?>.from(request['payload']! as Map);
    // Mobile sends expectedRevision plus a full prefs map; desktop omits the
    // revision. The pinned shape documents what the host accepts.
    expect(payload['expectedRevision'], isA<int>());
    final prefs = Map<String, Object?>.from(payload['prefs']! as Map);
    for (final key in <String>[
      'groupBy',
      'projectSort',
      'workspaceSort',
      'workspaceKindFilter',
    ]) {
      expect(prefs.containsKey(key), isTrue, reason: 'missing $key');
    }
  });
}
