import 'package:alera/src/features/workbench/application/workbench_workspace_tag_creation_service.dart';
import 'package:alera/src/features/workbench/application/workspace_graph_repository.dart';
import 'package:alera/src/features/workbench/application/workspace_service.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('rejects a blank tag name before touching the repository', () async {
    final repository = _FakeTagRepository();
    final service = WorkbenchWorkspaceTagCreationService(repository);

    await expectLater(
      service.create('   '),
      throwsA(
        isA<WorkspaceException>().having(
          (error) => error.toString(),
          'message',
          contains('Tag name is required'),
        ),
      ),
    );

    expect(repository.listCalls, 0);
    expect(repository.upserts, isEmpty);
  });

  test('reuses an existing tag case-insensitively', () async {
    final existing = WorkspaceTag.create(name: 'Review');
    final repository = _FakeTagRepository(tags: <WorkspaceTag>[existing]);
    final service = WorkbenchWorkspaceTagCreationService(repository);

    final result = await service.create('  review  ');

    expect(result.id, existing.id);
    expect(repository.listCalls, 1);
    expect(repository.upserts, isEmpty);
  });

  test('creates a trimmed tag when no duplicate exists', () async {
    final repository = _FakeTagRepository();
    final service = WorkbenchWorkspaceTagCreationService(repository);

    final result = await service.create('  Needs Review  ');

    expect(result.name, 'Needs Review');
    expect(repository.listCalls, 1);
    expect(repository.upserts, hasLength(1));
    expect(repository.upserts.single.name, 'Needs Review');
  });
}

final class _FakeTagRepository implements WorkspaceTagRepository {
  _FakeTagRepository({List<WorkspaceTag>? tags})
    : tags = <WorkspaceTag>[...?tags];

  final List<WorkspaceTag> tags;
  final List<WorkspaceTag> upserts = <WorkspaceTag>[];
  int listCalls = 0;

  @override
  Future<List<WorkspaceTag>> listTags() async {
    listCalls += 1;
    return List<WorkspaceTag>.of(tags);
  }

  @override
  Future<WorkspaceTag> upsertTag(WorkspaceTag tag) async {
    upserts.add(tag);
    tags.removeWhere((candidate) => candidate.id == tag.id);
    tags.add(tag);
    return tag;
  }
}
