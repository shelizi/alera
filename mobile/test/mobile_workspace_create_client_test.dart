import 'dart:convert';
import 'dart:io';

import 'package:alera_mobile/src/features/runtime/infra/mobile_runtime_client.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'creating mobile text is gated and does not request overwrite',
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
          final message = jsonDecode(raw as String) as Map<String, Object?>;
          requests.add(message);
          if (socket.readyState != WebSocket.open) return;
          final conflict =
              message['type'] == 'mobile.workspaceFile.create' &&
              (message['payload'] as Map?)?['relativePath'] ==
                  'src/existing.md';
          socket.add(
            jsonEncode(<String, Object?>{
              'id': message['id'],
              'ok': !conflict,
              if (conflict) ...<String, Object?>{
                'error': 'File already exists',
                'errorCode': 'mobile_workspace_file_exists',
                'errorDetails': <String, Object?>{
                  'relativePath': 'src/existing.md',
                },
              } else
                'payload': message['type'] == 'mobile.hello'
                    ? <String, Object?>{
                        'runtimeCapabilities': <String>[
                          mobileWorkspaceTextCreateCapability,
                        ],
                      }
                    : <String, Object?>{
                        'relativePath': 'src/new.md',
                        'sha256': 'new-digest',
                        'sizeBytes': 4,
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
      expect(client.supportsWorkspaceTextCreate, isFalse);
      await client.authenticate(deviceId: 'phone', deviceToken: 'token');
      expect(client.supportsWorkspaceTextCreate, isTrue);
      final result = await client.createWorkspaceText(
        workspaceId: 'workspace-1',
        relativePath: 'src/new.md',
        text: 'new\n',
      );
      expect(result.relativePath, 'src/new.md');
      expect(result.sizeBytes, 4);
      final sent = requests.singleWhere(
        (request) => request['type'] == 'mobile.workspaceFile.create',
      );
      expect(sent['payload'], <String, Object?>{
        'workspaceId': 'workspace-1',
        'relativePath': 'src/new.md',
        'text': 'new\n',
      });
      await expectLater(
        client.createWorkspaceText(
          workspaceId: 'workspace-1',
          relativePath: 'src/existing.md',
          text: 'x',
        ),
        throwsA(
          isA<MobileRuntimeConflictException>().having(
            (error) => error.code,
            'error code',
            'mobile_workspace_file_exists',
          ),
        ),
      );
      await expectLater(
        client.createWorkspaceText(
          workspaceId: 'workspace-1',
          relativePath: 'src/too-large.md',
          text: 'x' * (256 * 1024 + 1),
        ),
        throwsArgumentError,
      );
    },
  );
}
