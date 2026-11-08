import 'package:alera/src/features/workbench/application/workbench_branch_worktree_lookup.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final now = DateTime.utc(2026, 11, 8);

  Workspace workspace(
    String id,
    String branch, {
    String projectId = 'project-a',
    WorkspaceStatus status = WorkspaceStatus.active,
    bool archived = false,
  }) => Workspace(
    id: id,
    projectId: projectId,
    name: id,
    branch: branch,
    path: '/tmp/$id',
    createdAt: now,
    updatedAt: now,
    kind: WorkspaceKind.linked,
    status: status,
    archivedAt: archived ? now : null,
  );

  test('locates existing worktree from the same project, not another repo', () {
    final current = workspace('current', 'main');
    final target = workspace('feature-tree', 'feature/a');
    final foreign = workspace('foreign-tree', 'feature/a', projectId: 'other');
    // Put the unrelated project first: matching by branch alone is wrong.
    final workspaces = [foreign, current, target];

    expect(
      findExistingBranchWorktree(
        workspaces: workspaces,
        currentWorkspace: current,
        branch: 'feature/a',
      )?.id,
      target.id,
    );
    expect(
      findExistingBranchWorktree(
        workspaces: workspaces,
        currentWorkspace: current,
        branch: 'feature/b',
      ),
      isNull,
    );
    expect(
      findExistingBranchWorktree(
        workspaces: [foreign, current],
        currentWorkspace: current,
        branch: 'feature/a',
      ),
      isNull,
    );
    expect(
      findExistingBranchWorktree(
        workspaces: workspaces,
        currentWorkspace: current,
        branch: ' feature/a ',
      )?.id,
      target.id,
    );
  });

  test(
    'ignores current, detached and removed but matches archived worktrees',
    () {
      final current = workspace('current', 'main');
      final removed = workspace('removed', 'feature/a', status: .removed);
      final archived = workspace('archived', 'feature/b', archived: true);
      final workspaces = [current, removed, archived];

      for (final branch in ['main', 'HEAD', '', 'feature/a']) {
        expect(
          findExistingBranchWorktree(
            workspaces: workspaces,
            currentWorkspace: current,
            branch: branch,
          ),
          isNull,
          reason: branch,
        );
      }
      expect(
        findExistingBranchWorktree(
          workspaces: workspaces,
          currentWorkspace: current,
          branch: 'feature/b',
        )?.id,
        archived.id,
      );
    },
  );
}
