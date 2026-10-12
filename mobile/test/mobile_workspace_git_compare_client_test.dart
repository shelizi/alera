import 'dart:convert';
import 'dart:io';

import 'package:alera_mobile/src/features/runtime/domain/mobile_codex_workspace.dart';
import 'package:alera_mobile/src/features/runtime/infra/mobile_runtime_client.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('Git changed-file comparison parses paths, statuses and truncation', () {
    final result = MobileWorkspaceGitComparison.fromJson(<String, Object?>{
      'baseCommit': null,
      'targetCommit': 'abc',
      'items': <Object?>[
        <String, Object?>{
          'relativePath': 'lib/a.dart',
          'oldPath': 'lib/a.dart',
          'newPath': 'lib/a.dart',
          'status': 'modified',
        },
      ],
      'truncated': true,
    });
    expect(result.baseCommit, isNull);
    expect(result.targetCommit, 'abc');
    expect(result.items.single.relativePath, 'lib/a.dart');
    expect(result.items.single.status, 'modified');
    expect(result.truncated, isTrue);
  });

  test(
    'commit comparison RPC is capability gated and workspace-scoped',
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
          socket.add(
            jsonEncode(<String, Object?>{
              'id': message['id'],
              'ok': true,
              'payload': message['type'] == 'mobile.hello'
                  ? <String, Object?>{
                      'runtimeCapabilities': <String>[
                        mobileWorkspaceGitCompareCapability,
                      ],
                    }
                  : message['type'] == 'mobile.workspaceGit.revisionPatch'
                  ? <String, Object?>{
                      'baseCommit': 'older',
                      'targetCommit': 'newer',
                      'relativePath': 'lib/a.dart',
                      'patch': '-old\n+new\n',
                      'truncated': false,
                    }
                  : <String, Object?>{
                      'baseCommit': 'older',
                      'targetCommit': 'newer',
                      'items': <Object?>[],
                      'truncated': false,
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
      expect(client.supportsWorkspaceGitCompare, isFalse);
      await client.authenticate(deviceId: 'phone', deviceToken: 'token');
      expect(client.supportsWorkspaceGitCompare, isTrue);
      await client.compareWorkspaceGitCommits(
        workspaceId: 'workspace-1',
        baseCommit: 'older',
        targetCommit: 'newer',
      );
      await client.compareWorkspaceGitCommits(
        workspaceId: 'workspace-1',
        targetCommit: 'newer',
      );
      final patch = await client.loadWorkspaceGitRevisionPatch(
        workspaceId: 'workspace-1',
        baseCommit: 'older',
        targetCommit: 'newer',
        relativePath: 'lib/a.dart',
      );
      expect(patch.patch, contains('+new'));
      expect(patch.relativePath, 'lib/a.dart');
      expect(patch.truncated, isFalse);
      final sent = requests
          .where((entry) => entry['type'] == 'mobile.workspaceGit.compare')
          .toList();
      expect(sent, hasLength(2));
      expect(sent.first['payload'], <String, Object?>{
        'workspaceId': 'workspace-1',
        'baseCommit': 'older',
        'targetCommit': 'newer',
      });
      expect(sent.last['payload'], <String, Object?>{
        'workspaceId': 'workspace-1',
        'targetCommit': 'newer',
      });
      final patchRequest = requests.singleWhere(
        (entry) => entry['type'] == 'mobile.workspaceGit.revisionPatch',
      );
      expect(patchRequest['payload'], <String, Object?>{
        'workspaceId': 'workspace-1',
        'baseCommit': 'older',
        'targetCommit': 'newer',
        'relativePath': 'lib/a.dart',
      });
    },
  );
}
