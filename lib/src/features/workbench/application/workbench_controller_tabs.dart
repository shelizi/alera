part of 'workbench_controller.dart';

mixin _WorkbenchControllerTabs
    on _$WorkbenchController, _WorkbenchControllerInternals {
  Future<void> closeWorkspaceTab({
    required Workspace workspace,
    required String tabId,
  }) async {
    await closeWorkspaceTabs(workspace: workspace, tabIds: <String>[tabId]);
  }

  Future<void> closeWorkspaceTabs({
    required Workspace workspace,
    required List<String> tabIds,
  }) async {
    // Capture before closing: the tab watcher can sanitise the layout (and
    // replace the active tab) synchronously while the close is in flight.
    final snapshot = captureWorkbenchTabCloseSnapshot(
      requestedTabIds: tabIds,
      currentTabs: state.tabsFor(workspace.id),
      currentLayout: state.layoutFor(workspace.id),
    );
    final ids = snapshot.closedTabIds;
    if (ids.isEmpty) {
      return;
    }
    await _tabClosingScope.run(workspace.id, () async {
      try {
        await WorkbenchTabCloseCoordinator(
          tabStore: _workspaceTabService,
          hostedReviewRetention: _hostedReviewRetention,
          resourceCleaner: _explicitResourceCleaner,
        ).close(workspace: workspace, snapshot: snapshot);
        await WorkbenchClosedTabsCompletionCoordinator(
          readCurrentTabs: () => state.tabsFor(workspace.id),
          readCurrentLayout: () => state.layoutFor(workspace.id),
          mostRecentOpenTabId: (openTabIds) =>
              _tabFocusHistory.mostRecentOpen(workspace.id, openTabIds),
          applyTabs: (tabs) => _setTabsForWorkspace(workspace.id, tabs),
          forgetFocusHistory: () => _tabFocusHistory.forget(workspace.id),
          applyLayout: (layout) => _applyLayout(layout, persist: true),
          completeRemoval: (remainingTabs) {
            state = completeWorkbenchTabRemovalState(
              state: state,
              workspaceId: workspace.id,
              remainingTabs: remainingTabs,
            );
          },
        ).run(workspaceId: workspace.id, snapshot: snapshot);
      } catch (error) {
        state = state.copyWith(error: error.toString());
        rethrow;
      }
    });
  }

  Future<void> renameWorkspaceTab({
    required String tabId,
    required String title,
  }) async {
    try {
      final tab = await _workspaceTabService.renameTab(
        tabId: tabId,
        title: title,
      );
      if (!_disposed) {
        state = applyWorkbenchTabUpdateState(
          state: state,
          tab: tab,
        ).copyWith(error: null);
      }
    } catch (error) {
      if (!_disposed) {
        state = state.copyWith(error: error.toString());
      }
      rethrow;
    }
  }

  Future<void> syncFileTabsAfterPathMove({
    required Workspace workspace,
    required String oldRelativePath,
    required String newRelativePath,
  }) async {
    try {
      await WorkbenchFileTabPathMoveCoordinator(
        movePaths:
            ({
              required workspaceId,
              required oldRelativePath,
              required newRelativePath,
            }) => _workspaceTabService.updateFileTabPathsAfterMove(
              workspaceId: workspaceId,
              oldRelativePath: oldRelativePath,
              newRelativePath: newRelativePath,
            ),
        readCurrentTabs: () => state.tabsFor(workspace.id),
        readCurrentLayout: () => state.layoutFor(workspace.id),
        applyTabs: (tabs) => _setTabsForWorkspace(workspace.id, tabs),
        applyLayout: (layout) => _applyLayout(layout, persist: true),
        completeRemoval: (remainingTabs) {
          state = completeWorkbenchTabRemovalState(
            state: state,
            workspaceId: workspace.id,
            remainingTabs: remainingTabs,
          );
        },
      ).run(
        workspaceId: workspace.id,
        oldRelativePath: oldRelativePath,
        newRelativePath: newRelativePath,
      );
    } catch (error) {
      state = state.copyWith(error: error.toString());
      rethrow;
    }
  }

  void setActiveTab({required String workspaceId, required String tabId}) {
    final layout = state.layoutFor(workspaceId);
    final groupId = layout?.groupIdForTab(tabId);
    _setActiveTabInternal(
      workspaceId: workspaceId,
      tabId: tabId,
      groupId: groupId,
    );
  }

  void setActiveWorkspaceTab({
    required String workspaceId,
    required String groupId,
    required String tabId,
  }) {
    _setActiveTabInternal(
      workspaceId: workspaceId,
      groupId: groupId,
      tabId: tabId,
    );
  }

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
  }) {
    final layout = state.layoutFor(workspaceId);
    if (layout == null || layout.activeGroupId == groupId) {
      return;
    }
    final tabId = layout.groups[groupId]?.activeTabId;
    if (tabId == null) {
      return;
    }
    final nextLayout = layout.setActiveTab(groupId: groupId, tabId: tabId);
    _applyLayoutInBackground(nextLayout, persist: false);
  }

  Future<void> moveWorkspaceTab({
    required String workspaceId,
    required String tabId,
    required String targetGroupId,
    required WorkbenchDropZone zone,
    int? index,
  }) async {
    try {
      final tabs = state.tabsFor(workspaceId);
      final layout = _layoutForMutation(workspaceId, tabs);
      final nextLayout = layout
          .moveTab(
            tabId: tabId,
            targetGroupId: targetGroupId,
            zone: zone,
            newGroupId: _newPaneGroupId(),
            index: index,
          )
          .sanitize(tabs);
      await _applyLayout(nextLayout, persist: true);
      state = state.copyWith(error: null);
    } catch (error) {
      state = state.copyWith(error: error.toString());
      rethrow;
    }
  }

  Future<WorkspaceTabRecord> splitWorkbenchGroupWithTerminal({
    required Workspace workspace,
    required String groupId,
    required WorkbenchDropZone zone,
  }) async {
    try {
      final previousTabs = state.tabsFor(workspace.id);
      final layout = _layoutForMutation(workspace.id, previousTabs);
      final tab = await WorkbenchTabPlacementCoordinator(
        openTab: () => _workspaceTabService.createTerminalTab(workspace.id),
        planPlacement: (tab) => planWorkbenchTabSplitIntoGroup(
          previousTabs: previousTabs,
          layout: layout,
          tab: tab,
          targetGroupId: groupId,
          zone: zone,
          newGroupId: _newPaneGroupId(),
        ),
        applyTabs: (tabs) => _setTabsForWorkspace(workspace.id, tabs),
        applyLayout: (layout) => _applyLayout(layout, persist: true),
      ).run();
      state = state.copyWith(error: null);
      return tab;
    } catch (error) {
      state = state.copyWith(error: error.toString());
      rethrow;
    }
  }

  Future<void> mergeWorkbenchGroupIntoSibling({
    required String workspaceId,
    required String groupId,
  }) async {
    try {
      final tabs = state.tabsFor(workspaceId);
      final layout = _layoutForMutation(
        workspaceId,
        tabs,
      ).mergeGroupIntoSibling(groupId).sanitize(tabs);
      await _applyLayout(layout, persist: true);
      state = state.copyWith(error: null);
    } catch (error) {
      state = state.copyWith(error: error.toString());
      rethrow;
    }
  }

  void updateWorkbenchSplitRatio({
    required String workspaceId,
    required List<int> nodePath,
    required double ratio,
  }) {
    final tabs = state.tabsFor(workspaceId);
    final layout = _layoutForMutation(
      workspaceId,
      tabs,
    ).updateSplitRatio(nodePath, ratio).sanitize(tabs);
    _applyLayoutInBackground(layout, persist: true);
  }
}
