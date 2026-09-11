import 'package:alera/src/features/workbench/application/workbench_persisted_tab_restore_plan.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

typedef WorkbenchPersistedTabFinder = Future<WorkspaceTabRecord?> Function(
  String tabId,
);
typedef WorkbenchDisposedReader = bool Function();
typedef WorkbenchPersistedTabTabsReader = List<WorkspaceTabRecord> Function();
typedef WorkbenchPersistedTabLayoutResolver = WorkbenchLayout Function(
  List<WorkspaceTabRecord> tabs,
);
typedef WorkbenchPersistedTabTabsApplier = void Function(
  List<WorkspaceTabRecord> tabs,
);
typedef WorkbenchPersistedTabLayoutApplier = Future<void> Function(
  WorkbenchLayout layout,
);
typedef WorkbenchPersistedTabSelector = Future<void> Function({
  required String workspaceId,
  required String tabId,
});

final class WorkbenchPersistedTabOpenCoordinator {
  const WorkbenchPersistedTabOpenCoordinator({
    required WorkbenchPersistedTabFinder findTab,
    required WorkbenchDisposedReader isDisposed,
    required WorkbenchPersistedTabTabsReader readCurrentTabs,
    required WorkbenchPersistedTabLayoutResolver layoutForMutation,
    required WorkbenchPersistedTabTabsApplier applyTabs,
    required WorkbenchPersistedTabLayoutApplier applyLayout,
    required WorkbenchPersistedTabSelector selectTab,
  }) : _findTab = findTab,
       _isDisposed = isDisposed,
       _readCurrentTabs = readCurrentTabs,
       _layoutForMutation = layoutForMutation,
       _applyTabs = applyTabs,
       _applyLayout = applyLayout,
       _selectTab = selectTab;

  final WorkbenchPersistedTabFinder _findTab;
  final WorkbenchDisposedReader _isDisposed;
  final WorkbenchPersistedTabTabsReader _readCurrentTabs;
  final WorkbenchPersistedTabLayoutResolver _layoutForMutation;
  final WorkbenchPersistedTabTabsApplier _applyTabs;
  final WorkbenchPersistedTabLayoutApplier _applyLayout;
  final WorkbenchPersistedTabSelector _selectTab;

  Future<void> open({
    required String workspaceId,
    required String tabId,
  }) async {
    final tab = await _findTab(tabId);
    if (_isDisposed()) return;
    if (tab == null || tab.workspaceId != workspaceId) {
      throw StateError(
        'The created tab is no longer available in this workspace.',
      );
    }

    final currentTabs = _readCurrentTabs();
    final plan = planWorkbenchPersistedTabRestore(
      currentTabs: currentTabs,
      layout: _layoutForMutation(currentTabs),
      restoredTab: tab,
    );
    _applyTabs(plan.tabs);
    final layoutToPersist = plan.layoutToPersist;
    if (layoutToPersist != null) {
      await _applyLayout(layoutToPersist);
    }
    if (!_isDisposed()) {
      await _selectTab(workspaceId: workspaceId, tabId: tabId);
    }
  }
}
