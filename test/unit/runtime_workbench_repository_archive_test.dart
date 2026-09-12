import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/infra/runtime_workbench_repository.dart';
import 'package:alera/src/platform/runtime_host/protocol/terminal_host_protocol.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'RuntimeWorkbenchRepository sends and reads archivedAt on upsert',
    () async {
      final client = _ArchiveRuntimeHostClient();
      final repository = RuntimeWorkbenchRepository(client);
      final archivedAt = DateTime.utc(2026, 8, 20, 8, 30);

      final workspace = await repository.upsertWorkspace(
        _workspace(archivedAt: archivedAt),
      );

      expect(client.type, 'workspace.upsert');
      expect(client.payload?['archivedAt'], '2026-08-20T08:30:00.000Z');
      expect(workspace.isArchived, isTrue);
      expect(workspace.archivedAt, archivedAt);
    },
  );

  test('upsert without archivedAt clears the flag over the wire', () async {
    final client = _ArchiveRuntimeHostClient();
    final repository = RuntimeWorkbenchRepository(client);

    final workspace = await repository.upsertWorkspace(_workspace());

    expect(client.payload?['archivedAt'], isNull);
    expect(workspace.isArchived, isFalse);
  });
}

Workspace _workspace({DateTime? archivedAt}) {
  return Workspace(
    id: 'workspace-1',
    projectId: 'project-1',
    name: 'Feature',
    branch: 'feature/archive',
    path: '/tmp/workspace-1',
    createdAt: DateTime.utc(2026, 7, 16),
    updatedAt: DateTime.utc(2026, 7, 16),
    kind: .linked,
    status: .active,
    archivedAt: archivedAt,
  );
}

final class _ArchiveRuntimeHostClient implements RuntimeHostClient {
  String? type;
  Map<String, Object?>? payload;

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
    return <String, Object?>{
      ...payload,
      'hostId': 'local',
      'instanceId': 'instance-workspace-1',
    };
  }
}
