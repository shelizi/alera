import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:alera_mobile/src/features/runtime/infra/mobile_runtime_client.dart';
import 'package:flutter_test/flutter_test.dart';

typedef _RuntimeSettingsResponse = Map<String, Object?>? Function(
  String type,
  Map<String, Object?> payload,
);

void main() {
  test(
    'runtime settings revision is decoded and sent on later updates',
    () async {
      final harness = await _connectRuntimeSettingsClient(
        responseForType: (type, payload) {
          return switch (type) {
            'mobile.runtimeSettings.get' => <String, Object?>{
              'ok': true,
              'payload': <String, Object?>{
                'workspaceDirectory': '/initial',
                'revision': 7,
              },
            },
            'mobile.runtimeSettings.update' => <String, Object?>{
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

      final initial = await harness.client.loadPortableSettings();
      final updated = await harness.client.updatePortableSettings(
        const <String, Object?>{'workspaceDirectory': '/updated'},
      );
      await harness.client.updatePortableSettings(const <String, Object?>{
        'workspaceDirectory': '/updated-again',
      });

      expect(initial.revision, 7);
      expect(updated.revision, 8);
      expect(harness.server.payloadsFor('mobile.runtimeSettings.update'), [
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

  test('legacy mobile runtime settings omit expectedRevision and keep LWW behavior', () async {
    final harness = await _connectRuntimeSettingsClient(
      responseForType: (type, payload) {
        return switch (type) {
          'mobile.runtimeSettings.get' => <String, Object?>{
            'ok': true,
            'payload': const <String, Object?>{'workspaceDirectory': '/legacy'},
          },
          'mobile.runtimeSettings.update' => <String, Object?>{
            'ok': true,
            'payload': const <String, Object?>{},
          },
          _ => null,
        };
      },
    );

    await harness.client.loadPortableSettings();
    await harness.client.updatePortableSettings(const <String, Object?>{
      'workspaceDirectory': '/legacy-update',
    });

    final payload = harness.server
        .payloadsFor('mobile.runtimeSettings.update')
        .single;
    expect(payload, <String, Object?>{'workspaceDirectory': '/legacy-update'});
    expect(payload.containsKey('expectedRevision'), isFalse);
  });

  test(
    'mobile runtime settings conflict refreshes without silently retrying',
    () async {
      var getCount = 0;
      var updateCount = 0;
      final harness = await _connectRuntimeSettingsClient(
        responseForType: (type, payload) {
          return switch (type) {
            'mobile.runtimeSettings.get' => <String, Object?>{
              'ok': true,
              'payload': <String, Object?>{
                'workspaceDirectory': getCount++ == 0
                    ? '/before-conflict'
                    : '/changed-elsewhere',
                'revision': getCount == 1 ? 3 : 4,
              },
            },
            'mobile.runtimeSettings.update' =>
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

      await harness.client.loadPortableSettings();
      await expectLater(
        harness.client.updatePortableSettings(const <String, Object?>{
          'workspaceDirectory': '/local-edit',
        }),
        throwsA(
          isA<MobileRuntimeConflictException>()
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

      final settingsRequests = harness.server.requestsForRuntimeSettings;
      expect(settingsRequests.map((request) => request['type']), [
        'mobile.runtimeSettings.get',
        'mobile.runtimeSettings.update',
        'mobile.runtimeSettings.get',
      ]);
      expect(settingsRequests[1]['payload'], <String, Object?>{
        'workspaceDirectory': '/local-edit',
        'expectedRevision': 3,
      });

      await harness.client.updatePortableSettings(const <String, Object?>{
        'workspaceDirectory': '/retry-after-review',
      });
      expect(
        harness.server.payloadsFor('mobile.runtimeSettings.update').last,
        <String, Object?>{
          'workspaceDirectory': '/retry-after-review',
          'expectedRevision': 4,
        },
      );
    },
  );
}

Future<({MobileRuntimeClient client, _RuntimeSettingsServer server})>
_connectRuntimeSettingsClient({
  required _RuntimeSettingsResponse responseForType,
}) async {
  final server = await _RuntimeSettingsServer.start(
    responseForType: responseForType,
  );
  addTearDown(server.dispose);
  final client = await MobileRuntimeClient.connect(server.endpoint);
  addTearDown(client.dispose);
  await client.authenticate(deviceId: 'device-1', deviceToken: 'token-1');
  return (client: client, server: server);
}

final class _RuntimeSettingsServer {
  _RuntimeSettingsServer._(this._server, this._responseForType);

  final HttpServer _server;
  final _RuntimeSettingsResponse _responseForType;
  final List<WebSocket> _sockets = <WebSocket>[];
  final List<Map<String, Object?>> requests = <Map<String, Object?>>[];
  StreamSubscription<HttpRequest>? _subscription;

  static Future<_RuntimeSettingsServer> start({
    required _RuntimeSettingsResponse responseForType,
  }) async {
    final listener = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    final server = _RuntimeSettingsServer._(listener, responseForType);
    server._subscription = listener.listen(
      (request) => unawaited(server._upgrade(request)),
    );
    return server;
  }

  String get endpoint => 'ws://${_server.address.address}:${_server.port}';

  List<Map<String, Object?>> get requestsForRuntimeSettings {
    return <Map<String, Object?>>[
      for (final request in requests)
        if ((request['type'] as String).contains('runtimeSettings')) request,
    ];
  }

  List<Map<String, Object?>> payloadsFor(String type) {
    return <Map<String, Object?>>[
      for (final request in requests)
        if (request['type'] == type)
          request['payload']! as Map<String, Object?>,
    ];
  }

  Future<void> _upgrade(HttpRequest request) async {
    final socket = await WebSocketTransformer.upgrade(request);
    _sockets.add(socket);
    socket.listen((raw) => unawaited(_handle(socket, raw)), onError: (_) {});
  }

  Future<void> _handle(WebSocket socket, Object? raw) async {
    final request = Map<String, Object?>.from(jsonDecode(raw as String) as Map);
    request['payload'] = Map<String, Object?>.from(request['payload'] as Map);
    requests.add(request);
    final type = request['type']! as String;
    final payload = request['payload']! as Map<String, Object?>;
    final response = type == 'mobile.hello'
        ? <String, Object?>{
            'ok': true,
            'payload': <String, Object?>{
              'runtimeCapabilities': <String>[mobilePortableSettingsCapability],
            },
          }
        : _responseForType(type, payload) ??
              <String, Object?>{
                'ok': true,
                'payload': const <String, Object?>{},
              };
    socket.add(jsonEncode(<String, Object?>{...response, 'id': request['id']}));
  }

  Future<void> dispose() async {
    await _subscription?.cancel();
    for (final socket in _sockets) {
      await socket.close();
    }
    await _server.close(force: true);
  }
}
