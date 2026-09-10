import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_creation_result.dart';

typedef WorkbenchWorkspaceCreateAction =
    Future<WorkspaceCreationResult> Function();
typedef WorkbenchCreatedWorkspaceReconciler = void Function(
  Project project,
  Workspace workspace,
);
typedef WorkbenchCreatedWorkspaceSelector = Future<void> Function(
  Project project,
  Workspace workspace,
);
typedef WorkbenchDeferredSetupTabOpener = Future<void> Function(
  WorkspaceCreationResult creation,
);
typedef WorkbenchWorkspaceCreationParentAttacher =
    Future<WorkspaceCreationResult> Function({
      required WorkspaceCreationResult result,
      String? parentWorkspaceId,
    });

final class WorkbenchWorkspaceCreationCoordinator {
  const WorkbenchWorkspaceCreationCoordinator({
    required WorkbenchWorkspaceCreateAction createWorkspace,
    required WorkbenchCreatedWorkspaceReconciler reconcileWorkspace,
    required WorkbenchCreatedWorkspaceSelector selectWorkspace,
    required WorkbenchDeferredSetupTabOpener openDeferredSetupTab,
    required WorkbenchWorkspaceCreationParentAttacher attachParent,
  }) : _createWorkspace = createWorkspace,
       _reconcileWorkspace = reconcileWorkspace,
       _selectWorkspace = selectWorkspace,
       _openDeferredSetupTab = openDeferredSetupTab,
       _attachParent = attachParent;

  final WorkbenchWorkspaceCreateAction _createWorkspace;
  final WorkbenchCreatedWorkspaceReconciler _reconcileWorkspace;
  final WorkbenchCreatedWorkspaceSelector _selectWorkspace;
  final WorkbenchDeferredSetupTabOpener _openDeferredSetupTab;
  final WorkbenchWorkspaceCreationParentAttacher _attachParent;

  Future<WorkspaceCreationResult> run({
    required Project project,
    required bool initializeTabs,
    String? parentWorkspaceId,
  }) async {
    final result = await _createWorkspace();
    _reconcileWorkspace(project, result.workspace);
    if (initializeTabs) {
      await _selectWorkspace(project, result.workspace);
      await _openDeferredSetupTab(result);
    }
    return _attachParent(result: result, parentWorkspaceId: parentWorkspaceId);
  }
}
