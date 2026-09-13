part of 'terminal_host_client_test.dart';

void _registerTerminalHostClientRuntimeSettingsTests() {
  test(
    'runtime settings revision from get is sent on the next update',
    () async {
      final harness = await _connectRuntimeSettingsTestClient(
        responseForType: (type, payload) {
          return switch (type) {
            'runtimeSettings.get' => <String, Object?>{
              'ok': true,
              'payload': <String, Object?>{
                'workspaceDirectory': '/initial',
                'revision': 7,
              },
            },
            'runtimeSettings.update' => <String, Object?>{
              'ok': true,
              'payload': <String, Object?>{
                'workspaceDirectory': payload['workspaceDirectory'],
                'revision': 8,
              },
            },
            _ => null,
          };
        },
      );

      await harness.client.runtimeRequest('runtimeSettings.get');
      await harness.client.runtimeRequest(
        'runtimeSettings.update',
        const <String, Object?>{'workspaceDirectory': '/updated'},
      );
      await harness.client.runtimeRequest(
        'runtimeSettings.update',
        const <String, Object?>{'workspaceDirectory': '/updated-again'},
      );

      expect(harness.server.payloadsFor('runtimeSettings.update'), [
        <String, Object?>{
          'workspaceDirectory': '/updated',
          'expectedRevision': 7,
        },
        <String, Object?>{
          'workspaceDirectory': '/updated-again',
          'expectedRevision': 8,
        },
      ]);
    },
  );

  test(
    'runtime settings update omits expectedRevision for a legacy get',
    () async {
      final harness = await _connectRuntimeSettingsTestClient(
        responseForType: (type, payload) {
          return switch (type) {
            'runtimeSettings.get' => <String, Object?>{
              'ok': true,
              'payload': <String, Object?>{'workspaceDirectory': '/legacy'},
            },
            'runtimeSettings.update' => <String, Object?>{
              'ok': true,
              'payload': const <String, Object?>{},
            },
            _ => null,
          };
        },
      );

      await harness.client.runtimeRequest('runtimeSettings.get');
      await harness.client.runtimeRequest(
        'runtimeSettings.update',
        const <String, Object?>{'workspaceDirectory': '/legacy-update'},
      );

      final payload = harness.server.payloadFor('runtimeSettings.update');
      expect(payload, <String, Object?>{
        'workspaceDirectory': '/legacy-update',
      });
      expect(payload.containsKey('expectedRevision'), isFalse);
    },
  );

  test(
    'runtime settings conflict refreshes without silently retrying the update',
    () async {
      var updateCount = 0;
      var getCount = 0;
      final harness = await _connectRuntimeSettingsTestClient(
        responseForType: (type, payload) {
          return switch (type) {
            'runtimeSettings.get' => <String, Object?>{
              'ok': true,
              'payload': <String, Object?>{
                'workspaceDirectory': getCount++ == 0
                    ? '/before-conflict'
                    : '/changed-elsewhere',
                'revision': getCount == 1 ? 3 : 4,
              },
            },
            'runtimeSettings.update' =>
              updateCount++ == 0
                  ? <String, Object?>{
                      'ok': false,
                      'error': 'Runtime settings revision conflict. Refresh and retry.',
                      'errorCode': 'runtime_settings_revision_conflict',
                      'errorDetails': <String, Object?>{
                        'expectedRevision': 3,
                        'actualRevision': 4,
                      },
                    }
                  : <String, Object?>{
                      'ok': true,
                      'payload': <String, Object?>{'revision': 5},
                    },
            _ => null,
          };
        },
      );

      await harness.client.runtimeRequest('runtimeSettings.get');
      await expectLater(
        harness.client.runtimeRequest(
          'runtimeSettings.update',
          const <String, Object?>{'workspaceDirectory': '/local-edit'},
        ),
        throwsA(
          isA<TerminalHostConflictException>()
              .having(
                (error) => error.code,
                'code',
                'runtime_settings_revision_conflict',
              )
              .having((error) => error.details, 'details', <String, Object?>{
                'expectedRevision': 3,
                'actualRevision': 4,
              }),
        ),
      );

      expect(harness.server.requestTypes, <String>[
        'hello',
        'runtimeSettings.get',
        'runtimeSettings.update',
        'runtimeSettings.get',
      ]);
      expect(harness.server.payloadsFor('runtimeSettings.update'), [
        <String, Object?>{
          'workspaceDirectory': '/local-edit',
          'expectedRevision': 3,
        },
      ]);

      await harness.client.runtimeRequest(
        'runtimeSettings.update',
        const <String, Object?>{'workspaceDirectory': '/retry-after-review'},
      );
      expect(harness.server.payloadsFor('runtimeSettings.update').last, {
        'workspaceDirectory': '/retry-after-review',
        'expectedRevision': 4,
      });
    },
  );
}

Future<
  ({
    Directory tempDir,
    _TerminalHostTestServer server,
    SocketTerminalHostClient client,
  })
>
_connectRuntimeSettingsTestClient({
  required FutureOr<Map<String, Object?>?> Function(
    String type,
    Map<String, Object?> payload,
  )?
  responseForType,
}) async {
  final tempDir = await Directory.systemTemp.createTemp(
    'alera-runtime-settings-client-',
  );
  addTearDown(() async {
    if (await tempDir.exists()) {
      await tempDir.delete(recursive: true);
    }
  });
  final server = await _TerminalHostTestServer.start(
    responseForType: responseForType,
  );
  addTearDown(server.dispose);
  await _writeControlFile(
    tempDir: tempDir,
    port: server.port,
    token: server.token,
  );
  final client = SocketTerminalHostClient(
    launcher: _NoopTerminalHostLauncher(),
    applicationSupportDirectory: () async => tempDir,
  );
  addTearDown(client.dispose);
  return (tempDir: tempDir, server: server, client: client);
}
