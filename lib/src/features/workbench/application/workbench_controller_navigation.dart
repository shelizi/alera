part of 'workbench_controller.dart';

mixin _WorkbenchControllerNavigation
    on
        _$WorkbenchController,
        _WorkbenchControllerInternals,
        _WorkbenchControllerProjects {
  Future<void> openPersistedWorkspaceTab({
    required String workspaceId,
    required String tabId,
  }) => _tabLayoutOwner.openPersistedWorkspaceTab(
    workspaceId: workspaceId,
    tabId: tabId,
  );

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
