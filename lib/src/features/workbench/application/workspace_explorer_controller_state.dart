import 'package:alera/src/features/workbench/application/workspace_explorer_projection_index.dart';
import 'package:flutter_directory_tree/flutter_directory_tree.dart' as tree;

/// Preserve the previous expansion and selection semantics, but flatten
/// once for the complete expansion set rather than once for every folder.
/// The noncanonical index retains binding iteration order for aliases.
void restoreWorkspaceExplorerControllerState({
  required tree.DirectoryTreeController controller,
  required WorkspaceExplorerProjectionIndex index,
  required Set<String> expandedPaths,
  required String? selectedPath,
}) {
  controller.expansions.expandAll(index.boundNodeIdsForPaths(expandedPaths));
  if (selectedPath != null) {
    final selectedId = index.boundNodeIdsForPaths({selectedPath}).lastOrNull;
    if (selectedId != null) controller.selection.selectOnly(selectedId);
  }
}

/// Keep the first visible row as the selection anchor and preserve the
/// original toggle semantics, while avoiding one notification per row.
void selectAllWorkspaceExplorerNodes(
  tree.DirectoryTreeController controller,
  List<tree.VisibleNode> nodes,
) {
  if (nodes.isEmpty) return;
  controller.selection.performBatch(() {
    controller.selection.selectOnly(nodes.first.id);
    for (final node in nodes.skip(1)) {
      controller.selection.toggle(node.id);
    }
  });
}
