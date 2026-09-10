import 'package:alera/src/features/workbench/application/workbench_workspace_parent_update_service.dart';
import 'package:alera/src/features/workbench/application/workspace_graph_repository.dart';
import 'package:alera/src/features/workbench/application/workspace_service.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'no-ops normalized equivalent parent values without repository calls',
    () async {
      final repo = _FakeWorkspaceParentRepository();
      final service = WorkbenchWorkspaceParentUpdateService(repo);
      final workspace = _workspace(parentWorkspaceId: ' parent ');

      await service.update(workspace: workspace, parentWorkspaceId: 'parent');

      expect(repo.calls, isEmpty);
    },
  );

  test('clearing a parent only unlinks the current parent', () async {
    final repo = _FakeWorkspaceParentRepository();
    final service = WorkbenchWorkspaceParentUpdateService(repo);
    final workspace = _workspace(parentWorkspaceId: 'parent-old');

    await service.update(workspace: workspace, parentWorkspaceId: '   ');

    expect(repo.calls, <String>['unlink:parent-old->workspace']);
  });

  test('rejects self parent before loading relations', () async {
    final repo = _FakeWorkspaceParentRepository();
    final service = WorkbenchWorkspaceParentUpdateService(repo);

    await expectLater(
      service.update(workspace: _workspace(), parentWorkspaceId: ' workspace '),
      throwsA(
        isA<WorkspaceException>().having(
          (error) => error.toString(),
          'message',
          contains('cannot be its own parent'),
        ),
      ),
    );

    expect(repo.calls, isEmpty);
  });

  test('rejects a descendant using fresh relations before unlinking', () async {
    final repo = _FakeWorkspaceParentRepository()
      ..relations = <WorkspaceRelation>[
        _relation('workspace', 'child'),
        _relation('child', 'grandchild'),
      ];
    final service = WorkbenchWorkspaceParentUpdateService(repo);

    await expectLater(
      service.update(
        workspace: _workspace(parentWorkspaceId: 'parent-old'),
        parentWorkspaceId: 'grandchild',
      ),
      throwsA(
        isA<WorkspaceException>().having(
          (error) => error.toString(),
          'message',
          contains('descendant workspace'),
        ),
      ),
    );

    expect(repo.calls, <String>['list']);
  });

  test('replaces a parent after validating fresh relations', () async {
    final repo = _FakeWorkspaceParentRepository();
    final service = WorkbenchWorkspaceParentUpdateService(repo);

    await service.update(
      workspace: _workspace(parentWorkspaceId: 'parent-old'),
      parentWorkspaceId: ' parent-new ',
    );

    expect(repo.calls, <String>[
      'list',
      'unlink:parent-old->workspace',
      'link:parent-new->workspace',
    ]);
  });

  test(
    'restores the previous parent when linking the replacement fails',
    () async {
      final linkError = StateError('new parent missing');
      final repo = _FakeWorkspaceParentRepository()
        ..linkErrorsByParent['parent-new'] = linkError;
      final service = WorkbenchWorkspaceParentUpdateService(repo);

      await expectLater(
        service.update(
          workspace: _workspace(parentWorkspaceId: 'parent-old'),
          parentWorkspaceId: 'parent-new',
        ),
        throwsA(same(linkError)),
      );

      expect(repo.calls, <String>[
        'list',
        'unlink:parent-old->workspace',
        'link:parent-new->workspace',
        'link:parent-old->workspace',
      ]);
    },
  );

  test(
    'surfaces both errors when restoring the previous parent also fails',
    () async {
      final repo = _FakeWorkspaceParentRepository()
        ..linkErrorsByParent['parent-new'] = StateError('new parent missing')
        ..linkErrorsByParent['parent-old'] = StateError('restore failed');
      final service = WorkbenchWorkspaceParentUpdateService(repo);

      await expectLater(
        service.update(
          workspace: _workspace(parentWorkspaceId: 'parent-old'),
          parentWorkspaceId: 'parent-new',
        ),
        throwsA(
          isA<WorkspaceException>()
              .having(
                (error) => error.toString(),
                'new error',
                contains('new parent missing'),
              )
              .having(
                (error) => error.toString(),
                'restore error',
                contains('restore failed'),
              ),
        ),
      );
    },
  );
}

Workspace _workspace({String? parentWorkspaceId}) => Workspace(
  id: 'workspace',
  projectId: 'project',
  name: 'Workspace',
  path: 'C:/workspace',
  createdAt: DateTime.utc(2026, 9, 10),
  updatedAt: DateTime.utc(2026, 9, 10),
  kind: WorkspaceKind.linked,
  status: WorkspaceStatus.active,
  parentWorkspaceId: parentWorkspaceId,
);

WorkspaceRelation _relation(String parentId, String childId) =>
    WorkspaceRelation(
      id: '$parentId-$childId',
      parentWorkspaceId: parentId,
      parentInstanceId: 'instance',
      childWorkspaceId: childId,
      childInstanceId: 'instance',
      createdAt: DateTime.utc(2026, 9, 10),
    );

final class _FakeWorkspaceParentRepository
    implements WorkspaceParentRepository {
  List<WorkspaceRelation> relations = <WorkspaceRelation>[];
  final List<String> calls = <String>[];
  final Map<String, Object> linkErrorsByParent = <String, Object>{};

  @override
  Future<List<WorkspaceRelation>> listRelations() async {
    calls.add('list');
    return relations;
  }

  @override
  Future<WorkspaceRelation> linkWorkspaces({
    required String parentWorkspaceId,
    required String childWorkspaceId,
  }) async {
    calls.add('link:$parentWorkspaceId->$childWorkspaceId');
    if (linkErrorsByParent.remove(parentWorkspaceId) case final Object error) {
      throw error;
    }
    return _relation(parentWorkspaceId, childWorkspaceId);
  }

  @override
  Future<void> unlinkWorkspaces({
    required String parentWorkspaceId,
    required String childWorkspaceId,
  }) async {
    calls.add('unlink:$parentWorkspaceId->$childWorkspaceId');
  }
}
