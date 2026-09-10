import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

final class WorkbenchOpenedFileTabPlan {
  const WorkbenchOpenedFileTabPlan({
    required this.tabs,
    required this.layout,
    required this.shouldForgetEditorSession,
  });

  final List<WorkspaceTabRecord> tabs;
  final WorkbenchLayout layout;
  final bool shouldForgetEditorSession;
}

WorkbenchOpenedFileTabPlan planWorkbenchOpenedFileTab({
  required List<WorkspaceTabRecord> previousTabs,
  required WorkbenchLayout layout,
  required String targetGroupId,
  required WorkspaceTabRecord tab,
}) {
  WorkspaceTabRecord? previousTab;
  for (final candidate in previousTabs) {
    if (candidate.id == tab.id) {
      previousTab = candidate;
      break;
    }
  }
  final alreadyOpen = previousTab != null;
  final shouldForgetEditorSession =
      previousTab != null &&
      (previousTab.filePath != tab.filePath || previousTab.kind != tab.kind);
  final tabs = alreadyOpen
      ? <WorkspaceTabRecord>[
          for (final candidate in previousTabs)
            if (candidate.id == tab.id) tab else candidate,
        ]
      : <WorkspaceTabRecord>[...previousTabs, tab];
  final nextLayout = alreadyOpen
      ? layout.setActiveTab(
          groupId: layout.groupIdForTab(tab.id) ?? targetGroupId,
          tabId: tab.id,
        )
      : layout.addTabToGroup(groupId: targetGroupId, tabId: tab.id);
  return WorkbenchOpenedFileTabPlan(
    tabs: tabs,
    layout: nextLayout.sanitize(tabs),
    shouldForgetEditorSession: shouldForgetEditorSession,
  );
}
