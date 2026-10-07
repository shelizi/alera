part of 'workbench_controller.dart';

mixin _WorkbenchControllerTabs
    on _$WorkbenchController, _WorkbenchControllerInternals {
  Future<void> closeWorkspaceTab({
    required Workspace workspace,
    required String tabId,
  }) => _tabLayoutOwner.closeWorkspaceTab(workspace: workspace, tabId: tabId);

  Future<void> closeWorkspaceTabs({
    required Workspace workspace,
    required List<String> tabIds,
  }) =>
      _tabLayoutOwner.closeWorkspaceTabs(workspace: workspace, tabIds: tabIds);

  Future<void> renameWorkspaceTab({
    required String tabId,
    required String title,
  }) => _tabLayoutOwner.renameWorkspaceTab(tabId: tabId, title: title);

  Future<void> setWorkspaceTabPinned({
    required String tabId,
    required bool pinned,
  }) => _tabLayoutOwner.setWorkspaceTabPinned(tabId: tabId, pinned: pinned);

  Future<void> syncFileTabsAfterPathMove({
    required Workspace workspace,
    required String oldRelativePath,
    required String newRelativePath,
  }) => _tabLayoutOwner.syncFileTabsAfterPathMove(
    workspace: workspace,
    oldRelativePath: oldRelativePath,
    newRelativePath: newRelativePath,
  );

  void setActiveTab({required String workspaceId, required String tabId}) =>
      _selectionOwner.setActiveTab(workspaceId: workspaceId, tabId: tabId);

  void setActiveWorkspaceTab({
    required String workspaceId,
    required String groupId,
    required String tabId,
  }) => _selectionOwner.setActiveWorkspaceTab(
    workspaceId: workspaceId,
    groupId: groupId,
    tabId: tabId,
  );

  /// Promotes [groupId] to the workspace's active group when a pane in it
  /// receives real keyboard focus. Idempotent so it can be wired directly to
  /// focus-change events without risking loops or redundant rebuilds.
  ///
  /// Focus is ephemeral session state, so the new layout is applied in memory
  /// only; the next explicit user action (tab click, split, merge, rename)
  /// will persist the latest layout. This avoids a SQLite write on every
  /// pane click.
  void focusWorkbenchGroup({
    required String workspaceId,
    required String groupId,
  }) => _selectionOwner.focusWorkbenchGroup(
    workspaceId: workspaceId,
    groupId: groupId,
  );

  Future<void> moveWorkspaceTab({
    required String workspaceId,
    required String tabId,
    required String targetGroupId,
    required WorkbenchDropZone zone,
    int? index,
  }) => _tabLayoutOwner.moveWorkspaceTab(
    workspaceId: workspaceId,
    tabId: tabId,
    targetGroupId: targetGroupId,
    zone: zone,
    index: index,
  );

  Future<WorkspaceTabRecord> splitWorkbenchGroupWithTerminal({
    required Workspace workspace,
    required String groupId,
    required WorkbenchDropZone zone,
  }) => _tabLayoutOwner.splitWorkbenchGroupWithTerminal(
    workspace: workspace,
    groupId: groupId,
    zone: zone,
  );

  Future<void> mergeWorkbenchGroupIntoSibling({
    required String workspaceId,
    required String groupId,
  }) => _tabLayoutOwner.mergeWorkbenchGroupIntoSibling(
    workspaceId: workspaceId,
    groupId: groupId,
  );

  void updateWorkbenchSplitRatio({
    required String workspaceId,
    required List<int> nodePath,
    required double ratio,
  }) => _tabLayoutOwner.updateWorkbenchSplitRatio(
    workspaceId: workspaceId,
    nodePath: nodePath,
    ratio: ratio,
  );
}
