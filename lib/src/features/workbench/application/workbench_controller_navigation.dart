part of 'workbench_controller.dart';

mixin _WorkbenchControllerNavigation
    on
        _$WorkbenchController,
        _WorkbenchControllerInternals,
        _WorkbenchControllerProjects {
  Future<void> openPersistedWorkspaceTab({
    required String workspaceId,
    required String tabId,
  }) async {
    final tab = await _repository.findWorkspaceTabById(tabId);
    if (_disposed) return;
    if (tab == null || tab.workspaceId != workspaceId) {
      throw StateError(
        'The created tab is no longer available in this workspace.',
      );
    }
    final currentTabs = state.tabsFor(workspaceId);
    final layout = _layoutForMutation(workspaceId, currentTabs);
    final tabs = <WorkspaceTabRecord>[
      for (final current in currentTabs) current.id == tabId ? tab : current,
      if (!currentTabs.any((current) => current.id == tabId)) tab,
    ];
    _setTabsForWorkspace(workspaceId, tabs);
    if (layout.groupIdForTab(tabId) == null) {
      await _applyLayout(
        layout
            .addTabToGroup(groupId: layout.activeGroupId, tabId: tabId)
            .sanitize(tabs),
        persist: true,
      );
    }
    if (!_disposed) {
      await selectWorkspaceTab(workspaceId: workspaceId, tabId: tabId);
    }
  }

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
    }
    final groupId = state.layoutFor(workspaceId)?.groupIdForTab(tabId);
    _setActiveTabInternal(
      workspaceId: workspaceId,
      tabId: tabId,
      groupId: groupId,
    );
  }

  Future<void> goBack() async {
    final selection = _navigationHistory.peekBack(state);
    if (selection == null) {
      return;
    }
    await _selectWorkspace(
      project: selection.project,
      workspace: selection.workspace,
      ensureInitialTerminal: true,
      recordHistory: false,
    );
    _navigationHistory.commitBack(selection);
    _notifyNavigationHistoryChanged();
  }

  Future<void> goForward() async {
    final selection = _navigationHistory.peekForward(state);
    if (selection == null) {
      return;
    }
    await _selectWorkspace(
      project: selection.project,
      workspace: selection.workspace,
      ensureInitialTerminal: true,
      recordHistory: false,
    );
    _navigationHistory.commitForward(selection);
    _notifyNavigationHistoryChanged();
  }
}
