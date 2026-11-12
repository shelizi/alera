import 'package:alera/src/features/workbench/infra/runtime_workbench_repository.dart';
import 'package:alera/src/features/workbench/application/workspace_tab_service.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/platform/runtime_host/protocol/terminal_host_protocol.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('RuntimeWorkbenchRepository maps workspace pin updates', () async {
    final client = _PinningRuntimeHostClient();
    final repository = RuntimeWorkbenchRepository(client);

    final workspace = await repository.setWorkspacePinned('workspace-1', true);

    expect(workspace.isPinned, isTrue);
    expect(client.type, 'workspace.setPinned');
    expect(client.payload, <String, Object?>{
      'id': 'workspace-1',
      'isPinned': true,
    });
  });

  test(
    'Host Tab pin uses identity/revision CAS, never whole Tab upsert',
    () async {
      final client = _PinningRuntimeHostClient();
      final repository = RuntimeWorkbenchRepository(client);
      final original = WorkspaceTabRecord(
        id: 'tab-1',
        workspaceId: 'workspace-1',
        kind: WorkspaceTabKind.terminal,
        title: 'Codex',
        createdAt: DateTime.utc(2026, 11, 9),
        updatedAt: DateTime.utc(2026, 11, 9, 14),
        payload: const {'terminalSessionId': 'pty-1', 'preview': true},
      );
      final pinned = await repository.compareAndSetWorkspaceTabPinned(
        original,
        true,
      );
      expect(pinned.isPinned, isTrue);
      expect(pinned.isPreview, isFalse);
      expect(client.type, 'tab.pin.cas');
      expect(client.payload, {
        'workspaceId': 'workspace-1',
        'id': 'tab-1',
        'expectedUpdatedAt': '2026-11-09T14:00:00.000Z',
        'expectedPinned': false,
        'pinned': true,
        'expectedTerminalSessionId': 'pty-1',
      });
      expect(client.payload!.containsKey('payload'), isFalse);
    },
  );

  test('Classic pin then unpin reads fresh Host revision each time', () async {
    final client = _PinningRuntimeHostClient();
    final service = WorkspaceTabService(
      repository: RuntimeWorkbenchRepository(client),
    );
    final pinned = await service.setTabPinned(tabId: 'tab-1', pinned: true);
    final unpinned = await service.setTabPinned(tabId: 'tab-1', pinned: false);
    expect(pinned.isPinned, isTrue);
    expect(unpinned.isPinned, isFalse);
    expect(unpinned.isPreview, isFalse);
    expect(unpinned.updatedAt.isAfter(pinned.updatedAt), isTrue);
    expect(client.types, [
      'tab.find',
      'tab.pin.cas',
      'tab.find',
      'tab.pin.cas',
    ]);
    expect(
      client.payload!['expectedUpdatedAt'],
      pinned.updatedAt.toIso8601String(),
    );
    expect(client.payload!['expectedPinned'], true);
    expect(client.payload!['pinned'], false);
    expect(client.payload!.containsKey('payload'), isFalse);
  });
}

final class _PinningRuntimeHostClient implements RuntimeHostClient {
  String? type;
  Map<String, Object?>? payload;
  final List<String> types = [];
  String _tabUpdatedAt = '2026-11-09T14:00:00.000Z';
  bool _tabPinned = false;
  bool _tabPreview = true;

  Map<String, Object?> get _tab => <String, Object?>{
    'id': 'tab-1',
    'workspaceId': 'workspace-1',
    'kind': 'terminal',
    'title': 'Codex',
    'createdAt': '2026-11-09T00:00:00.000Z',
    'updatedAt': _tabUpdatedAt,
    'payload': <String, Object?>{
      'terminalSessionId': 'pty-1',
      if (_tabPinned) 'pinned': true,
      if (_tabPreview) 'preview': true,
    },
  };

  @override
  Stream<RuntimeHostEvent> get runtimeEvents => const Stream.empty();

  @override
  Future<Object?> runtimeRequest(
    String type, [
    Map<String, Object?> payload = const <String, Object?>{},
    Duration? timeout,
  ]) async {
    this.type = type;
    this.payload = payload;
    types.add(type);
    if (type == 'tab.find') {
      return _tab;
    }
    if (type == 'tab.pin.cas') {
      if (payload['id'] != 'tab-1' ||
          payload['workspaceId'] != 'workspace-1' ||
          payload['expectedTerminalSessionId'] != 'pty-1' ||
          payload['expectedUpdatedAt'] != _tabUpdatedAt ||
          payload['expectedPinned'] != _tabPinned) {
        throw StateError('Host refused stale pin CAS');
      }
      _tabPinned = payload['pinned'] == true;
      if (_tabPinned) _tabPreview = false;
      _tabUpdatedAt = DateTime.parse(_tabUpdatedAt)
          .add(const Duration(milliseconds: 1))
          .toUtc()
          .toIso8601String();
      return _tab;
    }
    return <String, Object?>{
      'id': 'workspace-1',
      'instanceId': 'instance-workspace-1',
      'hostId': 'local',
      'projectId': 'project-1',
      'name': 'Feature',
      'branch': 'feature/pins',
      'path': '/tmp/workspace-1',
      'createdAt': '2026-07-16T00:00:00.000Z',
      'updatedAt': '2026-07-16T00:00:00.000Z',
      'kind': 'linked',
      'status': 'active',
      'sourceBranch': 'main',
      'reusesExistingBranch': false,
      'isPinned': true,
      'tagIds': <String>[],
      'tagNames': <String>[],
      'parentWorkspaceId': null,
      'childCount': 0,
    };
  }
}
