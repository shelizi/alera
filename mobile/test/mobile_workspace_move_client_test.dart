import 'dart:convert';
import 'dart:io';

import 'package:alera_mobile/src/features/runtime/infra/mobile_runtime_client.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('cross-folder move uses workspace, destination and SHA with typed conflicts', () async {
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
        final collision =
            type == 'mobile.workspaceFile.move' &&
            (command['payload'] as Map?)?['destinationDirectory'] == 'occupied';
        socket.add(
          jsonEncode(<String, Object?>{
            'id': command['id'],
            'ok': !collision,
            if (collision) ...<String, Object?>{
              'error': 'Destination already exists',
              'errorCode': 'mobile_workspace_file_move_conflict',
            } else
              'payload': type == 'mobile.hello'
                  ? <String, Object?>{
                      'runtimeCapabilities': <String>[
                        mobileWorkspaceTextMoveCapability,
                      ],
                    }
                  : <String, Object?>{'relativePath': 'src/demo.md'},
          }),
        );
      });
    });
    addTearDown(subscription.cancel);
    final client = await MobileRuntimeClient.connect(
      'ws://${server.address.address}:${server.port}',
    );
    addTearDown(client.dispose);
    expect(client.supportsWorkspaceTextMove, isFalse);
    await client.authenticate(deviceId: 'phone', deviceToken: 'token');
    expect(client.supportsWorkspaceTextMove, isTrue);
    final destination = await client.moveWorkspaceText(
      workspaceId: 'workspace-1',
      relativePath: 'notes/demo.md',
      destinationDirectory: 'src',
      expectedSha256: 'digest-current',
    );
    expect(destination, 'src/demo.md');
    expect(
      requests.singleWhere(
        (request) => request['type'] == 'mobile.workspaceFile.move',
      )['payload'],
      <String, Object?>{
        'workspaceId': 'workspace-1',
        'relativePath': 'notes/demo.md',
        'destinationDirectory': 'src',
        'expectedSha256': 'digest-current',
      },
    );
    // Workspace root is represented by an explicit empty directory string;
    // callers never ask for arbitrary absolute host paths.
    await client.moveWorkspaceText(
      workspaceId: 'workspace-1',
      relativePath: 'notes/demo.md',
      destinationDirectory: '',
      expectedSha256: 'digest-current',
    );
    expect((requests.last['payload'] as Map)['destinationDirectory'], '');
    await expectLater(
      client.moveWorkspaceText(
        workspaceId: 'workspace-1',
        relativePath: 'notes/demo.md',
        destinationDirectory: 'occupied',
        expectedSha256: 'digest-current',
      ),
      throwsA(
        isA<MobileRuntimeConflictException>().having(
          (error) => error.code,
          'conflict code',
          'mobile_workspace_file_move_conflict',
        ),
      ),
    );
  });
}
