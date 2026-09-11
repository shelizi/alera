import 'package:alera/src/features/workbench/application/workbench_hosted_review_retention_service.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_focus_history.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

typedef WorkbenchRemovedProjectWorkspaceCloser = void Function(
  String workspaceId,
);
typedef WorkbenchProjectRemovalAction = Future<void> Function();

final class WorkbenchRemovedProjectWorkspace {
  const WorkbenchRemovedProjectWorkspace({
    required this.workspace,
    required this.tabs,
  });

  final Workspace workspace;
  final List<WorkspaceTabRecord> tabs;
}

final class WorkbenchRemoveProjectCleanupCoordinator {
  const WorkbenchRemoveProjectCleanupCoordinator({
    required WorkbenchRemovedProjectWorkspaceCloser closeLocalWorkspace,
    required WorkbenchHostedReviewTabRetention hostedReviewRetention,
    required WorkspaceTabFocusHistory tabFocusHistory,
  }) : _closeLocalWorkspace = closeLocalWorkspace,
       _hostedReviewRetention = hostedReviewRetention,
       _tabFocusHistory = tabFocusHistory;

  final WorkbenchRemovedProjectWorkspaceCloser _closeLocalWorkspace;
  final WorkbenchHostedReviewTabRetention _hostedReviewRetention;
  final WorkspaceTabFocusHistory _tabFocusHistory;

  Future<void> remove({
    required WorkbenchProjectRemovalAction removeProject,
    required Iterable<WorkbenchRemovedProjectWorkspace> removedWorkspaces,
  }) async {
    final workspaces = removedWorkspaces.toList(growable: false);
    for (final removed in workspaces) {
      _closeLocalWorkspace(removed.workspace.id);
    }
    await removeProject();
    await cleanup(workspaces);
  }

  Future<void> cleanup(
    Iterable<WorkbenchRemovedProjectWorkspace> removedWorkspaces,
  ) async {
    for (final removed in removedWorkspaces) {
      _tabFocusHistory.forget(removed.workspace.id);
      for (final tab in removed.tabs) {
        await _hostedReviewRetention.releaseTab(removed.workspace, tab);
      }
    }
  }
}
