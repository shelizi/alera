import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:alera_mobile/src/features/runtime/infra/mobile_runtime_client.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('mobile terminate reuses mutation ids for retries and rotates after success', () async {
    var terminateAttempts = 0;
    final server = await _MobileRuntimeTestServer.start((type, _) {
      if (type == 'terminate' && terminateAttempts++ == 0) {
        return <String, Object?>{
          'ok': false,
          'error': 'temporary terminate failure',
        };
      }
      return <String, Object?>{'ok': true, 'payload': <String, Object?>{}};
    });
    addTearDown(server.dispose);
    final client = await MobileRuntimeClient.connect(server.endpoint);
    addTearDown(client.dispose);

    await expectLater(
      client.terminateSession('session-1'),
      throwsA(isA<StateError>()),
    );
    await client.terminateSession('session-1');
    await client.terminateSession('session-1');

    final payloads = server.payloadsFor('terminate');
    expect(payloads, hasLength(3));
    expect(payloads[0]['sessionId'], 'session-1');
    expect(payloads[0]['clientMutationId'], isA<String>());
    expect(payloads[1]['clientMutationId'], payloads[0]['clientMutationId']);
    expect(
      payloads[2]['clientMutationId'],
      isNot(payloads[1]['clientMutationId']),
    );
  });

  test(
    'mobile quota consume reuses ids for retries and rotates by offer',
    () async {
      var consumeAttempts = 0;
      final server = await _MobileRuntimeTestServer.start((type, _) {
        if (type == 'agentQuota.consumeCodexResetCredit' &&
            consumeAttempts++ == 0) {
          return <String, Object?>{
            'ok': false,
            'error': 'temporary consume failure',
          };
        }
        return <String, Object?>{'ok': true, 'payload': _consumeResponse()};
      });
      addTearDown(server.dispose);
      final client = await MobileRuntimeClient.connect(server.endpoint);
      addTearDown(client.dispose);

      await expectLater(
        client.consumeCodexResetCredit('offer-1'),
        throwsA(isA<StateError>()),
      );
      await client.consumeCodexResetCredit('offer-1');
      await client.consumeCodexResetCredit('offer-2');

      final payloads = server.payloadsFor('agentQuota.consumeCodexResetCredit');
      expect(payloads, hasLength(3));
      expect(payloads[0]['offerRevision'], 'offer-1');
      expect(payloads[0]['clientMutationId'], isA<String>());
      expect(payloads[1]['clientMutationId'], payloads[0]['clientMutationId']);
      expect(payloads[2]['offerRevision'], 'offer-2');
      expect(
        payloads[2]['clientMutationId'],
        isNot(payloads[1]['clientMutationId']),
      );
    },
  );
}

Map<String, Object?> _consumeResponse() {
  return <String, Object?>{
    'status': 'consumed',
    'outcome': 'reset',
    'snapshot': <String, Object?>{
      'provider': 'codex',
      'accountId': 'default',
      'displayName': 'Codex',
      'status': 'ok',
      'updatedAt': 0,
      'windows': <Object?>[],
      'buckets': <Object?>[],
    },
  };
}

final class _MobileRuntimeTestServer {
  _MobileRuntimeTestServer(this._server, this._response);

  final HttpServer _server;
  final FutureOr<Map<String, Object?>> Function(
    String type,
    Map<String, Object?> payload,
  )
  _response;
  final List<WebSocket> _sockets = <WebSocket>[];
  final List<Map<String, Object?>> requests = <Map<String, Object?>>[];
  late final StreamSubscription<HttpRequest> _subscription;

  static Future<_MobileRuntimeTestServer> start(
    FutureOr<Map<String, Object?>> Function(
      String type,
      Map<String, Object?> payload,
    )
    response,
  ) async {
    final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    final testServer = _MobileRuntimeTestServer(server, response);
    testServer._subscription = server.listen(testServer._handleRequest);
    return testServer;
  }

  String get endpoint => 'ws://${_server.address.address}:${_server.port}';

  List<Map<String, Object?>> payloadsFor(String type) {
    return <Map<String, Object?>>[
      for (final request in requests)
        if (request['type'] == type)
          request['payload']! as Map<String, Object?>,
    ];
  }

  Future<void> _handleRequest(HttpRequest request) async {
    final socket = await WebSocketTransformer.upgrade(request);
    _sockets.add(socket);
    socket.listen((raw) async {
      final message = Map<String, Object?>.from(
        jsonDecode(raw as String) as Map,
      );
      final payload = Map<String, Object?>.from(message['payload'] as Map);
      final type = message['type'] as String;
      requests.add(<String, Object?>{...message, 'payload': payload});
      final response = await _response(type, payload);
      socket.add(
        jsonEncode(<String, Object?>{'id': message['id'], ...response}),
      );
    });
  }

  Future<void> dispose() async {
    await _subscription.cancel();
    for (final socket in _sockets) {
      await socket.close();
    }
    await _server.close(force: true);
  }
}
