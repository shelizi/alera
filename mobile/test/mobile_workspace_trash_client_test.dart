import 'dart:convert';
import 'dart:io';

import 'package:alera_mobile/src/features/runtime/infra/mobile_runtime_client.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'mobile trash, listing and restore are workspace-scoped and gated',
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
          final command = jsonDecode(raw as String) as Map<String, Object?>;
          requests.add(command);
          if (socket.readyState != WebSocket.open) return;
          final type = command['type'];
          final conflict =
              type == 'mobile.workspaceTrash.restore' &&
              (command['payload'] as Map?)?['trashId'] == 'occupied';
          final entry = <String, Object?>{
            'id': 'a75be2ce-aed0-4b95-90b6-7c81dd034e0b',
            'relativePath': 'docs/test.md',
            'sha256': 'abc',
            'sizeBytes': 3,
            'deletedAtMs': 1791674400000,
          };
          socket.add(
            jsonEncode(<String, Object?>{
              'id': command['id'],
              'ok': !conflict,
              if (conflict) ...<String, Object?>{
                'error': 'Destination already exists',
                'errorCode': 'mobile_workspace_trash_restore_conflict',
              } else
                'payload': switch (type) {
                  'mobile.hello' => <String, Object?>{
                    'runtimeCapabilities': <String>[
                      mobileWorkspaceTextTrashCapability,
                    ],
                  },
                  'mobile.workspaceFile.trash' => entry,
                  'mobile.workspaceTrash.list' => <String, Object?>{
                    'items': <Object?>[entry],
                    'limit': 64,
                    'capacityBytes': 16 * 1024 * 1024,
                  },
                  _ => <String, Object?>{'relativePath': 'docs/test.md'},
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
      expect(client.supportsWorkspaceTextTrash, isFalse);
      await client.authenticate(deviceId: 'phone', deviceToken: 'token');
      expect(client.supportsWorkspaceTextTrash, isTrue);
      final moved = await client.moveWorkspaceTextToTrash(
        workspaceId: 'workspace-1',
        relativePath: 'docs/test.md',
        expectedSha256: 'abc',
      );
      expect(moved.relativePath, 'docs/test.md');
      expect(moved.sizeBytes, 3);
      final list = await client.listWorkspaceTextTrash('workspace-1');
      expect(list.items.single.id, moved.id);
      expect(list.limit, 64);
      expect(list.capacityBytes, 16 * 1024 * 1024);
      final restored = await client.restoreWorkspaceTextFromTrash(
        workspaceId: 'workspace-1',
        trashId: moved.id,
      );
      expect(restored, 'docs/test.md');
      expect(
        requests.singleWhere(
          (request) => request['type'] == 'mobile.workspaceFile.trash',
        )['payload'],
        <String, Object?>{
          'workspaceId': 'workspace-1',
          'relativePath': 'docs/test.md',
          'expectedSha256': 'abc',
        },
      );
      expect(
        requests.singleWhere(
          (request) => request['type'] == 'mobile.workspaceTrash.list',
        )['payload'],
        <String, Object?>{'workspaceId': 'workspace-1'},
      );
      expect(
        requests.singleWhere(
          (request) => request['type'] == 'mobile.workspaceTrash.restore',
        )['payload'],
        <String, Object?>{'workspaceId': 'workspace-1', 'trashId': moved.id},
      );
      await expectLater(
        client.restoreWorkspaceTextFromTrash(
          workspaceId: 'workspace-1',
          trashId: 'occupied',
        ),
        throwsA(
          isA<MobileRuntimeConflictException>().having(
            (error) => error.code,
            'conflict code',
            'mobile_workspace_trash_restore_conflict',
          ),
        ),
      );
    },
  );
}
