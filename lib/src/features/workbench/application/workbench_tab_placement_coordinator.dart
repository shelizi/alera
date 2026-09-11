import 'package:alera/src/features/workbench/application/workbench_tab_placement_plan.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

typedef WorkbenchTabOpener = Future<WorkspaceTabRecord> Function();
typedef WorkbenchTabPlacementPlanner = WorkbenchTabPlacementPlan Function(
  WorkspaceTabRecord tab,
);
typedef WorkbenchPlacedTabsApplier = void Function(
  List<WorkspaceTabRecord> tabs,
);
typedef WorkbenchPlacedLayoutApplier = Future<void> Function(
  WorkbenchLayout layout,
);
typedef WorkbenchTabAfterPlaced = void Function(WorkspaceTabRecord tab);

final class WorkbenchTabPlacementCoordinator {
  const WorkbenchTabPlacementCoordinator({
    required WorkbenchTabOpener openTab,
    required WorkbenchTabPlacementPlanner planPlacement,
    required WorkbenchPlacedTabsApplier applyTabs,
    required WorkbenchPlacedLayoutApplier applyLayout,
    WorkbenchTabAfterPlaced? afterPlaced,
  }) : _openTab = openTab,
       _planPlacement = planPlacement,
       _applyTabs = applyTabs,
       _applyLayout = applyLayout,
       _afterPlaced = afterPlaced;

  final WorkbenchTabOpener _openTab;
  final WorkbenchTabPlacementPlanner _planPlacement;
  final WorkbenchPlacedTabsApplier _applyTabs;
  final WorkbenchPlacedLayoutApplier _applyLayout;
  final WorkbenchTabAfterPlaced? _afterPlaced;

  Future<WorkspaceTabRecord> run() async {
    final tab = await _openTab();
    final placement = _planPlacement(tab);
    _applyTabs(placement.tabs);
    await _applyLayout(placement.layout);
    _afterPlaced?.call(tab);
    return tab;
  }
}
