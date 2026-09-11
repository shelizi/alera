import 'package:alera/src/features/workbench/application/workbench_closed_tabs_plan.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

typedef WorkbenchClosedTabsReader = List<WorkspaceTabRecord> Function();
typedef WorkbenchClosedTabsLayoutReader = WorkbenchLayout? Function();
typedef WorkbenchMostRecentOpenTabResolver = String? Function(
  Set<String> openTabIds,
);
typedef WorkbenchClosedTabsApplier = void Function(
  List<WorkspaceTabRecord> tabs,
);
typedef WorkbenchClosedTabsFocusHistoryForgetter = void Function();
typedef WorkbenchClosedTabsLayoutApplier = Future<void> Function(
  WorkbenchLayout layout,
);
typedef WorkbenchClosedTabsRemovalCompleter = void Function(
  List<WorkspaceTabRecord> remainingTabs,
);

final class WorkbenchClosedTabsCompletionCoordinator {
  const WorkbenchClosedTabsCompletionCoordinator({
    required WorkbenchClosedTabsReader readCurrentTabs,
    required WorkbenchClosedTabsLayoutReader readCurrentLayout,
    required WorkbenchMostRecentOpenTabResolver mostRecentOpenTabId,
    required WorkbenchClosedTabsApplier applyTabs,
    required WorkbenchClosedTabsFocusHistoryForgetter forgetFocusHistory,
    required WorkbenchClosedTabsLayoutApplier applyLayout,
    required WorkbenchClosedTabsRemovalCompleter completeRemoval,
  }) : _readCurrentTabs = readCurrentTabs,
       _readCurrentLayout = readCurrentLayout,
       _mostRecentOpenTabId = mostRecentOpenTabId,
       _applyTabs = applyTabs,
       _forgetFocusHistory = forgetFocusHistory,
       _applyLayout = applyLayout,
       _completeRemoval = completeRemoval;

  final WorkbenchClosedTabsReader _readCurrentTabs;
  final WorkbenchClosedTabsLayoutReader _readCurrentLayout;
  final WorkbenchMostRecentOpenTabResolver _mostRecentOpenTabId;
  final WorkbenchClosedTabsApplier _applyTabs;
  final WorkbenchClosedTabsFocusHistoryForgetter _forgetFocusHistory;
  final WorkbenchClosedTabsLayoutApplier _applyLayout;
  final WorkbenchClosedTabsRemovalCompleter _completeRemoval;

  Future<void> run({
    required String workspaceId,
    required WorkbenchTabCloseSnapshot snapshot,
  }) async {
    final closedTabIds = snapshot.closedTabIds;
    final remainingTabs = _readCurrentTabs()
        .where((tab) => !closedTabIds.contains(tab.id))
        .toList(growable: false);
    final mostRecentOpenTabId =
        snapshot.closedActiveTab && remainingTabs.isNotEmpty
        ? _mostRecentOpenTabId(<String>{
            for (final tab in remainingTabs) tab.id,
          })
        : null;
    final plan = planWorkbenchClosedTabs(
      workspaceId: workspaceId,
      remainingTabs: remainingTabs,
      currentLayout: _readCurrentLayout(),
      closedTabIds: closedTabIds,
      closedActiveTab: snapshot.closedActiveTab,
      mostRecentOpenTabId: mostRecentOpenTabId,
    );
    _applyTabs(remainingTabs);
    if (plan.shouldForgetFocusHistory) {
      _forgetFocusHistory();
    }
    await _applyLayout(plan.layout);
    _completeRemoval(remainingTabs);
  }
}
