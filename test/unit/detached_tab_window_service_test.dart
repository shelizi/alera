import 'dart:async';

import 'package:alera/src/features/workbench/application/detached_tab_window_service.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final now = DateTime.utc(2026, 10, 20);

  DetachedTabDescriptor descriptor() => DetachedTabDescriptor(
    workspace: Workspace(
      id: 'workspace',
      projectId: 'project',
      name: 'test',
      path: r'C:\work\example',
      createdAt: now,
      updatedAt: now,
      kind: WorkspaceKind.main,
      status: WorkspaceStatus.active,
    ),
    tab: WorkspaceTabRecord(
      id: 'file-tab',
      workspaceId: 'workspace',
      title: 'readme.md',
      createdAt: now,
      updatedAt: now,
      kind: WorkspaceTabKind.markdownViewer,
      payload: const {'filePath': 'readme.md'},
    ),
  );

  test('child arguments accept one well-formed one-time capability', () {
    final sample = List<String>.filled(40, 'a').join();
    final accepted = DetachedTabLaunchArguments.parse([
      '--alera-detached-tab',
      '5123',
      sample,
    ]);
    expect(accepted, isNotNull);
    expect(accepted!.port, 5123);
    expect(accepted.token, sample);
    expect(
      DetachedTabLaunchArguments.parse(['--alera-detached-tab', '0', sample]),
      isNull,
    );
    expect(
      DetachedTabLaunchArguments.parse([
        '--alera-detached-tab',
        '5123',
        'malformed',
      ]),
      isNull,
    );
  });

  test('start failure leaves the original tab visible', () async {
    final service = DetachedTabWindowService(
      starter: (_, _) => Future<void>.error(StateError('launch failed')),
    );
    addTearDown(service.dispose);

    await expectLater(service.detach(descriptor()), throwsStateError);
    expect(service.isDetached('file-tab'), isFalse);
    expect(service.isLaunching('file-tab'), isFalse);
  });

  test('handshake hides only after child READY and restores on dock', () async {
    final childResult = Completer<DetachedTabWindowClient>();
    final service = DetachedTabWindowService(
      starter: (port, token) async {
        unawaited(
          DetachedTabWindowClient.connect(
            DetachedTabLaunchArguments(port, token),
          ).then(childResult.complete, onError: childResult.completeError),
        );
      },
    );
    addTearDown(service.dispose);
    final openedFiles = <String>[];
    service.onOpenFile = (_, path) async => openedFiles.add(path);

    final detached = service.detach(descriptor());
    await Future<void>.delayed(const Duration(milliseconds: 25));
    expect(service.isDetached('file-tab'), isFalse);

    final child = await childResult.future;
    child.ready();
    expect(await detached, isTrue);
    expect(service.isDetached('file-tab'), isTrue);
    expect(service.detachedTabIds, contains('file-tab'));

    child.requestOpenFile('docs/next.md');
    child.requestOpenFile('../outside-workspace.md');
    for (var i = 0; i < 30 && openedFiles.isEmpty; i++) {
      await Future<void>.delayed(const Duration(milliseconds: 10));
    }
    expect(openedFiles, <String>['docs/next.md']);

    await child.dock();
    for (var i = 0; i < 30 && service.isDetached('file-tab'); i++) {
      await Future<void>.delayed(const Duration(milliseconds: 10));
    }
    expect(service.isDetached('file-tab'), isFalse);
  });
}
