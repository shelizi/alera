import 'dart:convert';
import 'dart:io';

import 'package:alera_mobile/src/features/runtime/infra/mobile_runtime_client.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'mobile directory and guarded write use capability-gated RPCs',
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
      server.listen((request) async {
        final socket = await WebSocketTransformer.upgrade(request);
        sockets.add(socket);
        socket.listen((raw) {
          final message = jsonDecode(raw as String) as Map<String, Object?>;
          requests.add(message);
          final type = message['type'];
          final payload = message['payload'];
          final isConflict =
              type == 'mobile.workspaceFile.write' &&
              payload is Map &&
              payload['text'] == 'stale';
          socket.add(
            jsonEncode(<String, Object?>{
              'id': message['id'],
              'ok': !isConflict,
              if (isConflict) ...<String, Object?>{
                'error': 'Workspace file changed since it was opened',
                'errorCode': 'mobile_workspace_file_changed',
                'errorDetails': <String, Object?>{
                  'relativePath': 'src/main.dart',
                },
              } else
                'payload': switch (type) {
                  'mobile.hello' => <String, Object?>{
                    'runtimeCapabilities': <String>[
                      mobileWorkspaceDirectoryBrowseCapability,
                      mobileWorkspaceTextEditCapability,
                    ],
                  },
                  'mobile.workspaceDirectory.list' => <String, Object?>{
                    'items': <Object?>[
                      <String, Object?>{
                        'name': 'main.dart',
                        'relativePath': 'src/main.dart',
                        'isDirectory': false,
                        'sizeBytes': 12,
                      },
                    ],
                    'nextOffset': 1,
                    'hasMore': false,
                  },
                  'mobile.workspaceFile.write' => <String, Object?>{
                    'relativePath': 'src/main.dart',
                    'sha256': 'new-digest',
                    'sizeBytes': 6,
                  },
                  _ => <String, Object?>{},
                },
            }),
          );
        });
      });
      final client = await MobileRuntimeClient.connect(
        'ws://${server.address.address}:${server.port}',
      );
      addTearDown(client.dispose);
      expect(client.supportsWorkspaceDirectoryBrowse, isFalse);
      expect(client.supportsWorkspaceTextEdit, isFalse);
      await client.authenticate(deviceId: 'phone', deviceToken: 'token');
      expect(client.supportsWorkspaceDirectoryBrowse, isTrue);
      expect(client.supportsWorkspaceTextEdit, isTrue);
      final page = await client.listWorkspaceDirectory(
        workspaceId: 'workspace-1',
        relativePath: 'src',
        showHidden: true,
      );
      expect(page.items.single.relativePath, 'src/main.dart');
      final written = await client.saveWorkspaceText(
        workspaceId: 'workspace-1',
        relativePath: 'src/main.dart',
        expectedSha256: 'old-digest',
        text: 'edited',
      );
      expect(written.sizeBytes, 6);
      expect(
        requests.singleWhere(
          (e) => e['type'] == 'mobile.workspaceFile.write',
        )['payload'],
        <String, Object?>{
          'workspaceId': 'workspace-1',
          'relativePath': 'src/main.dart',
          'expectedSha256': 'old-digest',
          'text': 'edited',
        },
      );
      expect(
        requests.singleWhere(
          (e) => e['type'] == 'mobile.workspaceDirectory.list',
        )['payload'],
        <String, Object?>{
          'workspaceId': 'workspace-1',
          'relativePath': 'src',
          'offset': 0,
          'limit': 100,
          'showHidden': true,
        },
      );
      await expectLater(
        client.saveWorkspaceText(
          workspaceId: 'workspace-1',
          relativePath: 'src/main.dart',
          expectedSha256: 'old-digest',
          text: 'stale',
        ),
        throwsA(
          isA<MobileRuntimeConflictException>().having(
            (error) => error.code,
            'code',
            'mobile_workspace_file_changed',
          ),
        ),
      );
      await pumpEventQueue();
    },
  );
}
