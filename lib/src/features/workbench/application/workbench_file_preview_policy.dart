import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

String? workbenchPreviewTabIdInGroup({
  required WorkbenchLayout layout,
  required List<WorkspaceTabRecord> tabs,
  required String groupId,
}) {
  final group = layout.groups[groupId];
  if (group == null) {
    return null;
  }
  final tabsById = <String, WorkspaceTabRecord>{
    for (final tab in tabs) tab.id: tab,
  };
  final active = tabsById[group.activeTabId];
  if (active != null && active.isFilePreviewSlot) {
    return active.id;
  }
  for (final tabId in group.tabIds) {
    final tab = tabsById[tabId];
    if (tab != null && tab.isFilePreviewSlot) {
      return tab.id;
    }
  }
  return null;
}
