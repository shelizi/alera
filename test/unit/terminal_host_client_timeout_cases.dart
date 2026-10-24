part of 'terminal_host_client_test.dart';

Future<SocketTerminalHostClient> _connectedClient(
  _TerminalHostTestServer server, {
  Duration heartbeatInterval = const Duration(seconds: 15),
  Duration heartbeatTimeout = const Duration(seconds: 5),
  Duration startupTimeout = const Duration(seconds: 8),
  Duration connectRetryCooldown = const Duration(seconds: 2),
}) async {
  final tempDir = await Directory.systemTemp.createTemp('alera-host-timeout-');
  addTearDown(() async {
    if (await tempDir.exists()) {
      await tempDir.delete(recursive: true);
    }
  });
  await _writeControlFile(
    tempDir: tempDir,
    port: server.port,
    token: 'existing-token',
  );
  final client = SocketTerminalHostClient(
    launcher: _NoopTerminalHostLauncher(),
    applicationSupportDirectory: () async => tempDir,
    startupTimeout: startupTimeout,
    heartbeatInterval: heartbeatInterval,
    heartbeatTimeout: heartbeatTimeout,
    connectRetryCooldown: connectRetryCooldown,
  );
  addTearDown(client.dispose);
  return client;
}

void _registerTerminalHostClientTimeoutTests() {
  test(
    'quiesces pending tab mutations before an intentional app quit',
    () async {
      final releaseMutations = Completer<void>();
      final server = await _TerminalHostTestServer.start(
        beforeResponse: (type) async {
          if (type == 'tab.upsert' || type == 'tab.remove') {
            await releaseMutations.future;
          }
        },
      );
      addTearDown(server.dispose);
      final client = await _connectedClient(server);

      await client.runtimeRequest('project.list');
      final upsert = client.runtimeRequest(
        'tab.upsert',
        const <String, Object?>{'id': 'tab-1'},
      );
      final remove = client.runtimeRequest(
        'tab.remove',
        const <String, Object?>{'id': 'tab-1'},
      );
      await _waitForServerRequestCount(server, 4);

      client.beginAppQuit();

      await expectLater(
        upsert,
        throwsA(isA<TerminalHostConnectionClosedException>()),
      );
      await expectLater(
        remove,
        throwsA(isA<TerminalHostConnectionClosedException>()),
      );

      // Lifecycle requests remain available to complete the quit gate while
      // ordinary runtime traffic is rejected as an expected close.
      final shutdown = client.shutdownRuntime();
      final shutdownResult = await shutdown;
      expect(shutdownResult.stopped, isFalse);
      expect(server.requestTypes, contains('host.shutdown'));

      releaseMutations.complete();
    },
  );

  test('reopens runtime traffic when an app quit is cancelled', () async {
    final server = await _TerminalHostTestServer.start();
    addTearDown(server.dispose);
    final client = await _connectedClient(server);

    await client.runtimeRequest('project.list');
    client.beginAppQuit();
    await expectLater(
      client.runtimeRequest('project.list'),
      throwsA(isA<TerminalHostConnectionClosedException>()),
    );

    client.cancelAppQuit();
    await client.runtimeRequest('project.list');
    expect(server.requestTypes, <String>[
      'hello',
      'project.list',
      'project.list',
    ]);
  });

  test('disposal completes pending mutations as an expected close', () async {
    final releaseMutation = Completer<void>();
    final server = await _TerminalHostTestServer.start(
      beforeResponse: (type) async {
        if (type == 'tab.upsert') {
          await releaseMutation.future;
        }
      },
    );
    addTearDown(server.dispose);
    final client = await _connectedClient(server);

    await client.runtimeRequest('project.list');
    final mutation = client.runtimeRequest(
      'tab.upsert',
      const <String, Object?>{'id': 'tab-1'},
    );
    await _waitForServerRequestCount(server, 3);

    client.dispose();

    await expectLater(
      mutation,
      throwsA(isA<TerminalHostConnectionClosedException>()),
    );
    releaseMutation.complete();
  });

  test('a sibling request still completes while another times out', () async {
    final server = await _TerminalHostTestServer.start(
      beforeResponse: (type) async {
        if (type == 'slow.request') {
          await Future.pause(const Duration(milliseconds: 300));
        }
      },
    );
    addTearDown(server.dispose);
    final client = await _connectedClient(server);

    final slow = client.runtimeRequest(
      'slow.request',
      const <String, Object?>{},
      const Duration(milliseconds: 50),
    );
    final fast = client.runtimeRequest('status.get');

    await expectLater(
      slow,
      throwsA(isA<TerminalHostRequestTimeoutException>()),
    );
    await expectLater(fast, completes);
  });

  test('the connection survives fewer failures than the probe limit', () async {
    final server = await _TerminalHostTestServer.start(
      beforeResponse: (type) async {
        if (type == 'slow.request' || type == 'status.get') {
          await Future.pause(const Duration(milliseconds: 200));
        }
      },
    );
    addTearDown(server.dispose);
    final client = await _connectedClient(
      server,
      heartbeatInterval: const Duration(milliseconds: 30),
      heartbeatTimeout: const Duration(milliseconds: 20),
    );

    await expectLater(
      client.runtimeRequest(
        'slow.request',
        const <String, Object?>{},
        const Duration(milliseconds: 20),
      ),
      throwsA(isA<TerminalHostRequestTimeoutException>()),
    );
    // Two probe intervals: below the three-failure limit, so no teardown.
    await Future.pause(const Duration(milliseconds: 75));

    expect(server.acceptedConnections, 1);
  });

  test('a wedged host is torn down after the probe limit', () async {
    // Sockets cannot report a peer that is alive but stuck, so this is the
    // one case the heartbeat exists for.
    final server = await _TerminalHostTestServer.start(
      beforeResponse: (type) async {
        if (type != 'hello') {
          await Future.pause(const Duration(seconds: 5));
        }
      },
    );
    addTearDown(server.dispose);
    final client = await _connectedClient(
      server,
      heartbeatInterval: const Duration(milliseconds: 20),
      heartbeatTimeout: const Duration(milliseconds: 15),
    );

    await expectLater(
      client.runtimeRequest(
        'slow.request',
        const <String, Object?>{},
        const Duration(milliseconds: 20),
      ),
      throwsA(isA<TerminalHostRequestTimeoutException>()),
    );
    await Future.pause(const Duration(milliseconds: 200));

    // The dead connection was dropped, so the next request reconnects.
    await expectLater(
      client.runtimeRequest(
        'status.get',
        const <String, Object?>{},
        const Duration(milliseconds: 50),
      ),
      throwsA(anything),
    );
    expect(server.acceptedConnections, greaterThan(1));
  });

  test('connect failures pace the next attempt instead of storming', () async {
    // A wedged host accepts the socket but never answers hello, so every
    // attempt burns an auth timeout. A single failure still retries right
    // away; repeated failures must not each open another doomed socket.
    final server = await _TerminalHostTestServer.start(
      beforeResponse: (type) async {
        if (type == 'hello') {
          await Future.pause(const Duration(seconds: 5));
        }
      },
    );
    addTearDown(server.dispose);
    final client = await _connectedClient(
      server,
      startupTimeout: const Duration(milliseconds: 2300),
      connectRetryCooldown: const Duration(milliseconds: 400),
    );

    await expectLater(
      client.ensureStarted(config: TerminalHostConfig.defaults),
      throwsA(anything),
    );
    final acceptedAfterFirst = server.acceptedConnections;
    expect(acceptedAfterFirst, greaterThan(0));

    // The first failure is not paced: the next call is a real attempt.
    await expectLater(
      client.ensureStarted(config: TerminalHostConfig.defaults),
      throwsA(anything),
    );
    final acceptedAfterSecond = server.acceptedConnections;
    expect(acceptedAfterSecond, greaterThan(acceptedAfterFirst));

    // Repeated failures now cool down: the call fails closed with no socket.
    await expectLater(
      client.ensureStarted(config: TerminalHostConfig.defaults),
      throwsA(isA<TerminalHostConnectionClosedException>()),
    );
    expect(server.acceptedConnections, acceptedAfterSecond);

    // Past the cooldown a real connect is attempted again.
    await Future.pause(const Duration(milliseconds: 500));
    await expectLater(
      client.ensureStarted(config: TerminalHostConfig.defaults),
      throwsA(anything),
    );
    expect(server.acceptedConnections, greaterThan(acceptedAfterSecond));
  });
}
