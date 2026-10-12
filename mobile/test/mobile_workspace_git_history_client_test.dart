import 'dart:convert';
import 'dart:io';

import 'package:alera_mobile/src/features/runtime/domain/mobile_codex_workspace.dart';
import 'package:alera_mobile/src/features/runtime/infra/mobile_runtime_client.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('Git history models retain branch, commit details and parents', () {
    final history = MobileWorkspaceGitHistory.fromJson(<String, Object?>{
      'branch': 'feature/mobile',
      'commits': <Object?>[
        <String, Object?>{
          'id': 'abcdef0123456789',
          'summary': 'Create feature',
          'message': 'Create feature\n\nDetails.',
          'author': 'Developer',
          'timestamp': 1791651000,
          'parents': <String>['1234567'],
        },
      ],
    });
    expect(history.branch, 'feature/mobile');
    expect(history.commits.single.id, 'abcdef0123456789');
    expect(history.commits.single.message, contains('Details.'));
    expect(history.commits.single.author, 'Developer');
    expect(history.commits.single.parents, <String>['1234567']);
  });

  test(
    'history requests are authenticated, scoped and capability gated',
    () async {
      final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
      final sockets = <WebSocket>[];
      final requests = <Map<String, Object?>>[];
      addTearDown(() async {
        for (final socket in sockets) {
          await socket.close();
        }
        await server.close(force: true);
      });
      final subscription = server.listen((request) async {
        final socket = await WebSocketTransformer.upgrade(request);
        sockets.add(socket);
        socket.listen((raw) {
          final request = jsonDecode(raw as String) as Map<String, Object?>;
          requests.add(request);
          if (socket.readyState != WebSocket.open) return;
          socket.add(
            jsonEncode(<String, Object?>{
              'id': request['id'],
              'ok': true,
              'payload': request['type'] == 'mobile.hello'
                  ? <String, Object?>{
                      'runtimeCapabilities': <String>[
                        mobileWorkspaceGitHistoryCapability,
                      ],
                    }
                  : <String, Object?>{
                      'branch': 'feature/mobile',
                      'commits': <Object?>[
                        <String, Object?>{
                          'id': 'abc123456789',
                          'summary': 'One commit',
                          'message': 'One commit',
                          'author': 'Developer',
                          'timestamp': 123456,
                          'parents': <String>[],
                        },
                      ],
                    },
            }),
          );
        });
      });
      addTearDown(subscription.cancel);
      final client = await MobileRuntimeClient.connect(
        'ws://${server.address.address}:${server.port}',
      );
      addTearDown(client.dispose);
      expect(client.supportsWorkspaceGitHistory, isFalse);
      await client.authenticate(deviceId: 'phone', deviceToken: 'token');
      expect(client.supportsWorkspaceGitHistory, isTrue);
      final history = await client.loadWorkspaceGitHistory(
        'workspace-1',
        limit: 20,
      );
      expect(history.commits.single.summary, 'One commit');
      final sent = requests.singleWhere(
        (entry) => entry['type'] == 'mobile.workspaceGit.history',
      );
      expect(sent['payload'], <String, Object?>{
        'workspaceId': 'workspace-1',
        'limit': 20,
      });
      await expectLater(
        client.loadWorkspaceGitHistory('workspace-1', limit: 101),
        throwsArgumentError,
      );
    },
  );
}
