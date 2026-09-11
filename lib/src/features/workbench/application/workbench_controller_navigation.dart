part of 'workbench_controller.dart';

mixin _WorkbenchControllerNavigation
    on
        _$WorkbenchController,
        _WorkbenchControllerInternals,
        _WorkbenchControllerProjects {
  Future<void> openPersistedWorkspaceTab({
    required String workspaceId,
    required String tabId,
  }) => WorkbenchPersistedTabOpenCoordinator(
    findTab: _repository.findWorkspaceTabById,
    isDisposed: () => _disposed,
    readCurrentTabs: () => state.tabsFor(workspaceId),
    layoutForMutation: (tabs) => _layoutForMutation(workspaceId, tabs),
    applyTabs: (tabs) => _setTabsForWorkspace(workspaceId, tabs),
    applyLayout: (layout) => _applyLayout(layout, persist: true),
    selectTab: ({required workspaceId, required tabId}) =>
        selectWorkspaceTab(workspaceId: workspaceId, tabId: tabId),
  ).open(workspaceId: workspaceId, tabId: tabId);

  Future<void> selectWorkspaceTab({
    required String workspaceId,
    required String tabId,
  }) async {
    final workspace = state.workspacesByProject.values
        .expand((workspaces) => workspaces)
        .where((workspace) => workspace.id == workspaceId)
        .firstOrNull;
    final project = workspace == null
        ? null
        : _projectById(state.projects, workspace.projectId);
    if (workspace == null || project == null) return;
    if (state.activeWorkspaceId != workspaceId) {
      await selectWorkspace(project: project, workspace: workspace);
      if (!state.tabsFor(workspaceId).any((tab) => tab.id == tabId)) {
        return;
      }
    }
    final groupId = state.layoutFor(workspaceId)?.groupIdForTab(tabId);
    _setActiveTabInternal(
      workspaceId: workspaceId,
      tabId: tabId,
      groupId: groupId,
    );
  }

  Future<void> goBack() async {
    final changed = await _navigationHistory.navigateBack(
      state,
      select: (selection) => _selectWorkspace(
        project: selection.project,
        workspace: selection.workspace,
        ensureInitialTerminal: true,
        recordHistory: false,
      ),
    );
    if (changed) {
      _notifyNavigationHistoryChanged();
    }
  }

  Future<void> goForward() async {
    final changed = await _navigationHistory.navigateForward(
      state,
      select: (selection) => _selectWorkspace(
        project: selection.project,
        workspace: selection.workspace,
        ensureInitialTerminal: true,
        recordHistory: false,
      ),
    );
    if (changed) {
      _notifyNavigationHistoryChanged();
    }
  }
}
