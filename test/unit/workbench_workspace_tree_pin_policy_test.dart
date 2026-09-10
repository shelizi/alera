import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_tree_pin_policy.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('returns only root and descendants whose pin state must change', () {
    final now = DateTime.utc(2026, 9, 10);
    final root = _workspace('root', now, isPinned: false);
    final child = _workspace(
      'child',
      now,
      parentWorkspaceId: root.id,
      isPinned: false,
    );
    final grandchild = _workspace(
      'grandchild',
      now,
      parentWorkspaceId: child.id,
      isPinned: true,
    );
    final unrelated = _workspace('unrelated', now, isPinned: false);
    final state = WorkbenchState(
      workspacesByProject: <String, List<Workspace>>{
        'project': <Workspace>[root, child, grandchild, unrelated],
      },
    );

    final targets = workbenchWorkspaceTreePinTargets(
      state: state,
      workspaceId: root.id,
      isPinned: true,
    );

    expect(targets, <String>[root.id, child.id]);
  });

  test('unpinning includes already traversed descendants that are pinned', () {
    final now = DateTime.utc(2026, 9, 10);
    final root = _workspace('root', now, isPinned: true);
    final child = _workspace(
      'child',
      now,
      parentWorkspaceId: root.id,
      isPinned: false,
    );
    final grandchild = _workspace(
      'grandchild',
      now,
      parentWorkspaceId: child.id,
      isPinned: true,
    );
    final state = WorkbenchState(
      workspacesByProject: <String, List<Workspace>>{
        'project': <Workspace>[root, child, grandchild],
      },
    );

    final targets = workbenchWorkspaceTreePinTargets(
      state: state,
      workspaceId: root.id,
      isPinned: false,
    );

    expect(targets, <String>[root.id, grandchild.id]);
  });

  test(
    'preserves legacy descendant handling when the root snapshot is missing',
    () {
      final now = DateTime.utc(2026, 9, 10);
      final child = _workspace(
        'child',
        now,
        parentWorkspaceId: 'missing-root',
        isPinned: false,
      );
      final state = WorkbenchState(
        workspacesByProject: <String, List<Workspace>>{
          'project': <Workspace>[child],
        },
      );

      final targets = workbenchWorkspaceTreePinTargets(
        state: state,
        workspaceId: 'missing-root',
        isPinned: true,
      );

      expect(targets, <String>[child.id]);
    },
  );
}

Workspace _workspace(
  String id,
  DateTime now, {
  String? parentWorkspaceId,
  required bool isPinned,
}) => Workspace(
  id: id,
  projectId: 'project',
  name: id,
  path: 'C:/$id',
  createdAt: now,
  updatedAt: now,
  kind: WorkspaceKind.linked,
  status: WorkspaceStatus.active,
  parentWorkspaceId: parentWorkspaceId,
  isPinned: isPinned,
);
