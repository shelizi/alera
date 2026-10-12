import 'package:alera_mobile/src/features/runtime/domain/workspace_summary.dart';
import 'package:alera_mobile/src/features/workbench/presentation/workspace_branch_picker.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('searches local branches case-insensitively without duplicates', () {
    expect(
      findWorkspaceBranches([
        'feature/alpha',
        'main',
        'feature/alpha',
        'Fix/Beta',
      ], 'FE'),
      ['feature/alpha'],
    );
    expect(findWorkspaceBranches(['main', 'Fix/Beta'], ''), [
      'Fix/Beta',
      'main',
    ]);
  });

  test('only reuses an active workspace from the same project and branch', () {
    WorkspaceSummary workspace(
      String id,
      String projectId, {
      bool archived = false,
    }) => WorkspaceSummary(
      id: id,
      projectId: projectId,
      name: id,
      path: 'example/$id',
      branch: 'feature/alpha',
      archivedAt: archived ? DateTime.utc(2026) : null,
    );
    final foreign = workspace('foreign', 'p2');
    final archived = workspace('archived', 'p1', archived: true);
    final active = workspace('active', 'p1');
    expect(
      findExistingBranchWorkspace(
        [foreign, archived, active],
        'p1',
        'feature/alpha',
      ),
      same(active),
    );
    expect(
      findExistingBranchWorkspace([foreign], 'p1', 'feature/alpha'),
      isNull,
    );
    expect(findExistingBranchWorkspace([active], 'p1', 'feature/beta'), isNull);
  });
}
