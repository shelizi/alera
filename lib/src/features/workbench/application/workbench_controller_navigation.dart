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
  }) => _selectionOwner.selectWorkspaceTab(
    workspaceId: workspaceId,
    tabId: tabId,
  );

  Future<void> goBack() => _selectionOwner.goBack();

  Future<void> goForward() => _selectionOwner.goForward();
}
