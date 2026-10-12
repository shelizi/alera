import 'dart:convert';
import 'dart:io';

import 'package:alera_mobile/src/features/runtime/domain/mobile_codex_workspace.dart';
import 'package:alera_mobile/src/features/runtime/infra/mobile_runtime_client.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('status parses independent staged and unstaged Git states', () {
    final status = MobileWorkspaceGitStatus.fromJson(<String, Object?>{
      'items': <Object?>[
        <String, Object?>{
          'relativePath': 'src/main.dart',
          'status': 'modified',
          'staged': true,
          'unstaged': true,
        },
      ],
      'truncated': false,
    });
    expect(status.items.single.staged, isTrue);
    expect(status.items.single.unstaged, isTrue);
  });

  test(
    'Git staging requires negotiated capability and targets one file',
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
          final req = jsonDecode(raw as String) as Map<String, Object?>;
          requests.add(req);
          socket.add(
            jsonEncode(<String, Object?>{
              'id': req['id'],
              'ok': true,
              'payload': req['type'] == 'mobile.hello'
                  ? <String, Object?>{
                      'runtimeCapabilities': <String>[
                        mobileWorkspaceGitStageCapability,
                      ],
                    }
                  : <String, Object?>{},
            }),
          );
        });
      });
      addTearDown(subscription.cancel);
      final client = await MobileRuntimeClient.connect(
        'ws://${server.address.address}:${server.port}',
      );
      addTearDown(client.dispose);
      expect(client.supportsWorkspaceGitStage, isFalse);
      await client.authenticate(deviceId: 'phone', deviceToken: 'token');
      expect(client.supportsWorkspaceGitStage, isTrue);
      await client.stageWorkspaceFile(
        workspaceId: 'ws-1',
        relativePath: 'src/main.dart',
        stage: true,
      );
      await client.stageWorkspaceFile(
        workspaceId: 'ws-1',
        relativePath: 'src/main.dart',
        stage: false,
      );
      final commands = requests
          .where((req) => req['type'] == 'mobile.workspaceGit.stage')
          .toList();
      expect(commands, hasLength(2));
      expect(commands.map((req) => req['payload']), <Object?>[
        <String, Object?>{
          'workspaceId': 'ws-1',
          'relativePath': 'src/main.dart',
          'stage': true,
        },
        <String, Object?>{
          'workspaceId': 'ws-1',
          'relativePath': 'src/main.dart',
          'stage': false,
        },
      ]);
    },
  );
}
