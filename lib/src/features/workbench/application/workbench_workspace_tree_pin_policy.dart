import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/application/workspace_descendants.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';

List<String> workbenchWorkspaceTreePinTargets({
  required WorkbenchState state,
  required String workspaceId,
  required bool isPinned,
}) {
  final workspaces = <Workspace>[
    for (final group in state.workspacesByProject.values) ...group,
  ];
  final targetIds = <String>[
    workspaceId,
    ...workspaceIdsDescendedFrom(workspaces, workspaceId),
  ];
  final result = <String>[];
  for (final id in targetIds) {
    Workspace? current;
    for (final workspace in workspaces) {
      if (workspace.id == id) {
        current = workspace;
        break;
      }
    }
    if (current != null && current.isPinned != isPinned) {
      result.add(id);
    }
  }
  return result;
}
