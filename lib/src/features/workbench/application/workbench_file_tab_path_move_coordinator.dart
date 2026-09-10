import 'package:alera/src/features/workbench/application/workbench_file_tab_path_move_plan.dart';
import 'package:alera/src/features/workbench/application/workspace_tab_service.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

typedef WorkbenchFileTabPathMover =
    Future<WorkspaceFileTabPathMoveResult> Function({
      required String workspaceId,
      required String oldRelativePath,
      required String newRelativePath,
    });
typedef WorkbenchCurrentTabsReader = List<WorkspaceTabRecord> Function();
typedef WorkbenchCurrentLayoutReader = WorkbenchLayout? Function();
typedef WorkbenchFileTabsApplier = void Function(List<WorkspaceTabRecord> tabs);
typedef WorkbenchFileTabLayoutApplier = Future<void> Function(
  WorkbenchLayout layout,
);
typedef WorkbenchFileTabRemovalCompleter = void Function(
  List<WorkspaceTabRecord> remainingTabs,
);

final class WorkbenchFileTabPathMoveCoordinator {
  const WorkbenchFileTabPathMoveCoordinator({
    required WorkbenchFileTabPathMover movePaths,
    required WorkbenchCurrentTabsReader readCurrentTabs,
    required WorkbenchCurrentLayoutReader readCurrentLayout,
    required WorkbenchFileTabsApplier applyTabs,
    required WorkbenchFileTabLayoutApplier applyLayout,
    required WorkbenchFileTabRemovalCompleter completeRemoval,
  }) : _movePaths = movePaths,
       _readCurrentTabs = readCurrentTabs,
       _readCurrentLayout = readCurrentLayout,
       _applyTabs = applyTabs,
       _applyLayout = applyLayout,
       _completeRemoval = completeRemoval;

  final WorkbenchFileTabPathMover _movePaths;
  final WorkbenchCurrentTabsReader _readCurrentTabs;
  final WorkbenchCurrentLayoutReader _readCurrentLayout;
  final WorkbenchFileTabsApplier _applyTabs;
  final WorkbenchFileTabLayoutApplier _applyLayout;
  final WorkbenchFileTabRemovalCompleter _completeRemoval;

  Future<void> run({
    required String workspaceId,
    required String oldRelativePath,
    required String newRelativePath,
  }) async {
    final result = await _movePaths(
      workspaceId: workspaceId,
      oldRelativePath: oldRelativePath,
      newRelativePath: newRelativePath,
    );
    if (result.isEmpty) {
      return;
    }
    final plan = planWorkbenchFileTabPathMove(
      workspaceId: workspaceId,
      currentTabs: _readCurrentTabs(),
      currentLayout: _readCurrentLayout(),
      updatedTabs: result.updatedTabs,
      closedTabIds: result.closedTabIds,
    );
    _applyTabs(plan.tabs);
    final layout = plan.layoutToPersist;
    if (layout != null) {
      await _applyLayout(layout);
    }
    _completeRemoval(plan.tabs);
  }
}
