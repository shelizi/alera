part of 'terminal_host_client_test.dart';

Future<SocketTerminalHostClient> _binaryClient(
  _TerminalHostTestServer server, {
  required bool advertiseCapability,
}) async {
  final tempDir = await Directory.systemTemp.createTemp('alera-host-binary-');
  addTearDown(() async {
    if (await tempDir.exists()) {
      await tempDir.delete(recursive: true);
    }
  });
  await _writeControlFile(
    tempDir: tempDir,
    port: server.port,
    token: 'existing-token',
    includeBinaryFramesCapability: advertiseCapability,
  );
  final client = SocketTerminalHostClient(
    launcher: _NoopTerminalHostLauncher(),
    applicationSupportDirectory: () async => tempDir,
  );
  addTearDown(client.dispose);
  return client;
}

void _registerTerminalHostClientBinaryFrameTests() {
  test('a burst of 1500 framed outputs keeps one socket connected', () async {
    final server = await _TerminalHostTestServer.start(
      negotiateBinaryFrames: true,
    );
    addTearDown(server.dispose);
    final client = await _binaryClient(server, advertiseCapability: true);
    await client.runtimeRequest('status.get');

    const count = 1500;
    final expected = StringBuffer();
    final actual = StringBuffer();
    final done = Completer<void>();
    var outputs = 0;
    final sub = client.eventsForSession('session-1').listen((event) {
      if (event is! TerminalHostOutputTextEvent) return;
      actual.write(event.text);
      outputs++;
      if (outputs == count && !done.isCompleted) done.complete();
    });
    addTearDown(sub.cancel);

    for (var index = 0; index < count; index++) {
      final line = 'BURST_${index.toString().padLeft(4, '0')}_SAFE\r\n';
      expected.write(line);
      server.sendOutput('session-1', utf8.encode(line));
    }
    await done.future.timeout(const Duration(seconds: 12));
    expect(actual.toString(), expected.toString());

    // After the burst both output and control traffic must remain viable on
    // the original authenticated socket; no full reconnect/replay was needed.
    await client.runtimeRequest('status.get');
    expect(server.acceptedConnections, 1);
  });

  test('broken frame exposes a classified close and allows reattach', () async {
    final warnings = <LogRecord>[];
    final logSub = Logger('SocketTerminalHostClient').onRecord.listen((record) {
      if (record.level >= Level.WARNING) warnings.add(record);
    });
    addTearDown(logSub.cancel);
    final server = await _TerminalHostTestServer.start(
      negotiateBinaryFrames: true,
    );
    addTearDown(server.dispose);
    final client = await _binaryClient(server, advertiseCapability: true);

    await client.runtimeRequest('status.get');
    final failure = client
        .eventsForSession('session-1')
        .where((event) => event is TerminalHostErrorEvent)
        .cast<TerminalHostErrorEvent>()
        .first;
    server.sendMalformedFrameHeader();

    expect(
      (await failure.timeout(const Duration(seconds: 5))).error.toString(),
      contains('protocol'),
    );
    expect(
      warnings.any(
        (record) => record.message.contains(
          'origin=socket_reader_error reader_stage=protocol',
        ),
      ),
      isTrue,
    );
    // A decoder fault should not poison the client: the published runtime is
    // still available for a fresh authenticated connection.
    await client.runtimeRequest('status.get');
    expect(server.acceptedConnections, greaterThanOrEqualTo(2));
  });

  test('negotiates frames and receives raw output', () async {
    final server = await _TerminalHostTestServer.start(
      negotiateBinaryFrames: true,
    );
    addTearDown(server.dispose);
    final client = await _binaryClient(server, advertiseCapability: true);

    await client.runtimeRequest('status.get');
    expect(
      server.payloadFor('hello')['binaryFrames'],
      isTrue,
      reason: 'the client must ask for the upgrade it is capable of',
    );
    expect(server.usingBinaryFrames, isTrue);

    // On the framed path the reader isolate also decodes, so the UI isolate
    // receives text it can hand straight to the emulator.
    final output = client
        .eventsForSession('session-1')
        .where((event) => event is TerminalHostOutputTextEvent)
        .cast<TerminalHostOutputTextEvent>()
        .first;
    server.sendOutput('session-1', utf8.encode('hola ñ'));

    expect((await output).text, 'hola ñ');
  });

  test('control events still arrive once framed', () async {
    final server = await _TerminalHostTestServer.start(
      negotiateBinaryFrames: true,
    );
    addTearDown(server.dispose);
    final client = await _binaryClient(server, advertiseCapability: true);
    await client.runtimeRequest('status.get');

    final runtimeEvent = client.runtimeEvents.first;
    server.send(<String, Object?>{
      'event': 'agentPresenceChanged',
      'payload': <String, Object?>{},
    });

    expect((await runtimeEvent).name, 'agentPresenceChanged');
  });

  test('a host without the capability keeps JSON lines', () async {
    // The compatibility case that matters most: attaching to an older sidecar
    // that is already running must not change anything.
    final server = await _TerminalHostTestServer.start(
      negotiateBinaryFrames: true,
    );
    addTearDown(server.dispose);
    final client = await _binaryClient(server, advertiseCapability: false);

    await client.runtimeRequest('status.get');
    expect(server.payloadFor('hello').containsKey('binaryFrames'), isFalse);
    expect(server.usingBinaryFrames, isFalse);

    final output = client
        .eventsForSession('session-1')
        .where((event) => event is TerminalHostOutputEvent)
        .cast<TerminalHostOutputEvent>()
        .first;
    server.sendOutput('session-1', <int>[1, 2, 3]);

    expect((await output).data, <int>[1, 2, 3]);
  });

  test('a host that declines the upgrade keeps JSON lines', () async {
    final server = await _TerminalHostTestServer.start();
    addTearDown(server.dispose);
    final client = await _binaryClient(server, advertiseCapability: true);

    await client.runtimeRequest('status.get');
    expect(server.payloadFor('hello')['binaryFrames'], isTrue);
    expect(
      server.usingBinaryFrames,
      isFalse,
      reason: 'asking is not enough; the response decides',
    );

    final output = client
        .eventsForSession('session-1')
        .where((event) => event is TerminalHostOutputEvent)
        .cast<TerminalHostOutputEvent>()
        .first;
    server.sendOutput('session-1', <int>[4, 5]);

    expect((await output).data, <int>[4, 5]);
  });
}
