import 'package:alera/src/features/workbench/application/terminal_runtime_focus.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

typedef WorkbenchSidebarWorkspaceTabSelector = Future<void> Function({
  required String workspaceId,
  required String tabId,
});

typedef WorkbenchSidebarWorkspaceTabsReader = List<WorkspaceTabRecord> Function(
  String workspaceId,
);

final class WorkbenchSidebarTerminalSelectionCoordinator {
  const WorkbenchSidebarTerminalSelectionCoordinator({
    required WorkbenchSidebarWorkspaceTabSelector selectWorkspaceTab,
    required WorkbenchSidebarWorkspaceTabsReader readWorkspaceTabs,
    required TerminalRuntimeFocus terminalFocus,
  }) : _selectWorkspaceTab = selectWorkspaceTab,
       _readWorkspaceTabs = readWorkspaceTabs,
       _terminalFocus = terminalFocus;

  final WorkbenchSidebarWorkspaceTabSelector _selectWorkspaceTab;
  final WorkbenchSidebarWorkspaceTabsReader _readWorkspaceTabs;
  final TerminalRuntimeFocus _terminalFocus;

  Future<void> select({
    required Workspace workspace,
    required String tabId,
  }) async {
    await _selectWorkspaceTab(workspaceId: workspace.id, tabId: tabId);

    WorkspaceTabRecord? selectedTab;
    for (final tab in _readWorkspaceTabs(workspace.id)) {
      if (tab.id == tabId) {
        selectedTab = tab;
        break;
      }
    }
    if (selectedTab == null) {
      return;
    }

    _terminalFocus.requestFocus(workspace: workspace, tab: selectedTab);
  }
}
