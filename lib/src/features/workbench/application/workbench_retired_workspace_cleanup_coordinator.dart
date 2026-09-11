import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

typedef WorkbenchHostedReviewTabsBackgroundReleaser = void Function(
  Workspace workspace,
  Iterable<WorkspaceTabRecord> tabs,
);
typedef WorkbenchRetiredWorkspaceFocusForgetter = void Function(
  String workspaceId,
);
typedef WorkbenchRetiredWorkspaceLocalReleaser = void Function(
  String workspaceId,
  Iterable<WorkspaceTabRecord> tabs,
);

final class WorkbenchRetiredWorkspaceCleanupCoordinator {
  const WorkbenchRetiredWorkspaceCleanupCoordinator({
    required WorkbenchHostedReviewTabsBackgroundReleaser
    releaseHostedReviewTabsInBackground,
    required WorkbenchRetiredWorkspaceFocusForgetter forgetFocusHistory,
    required WorkbenchRetiredWorkspaceLocalReleaser releaseLocalWorkspace,
  }) : _releaseHostedReviewTabsInBackground =
           releaseHostedReviewTabsInBackground,
       _forgetFocusHistory = forgetFocusHistory,
       _releaseLocalWorkspace = releaseLocalWorkspace;

  final WorkbenchHostedReviewTabsBackgroundReleaser
  _releaseHostedReviewTabsInBackground;
  final WorkbenchRetiredWorkspaceFocusForgetter _forgetFocusHistory;
  final WorkbenchRetiredWorkspaceLocalReleaser _releaseLocalWorkspace;

  void cleanup({
    required String workspaceId,
    required Workspace? workspace,
    required Iterable<WorkspaceTabRecord> tabs,
  }) {
    if (workspace != null) {
      _releaseHostedReviewTabsInBackground(workspace, tabs);
    }
    _forgetFocusHistory(workspaceId);
    _releaseLocalWorkspace(workspaceId, tabs);
  }
}
