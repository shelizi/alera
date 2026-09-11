import 'package:alera/src/features/workbench/application/workbench_workspace_selection_hydrator.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

typedef WorkbenchWorkspaceSelectionActivator = void Function();
typedef WorkbenchWorkspaceSelectionHydrate =
    Future<WorkbenchWorkspaceSelectionHydration> Function({
      required String workspaceId,
      required bool ensureInitialTerminal,
    });
typedef WorkbenchWorkspaceSelectionTabsApplier = void Function(
  List<WorkspaceTabRecord> tabs,
);
typedef WorkbenchWorkspaceSelectionLayoutApplier = Future<void> Function(
  WorkbenchLayout layout,
);
typedef WorkbenchWorkspaceSelectionHistoryRecorder = bool Function();
typedef WorkbenchWorkspaceSelectionHistoryNotifier = void Function();

final class WorkbenchWorkspaceSelectionCoordinator {
  const WorkbenchWorkspaceSelectionCoordinator({
    required WorkbenchWorkspaceSelectionActivator activateSelection,
    required WorkbenchWorkspaceSelectionHydrate hydrate,
    required WorkbenchWorkspaceSelectionTabsApplier applyTabs,
    required WorkbenchWorkspaceSelectionLayoutApplier applyLayout,
    required WorkbenchWorkspaceSelectionHistoryRecorder recordHistory,
    required WorkbenchWorkspaceSelectionHistoryNotifier notifyHistoryChanged,
  }) : _activateSelection = activateSelection,
       _hydrate = hydrate,
       _applyTabs = applyTabs,
       _applyLayout = applyLayout,
       _recordHistory = recordHistory,
       _notifyHistoryChanged = notifyHistoryChanged;

  final WorkbenchWorkspaceSelectionActivator _activateSelection;
  final WorkbenchWorkspaceSelectionHydrate _hydrate;
  final WorkbenchWorkspaceSelectionTabsApplier _applyTabs;
  final WorkbenchWorkspaceSelectionLayoutApplier _applyLayout;
  final WorkbenchWorkspaceSelectionHistoryRecorder _recordHistory;
  final WorkbenchWorkspaceSelectionHistoryNotifier _notifyHistoryChanged;

  Future<void> select({
    required String workspaceId,
    required bool ensureInitialTerminal,
    required bool shouldRecordHistory,
  }) async {
    _activateSelection();
    final hydration = await _hydrate(
      workspaceId: workspaceId,
      ensureInitialTerminal: ensureInitialTerminal,
    );
    _applyTabs(hydration.tabs);
    await _applyLayout(hydration.layout);
    if (shouldRecordHistory && _recordHistory()) {
      _notifyHistoryChanged();
    }
  }
}
