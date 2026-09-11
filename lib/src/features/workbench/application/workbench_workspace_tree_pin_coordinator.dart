import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_tree_pin_policy.dart';

typedef WorkbenchWorkspacePinAction = Future<void> Function({
  required String workspaceId,
  required bool isPinned,
});

final class WorkbenchWorkspaceTreePinCoordinator {
  const WorkbenchWorkspaceTreePinCoordinator({
    required WorkbenchWorkspacePinAction setPinned,
  }) : _setPinned = setPinned;

  final WorkbenchWorkspacePinAction _setPinned;

  Future<void> run({
    required WorkbenchState state,
    required String workspaceId,
    required bool isPinned,
  }) async {
    final targetIds = workbenchWorkspaceTreePinTargets(
      state: state,
      workspaceId: workspaceId,
      isPinned: isPinned,
    );
    for (final targetId in targetIds) {
      await _setPinned(workspaceId: targetId, isPinned: isPinned);
    }
  }
}
