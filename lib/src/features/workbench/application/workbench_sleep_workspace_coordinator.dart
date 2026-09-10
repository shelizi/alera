import 'package:alera/src/features/workbench/application/workbench_cleared_layout_registry.dart';
import 'package:alera/src/features/workbench/application/workbench_hosted_review_retention_service.dart';
import 'package:alera/src/features/workbench/application/workbench_repository.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_focus_history.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

final class WorkbenchSleepWorkspaceCoordinator {
  const WorkbenchSleepWorkspaceCoordinator({
    required WorkbenchWorkspaceTabRemovalRepository tabRemoval,
    required WorkbenchHostedReviewTabRetention hostedReviewRetention,
    required WorkbenchClearedLayoutRegistry clearedLayouts,
    required WorkspaceTabFocusHistory tabFocusHistory,
  }) : _tabRemoval = tabRemoval,
       _hostedReviewRetention = hostedReviewRetention,
       _clearedLayouts = clearedLayouts,
       _tabFocusHistory = tabFocusHistory;

  final WorkbenchWorkspaceTabRemovalRepository _tabRemoval;
  final WorkbenchHostedReviewTabRetention _hostedReviewRetention;
  final WorkbenchClearedLayoutRegistry _clearedLayouts;
  final WorkspaceTabFocusHistory _tabFocusHistory;

  Future<void> sleep({
    required Workspace workspace,
    required List<WorkspaceTabRecord> tabs,
  }) async {
    _clearedLayouts.mark(workspace.id);
    _tabFocusHistory.forget(workspace.id);
    try {
      await _tabRemoval.removeWorkspaceTabsForWorkspace(workspace.id);
      for (final tab in tabs) {
        await _hostedReviewRetention.releaseTab(workspace, tab);
      }
    } catch (_) {
      _clearedLayouts.forget(workspace.id);
      rethrow;
    }
  }
}
