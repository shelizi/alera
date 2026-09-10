import 'package:alera/src/features/workbench/application/workbench_workspace_tag_update_plan.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_tag_update_service.dart';
import 'package:alera/src/features/workbench/application/workspace_graph_repository.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('removes stale tags before assigning new tags', () async {
    final repository = _FakeWorkspaceTagAssignmentRepository();
    final service = WorkbenchWorkspaceTagUpdateService(repository);

    await service.apply(
      workspaceId: 'workspace',
      plan: const WorkbenchWorkspaceTagUpdatePlan(
        tagIdsToRemove: <String>{'old-1', 'old-2'},
        tagIdsToAdd: <String>{'new-1', 'new-2'},
      ),
    );

    expect(repository.events, <String>[
      'remove:workspace:old-1',
      'remove:workspace:old-2',
      'add:workspace:new-1',
      'add:workspace:new-2',
    ]);
  });

  test('empty diff does not touch persistence', () async {
    final repository = _FakeWorkspaceTagAssignmentRepository();
    final service = WorkbenchWorkspaceTagUpdateService(repository);

    await service.apply(
      workspaceId: 'workspace',
      plan: const WorkbenchWorkspaceTagUpdatePlan(
        tagIdsToRemove: <String>{},
        tagIdsToAdd: <String>{},
      ),
    );

    expect(repository.events, isEmpty);
  });

  test('remove failure stops before later removals and additions', () async {
    final repository = _FakeWorkspaceTagAssignmentRepository(
      failEvent: 'remove:workspace:old-1',
    );
    final service = WorkbenchWorkspaceTagUpdateService(repository);

    await expectLater(
      service.apply(
        workspaceId: 'workspace',
        plan: const WorkbenchWorkspaceTagUpdatePlan(
          tagIdsToRemove: <String>{'old-1', 'old-2'},
          tagIdsToAdd: <String>{'new-1'},
        ),
      ),
      throwsStateError,
    );

    expect(repository.events, <String>['remove:workspace:old-1']);
  });

  test(
    'add failure preserves completed removals and stops later additions',
    () async {
      final repository = _FakeWorkspaceTagAssignmentRepository(
        failEvent: 'add:workspace:new-1',
      );
      final service = WorkbenchWorkspaceTagUpdateService(repository);

      await expectLater(
        service.apply(
          workspaceId: 'workspace',
          plan: const WorkbenchWorkspaceTagUpdatePlan(
            tagIdsToRemove: <String>{'old-1'},
            tagIdsToAdd: <String>{'new-1', 'new-2'},
          ),
        ),
        throwsStateError,
      );

      expect(repository.events, <String>[
        'remove:workspace:old-1',
        'add:workspace:new-1',
      ]);
    },
  );
}

final class _FakeWorkspaceTagAssignmentRepository
    implements WorkspaceTagAssignmentRepository {
  _FakeWorkspaceTagAssignmentRepository({this.failEvent});

  final String? failEvent;
  final List<String> events = <String>[];

  @override
  Future<void> assignTag({
    required String workspaceId,
    required String tagId,
  }) async {
    final event = 'add:$workspaceId:$tagId';
    events.add(event);
    if (event == failEvent) throw StateError('assign failed');
  }

  @override
  Future<void> unassignTag({
    required String workspaceId,
    required String tagId,
  }) async {
    final event = 'remove:$workspaceId:$tagId';
    events.add(event);
    if (event == failEvent) throw StateError('unassign failed');
  }
}
