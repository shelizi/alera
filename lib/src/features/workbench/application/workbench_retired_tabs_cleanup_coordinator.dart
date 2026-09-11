import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

typedef WorkbenchRetiredTabsHostedReviewReleaser = void Function(
  Workspace workspace,
  Iterable<WorkspaceTabRecord> tabs,
);
typedef WorkbenchRetiredTabsLocalReleaser = void Function(
  Iterable<WorkspaceTabRecord> tabs,
);

final class WorkbenchRetiredTabsCleanupCoordinator {
  const WorkbenchRetiredTabsCleanupCoordinator({
    required WorkbenchRetiredTabsHostedReviewReleaser
    releaseHostedReviewTabsInBackground,
    required WorkbenchRetiredTabsLocalReleaser releaseLocalTabs,
  }) : _releaseHostedReviewTabsInBackground =
           releaseHostedReviewTabsInBackground,
       _releaseLocalTabs = releaseLocalTabs;

  final WorkbenchRetiredTabsHostedReviewReleaser
  _releaseHostedReviewTabsInBackground;
  final WorkbenchRetiredTabsLocalReleaser _releaseLocalTabs;

  void cleanup({
    required Workspace? workspace,
    required Iterable<WorkspaceTabRecord> tabs,
  }) {
    if (workspace != null) {
      _releaseHostedReviewTabsInBackground(workspace, tabs);
    }
    _releaseLocalTabs(tabs);
  }
}
