import 'package:alera/src/features/workbench/application/workbench_hosted_review_retention_service.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_focus_history.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

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
    required WorkbenchHostedReviewTabRetention hostedReviewRetention,
    required WorkspaceTabFocusHistory tabFocusHistory,
  }) : _hostedReviewRetention = hostedReviewRetention,
       _tabFocusHistory = tabFocusHistory;

  final WorkbenchHostedReviewTabRetention _hostedReviewRetention;
  final WorkspaceTabFocusHistory _tabFocusHistory;

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
