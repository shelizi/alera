import 'dart:convert';
import 'dart:io';

import 'package:alera_mobile/src/features/runtime/infra/mobile_runtime_client.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('mobile rename uses selected workspace and expected SHA without overwrite flag', () async {
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
        final conflicting =
            command['type'] == 'mobile.workspaceFile.rename' &&
            (command['payload'] as Map?)?['newName'] == 'exists.md';
        socket.add(
          jsonEncode(<String, Object?>{
            'id': command['id'],
            'ok': !conflicting,
            if (conflicting) ...<String, Object?>{
              'error': 'Destination exists',
              'errorCode': 'mobile_workspace_file_rename_conflict',
            } else
              'payload': command['type'] == 'mobile.hello'
                  ? <String, Object?>{
                      'runtimeCapabilities': <String>[
                        mobileWorkspaceTextRenameCapability,
                      ],
                    }
                  : <String, Object?>{'relativePath': 'src/renamed.md'},
          }),
        );
      });
    });
    addTearDown(subscription.cancel);
    final client = await MobileRuntimeClient.connect(
      'ws://${server.address.address}:${server.port}',
    );
    addTearDown(client.dispose);
    expect(client.supportsWorkspaceTextRename, isFalse);
    await client.authenticate(deviceId: 'phone', deviceToken: 'token');
    expect(client.supportsWorkspaceTextRename, isTrue);
    final renamed = await client.renameWorkspaceText(
      workspaceId: 'workspace-1',
      relativePath: 'src/notes.md',
      newName: 'renamed.md',
      expectedSha256: 'sha256-expected',
    );
    expect(renamed, 'src/renamed.md');
    expect(
      requests.singleWhere(
        (request) => request['type'] == 'mobile.workspaceFile.rename',
      )['payload'],
      <String, Object?>{
        'workspaceId': 'workspace-1',
        'relativePath': 'src/notes.md',
        'newName': 'renamed.md',
        'expectedSha256': 'sha256-expected',
      },
    );
    await expectLater(
      client.renameWorkspaceText(
        workspaceId: 'workspace-1',
        relativePath: 'src/notes.md',
        newName: 'exists.md',
        expectedSha256: 'sha256-expected',
      ),
      throwsA(
        isA<MobileRuntimeConflictException>().having(
          (error) => error.code,
          'code',
          'mobile_workspace_file_rename_conflict',
        ),
      ),
    );
  });
}
