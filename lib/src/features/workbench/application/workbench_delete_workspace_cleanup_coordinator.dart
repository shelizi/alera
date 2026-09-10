import 'package:alera/src/features/workbench/application/workbench_explicit_resource_cleaner.dart';
import 'package:alera/src/features/workbench/application/workbench_hosted_review_retention_service.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_focus_history.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

final class WorkbenchDeleteWorkspaceCleanupCoordinator {
  const WorkbenchDeleteWorkspaceCleanupCoordinator({
    required WorkbenchExplicitWorkspaceResourceCleaner resourceCleaner,
    required WorkbenchHostedReviewTabRetention hostedReviewRetention,
    required WorkspaceTabFocusHistory tabFocusHistory,
  }) : _resourceCleaner = resourceCleaner,
       _hostedReviewRetention = hostedReviewRetention,
       _tabFocusHistory = tabFocusHistory;

  final WorkbenchExplicitWorkspaceResourceCleaner _resourceCleaner;
  final WorkbenchHostedReviewTabRetention _hostedReviewRetention;
  final WorkspaceTabFocusHistory _tabFocusHistory;

  Future<void> cleanup({
    required Workspace workspace,
    required List<WorkspaceTabRecord> tabs,
    required String fallbackWorkspacePath,
  }) async {
    _resourceCleaner.closeWorkspaceLocalResources(workspace.id, tabs);
    for (final tab in tabs) {
      await _hostedReviewRetention.releaseTab(
        workspace,
        tab,
        fallbackWorkspacePath: fallbackWorkspacePath,
      );
    }
    _tabFocusHistory.forget(workspace.id);
    _resourceCleaner.clearDeletedWorkspaceObservers(workspace.id, tabs);
  }
}
