import 'package:alera/src/features/workbench/application/workbench_closed_tabs_plan.dart';
import 'package:alera/src/features/workbench/application/workbench_explicit_resource_cleaner.dart';
import 'package:alera/src/features/workbench/application/workbench_hosted_review_retention_service.dart';
import 'package:alera/src/features/workbench/application/workbench_tab_close_store.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

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
    Map<String, String> observedViewLeaseIds = const <String, String>{},
  }) async {
    for (final tabId in snapshot.closedTabIds) {
      final closedTab = snapshot.closingTabs[tabId];
      if (closedTab != null &&
          closedTab.kind == WorkspaceTabKind.terminal &&
          closedTab.payload['lastTerminalTabTransfer'] is Map) {
        // Never fall back to legacy unscoped close, including repositories
        // that would silently ignore the transferred receipt.
        final observedLease = observedViewLeaseIds[tabId];
        if (observedLease == null || observedLease.trim().isEmpty) {
          throw StateError(
            'Cannot close a transferred Terminal without the already attached view lease. Reopen and attach it in its current workspace first.',
          );
        }
        if (_tabStore is! WorkbenchViewBoundObservedTabCloseStore) {
          throw StateError(
            'This tab store cannot safely close a transferred Terminal.',
          );
        }
        await (_tabStore as WorkbenchViewBoundObservedTabCloseStore)
            .closeObservedTabWithViewLease(
              closedTab,
              workspace.id,
              observedLease,
            );
      } else if (closedTab != null &&
          _tabStore is WorkbenchObservedTabCloseStore) {
        await (_tabStore as WorkbenchObservedTabCloseStore)
            .closeObservedTabInWorkspace(closedTab, workspace.id);
      } else if (_tabStore is WorkbenchScopedTabCloseStore) {
        await (_tabStore as WorkbenchScopedTabCloseStore).closeTabInWorkspace(
          tabId,
          workspace.id,
        );
      } else {
        await _tabStore.closeTab(tabId);
      }
      if (closedTab != null) {
        await _hostedReviewRetention.releaseTab(workspace, closedTab);
      }
      _resourceCleaner.closeTabLocalResources(
        tabId,
        terminalSessionId: closedTab?.terminalSessionId,
      );
    }
  }
}
