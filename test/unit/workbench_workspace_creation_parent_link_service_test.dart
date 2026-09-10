import 'package:alera/src/features/workbench/application/workbench_workspace_creation_parent_link_service.dart';
import 'package:alera/src/features/workbench/application/workspace_graph_repository.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_creation_result.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'blank parent returns the original creation result without linking',
    () async {
      final repo = _FakeWorkspaceParentRepository();
      final service = WorkbenchWorkspaceCreationParentLinkService(repo);
      final result = _result();

      final next = await service.attach(
        result: result,
        parentWorkspaceId: '   ',
      );

      expect(next, same(result));
      expect(repo.linkedParents, isEmpty);
    },
  );

  test('links a normalized parent and preserves the original result', () async {
    final repo = _FakeWorkspaceParentRepository();
    final service = WorkbenchWorkspaceCreationParentLinkService(repo);
    final result = _result();

    final next = await service.attach(
      result: result,
      parentWorkspaceId: ' parent ',
    );

    expect(next, same(result));
    expect(repo.linkedParents, <String>['parent']);
  });

  test(
    'link failure becomes a warning result without losing creation data',
    () async {
      final repo = _FakeWorkspaceParentRepository()
        ..linkError = StateError('parent missing');
      final service = WorkbenchWorkspaceCreationParentLinkService(repo);
      final result = _result();

      final next = await service.attach(
        result: result,
        parentWorkspaceId: 'parent',
      );

      expect(next, isNot(same(result)));
      expect(next.workspace, same(result.workspace));
      expect(next.setupReport, same(result.setupReport));
      expect(next.deferredSetupCommand, result.deferredSetupCommand);
      expect(next.parentLinkError, contains('parent missing'));
    },
  );
}

WorkspaceCreationResult _result() => WorkspaceCreationResult(
  workspace: Workspace(
    id: 'workspace',
    projectId: 'project',
    name: 'Workspace',
    path: 'C:/workspace',
    createdAt: DateTime.utc(2026, 9, 10),
    updatedAt: DateTime.utc(2026, 9, 10),
    kind: WorkspaceKind.linked,
    status: WorkspaceStatus.active,
  ),
  setupReport: const WorktreeSetupReport(),
  deferredSetupCommand: 'setup',
);

final class _FakeWorkspaceParentRepository
    implements WorkspaceParentRepository {
  Object? linkError;
  final List<String> linkedParents = <String>[];

  @override
  Future<List<WorkspaceRelation>> listRelations() async => const [];

  @override
  Future<WorkspaceRelation> linkWorkspaces({
    required String parentWorkspaceId,
    required String childWorkspaceId,
  }) async {
    if (linkError case final Object error) throw error;
    linkedParents.add(parentWorkspaceId);
    return WorkspaceRelation(
      id: 'relation',
      parentWorkspaceId: parentWorkspaceId,
      parentInstanceId: 'parent-instance',
      childWorkspaceId: childWorkspaceId,
      childInstanceId: 'child-instance',
      createdAt: DateTime.utc(2026, 9, 10),
    );
  }

  @override
  Future<void> unlinkWorkspaces({
    required String parentWorkspaceId,
    required String childWorkspaceId,
  }) async {}
}
