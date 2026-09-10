import 'package:alera/src/features/workbench/application/workbench_closed_tabs_plan.dart';
import 'package:alera/src/features/workbench/application/workbench_explicit_resource_cleaner.dart';
import 'package:alera/src/features/workbench/application/workbench_hosted_review_retention_service.dart';
import 'package:alera/src/features/workbench/application/workbench_tab_close_store.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';

final class WorkbenchTabCloseCoordinator {
  const WorkbenchTabCloseCoordinator({
    required WorkbenchTabCloseStore tabStore,
    required WorkbenchHostedReviewTabRetention hostedReviewRetention,
    required WorkbenchExplicitTabResourceCleaner resourceCleaner,
  }) : _tabStore = tabStore,
       _hostedReviewRetention = hostedReviewRetention,
       _resourceCleaner = resourceCleaner;

  final WorkbenchTabCloseStore _tabStore;
  final WorkbenchHostedReviewTabRetention _hostedReviewRetention;
  final WorkbenchExplicitTabResourceCleaner _resourceCleaner;

  Future<void> close({
    required Workspace workspace,
    required WorkbenchTabCloseSnapshot snapshot,
  }) async {
    for (final tabId in snapshot.closedTabIds) {
      await _tabStore.closeTab(tabId);
      final closedTab = snapshot.closingTabs[tabId];
      if (closedTab != null) {
        await _hostedReviewRetention.releaseTab(workspace, closedTab);
      }
      _resourceCleaner.closeTabLocalResources(tabId);
    }
  }
}
