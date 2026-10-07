part of 'workbench_tab_layout_owner.dart';

/// The lifecycle of tabs that already exist: closing, renaming, moving and
/// pane-layout operations.
extension WorkbenchTabLayoutOwnerTabs on WorkbenchTabLayoutOwner {
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
      currentTabs: _host.readState().tabsFor(workspace.id),
      currentLayout: _host.readState().layoutFor(workspace.id),
    );
    final ids = snapshot.closedTabIds;
    if (ids.isEmpty) {
      return;
    }
    await _tabClosingScope.run(workspace.id, () async {
      try {
        await WorkbenchTabCloseCoordinator(
          tabStore: _host.workspaceTabService,
          hostedReviewRetention: _host.hostedReviewRetention,
          resourceCleaner: _host.explicitResourceCleaner,
        ).close(workspace: workspace, snapshot: snapshot);
        await WorkbenchClosedTabsCompletionCoordinator(
          readCurrentTabs: () => _host.readState().tabsFor(workspace.id),
          readCurrentLayout: () => _host.readState().layoutFor(workspace.id),
          mostRecentOpenTabId: (openTabIds) =>
              _tabFocusHistory.mostRecentOpen(workspace.id, openTabIds),
          applyTabs: (tabs) => applyWorkspaceTabs(workspace.id, tabs),
          forgetFocusHistory: () => _tabFocusHistory.forget(workspace.id),
          applyLayout: (layout) => applyWorkspaceLayout(layout, persist: true),
          completeRemoval: (remainingTabs) {
            _host.emitState(
              completeWorkbenchTabRemovalState(
                state: _host.readState(),
                workspaceId: workspace.id,
                remainingTabs: remainingTabs,
              ),
            );
          },
        ).run(workspaceId: workspace.id, snapshot: snapshot);
      } catch (error) {
        _host.emitState(_host.readState().copyWith(error: error.toString()));
        rethrow;
      }
    });
  }

  Future<void> renameWorkspaceTab({
    required String tabId,
    required String title,
  }) async {
    try {
      final tab = await _host.workspaceTabService.renameTab(
        tabId: tabId,
        title: title,
      );
      if (!_host.isDisposed) {
        _host.emitState(
          applyWorkbenchTabUpdateState(
            state: _host.readState(),
            tab: tab,
          ).copyWith(error: null),
        );
      }
    } catch (error) {
      if (!_host.isDisposed) {
        _host.emitState(_host.readState().copyWith(error: error.toString()));
      }
      rethrow;
    }
  }

  Future<void> setWorkspaceTabPinned({
    required String tabId,
    required bool pinned,
  }) async {
    try {
      final tab = await _host.workspaceTabService.setTabPinned(
        tabId: tabId,
        pinned: pinned,
      );
      if (!_host.isDisposed) {
        _host.emitState(
          applyWorkbenchTabUpdateState(
            state: _host.readState(),
            tab: tab,
          ).copyWith(error: null),
        );
      }
    } catch (error) {
      if (!_host.isDisposed) {
        _host.emitState(_host.readState().copyWith(error: error.toString()));
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
            }) => _host.workspaceTabService.updateFileTabPathsAfterMove(
              workspaceId: workspaceId,
              oldRelativePath: oldRelativePath,
              newRelativePath: newRelativePath,
            ),
        readCurrentTabs: () => _host.readState().tabsFor(workspace.id),
        readCurrentLayout: () => _host.readState().layoutFor(workspace.id),
        applyTabs: (tabs) => applyWorkspaceTabs(workspace.id, tabs),
        applyLayout: (layout) => applyWorkspaceLayout(layout, persist: true),
        completeRemoval: (remainingTabs) {
          _host.emitState(
            completeWorkbenchTabRemovalState(
              state: _host.readState(),
              workspaceId: workspace.id,
              remainingTabs: remainingTabs,
            ),
          );
        },
      ).run(
        workspaceId: workspace.id,
        oldRelativePath: oldRelativePath,
        newRelativePath: newRelativePath,
      );
    } catch (error) {
      _host.emitState(_host.readState().copyWith(error: error.toString()));
      rethrow;
    }
  }

  Future<void> moveWorkspaceTab({
    required String workspaceId,
    required String tabId,
    required String targetGroupId,
    required WorkbenchDropZone zone,
    int? index,
  }) async {
    try {
      final tabs = _host.readState().tabsFor(workspaceId);
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
      await applyWorkspaceLayout(nextLayout, persist: true);
      _host.emitState(_host.readState().copyWith(error: null));
    } catch (error) {
      _host.emitState(_host.readState().copyWith(error: error.toString()));
      rethrow;
    }
  }

  Future<WorkspaceTabRecord> splitWorkbenchGroupWithTerminal({
    required Workspace workspace,
    required String groupId,
    required WorkbenchDropZone zone,
  }) async {
    try {
      final previousTabs = _host.readState().tabsFor(workspace.id);
      final layout = _layoutForMutation(workspace.id, previousTabs);
      final tab = await WorkbenchTabPlacementCoordinator(
        openTab: () =>
            _host.workspaceTabService.createTerminalTab(workspace.id),
        planPlacement: (tab) => planWorkbenchTabSplitIntoGroup(
          previousTabs: previousTabs,
          layout: layout,
          tab: tab,
          targetGroupId: groupId,
          zone: zone,
          newGroupId: _newPaneGroupId(),
        ),
        applyTabs: (tabs) => applyWorkspaceTabs(workspace.id, tabs),
        applyLayout: (layout) => applyWorkspaceLayout(layout, persist: true),
      ).run();
      _host.emitState(_host.readState().copyWith(error: null));
      return tab;
    } catch (error) {
      _host.emitState(_host.readState().copyWith(error: error.toString()));
      rethrow;
    }
  }

  Future<void> mergeWorkbenchGroupIntoSibling({
    required String workspaceId,
    required String groupId,
  }) async {
    try {
      final tabs = _host.readState().tabsFor(workspaceId);
      final layout = _layoutForMutation(
        workspaceId,
        tabs,
      ).mergeGroupIntoSibling(groupId).sanitize(tabs);
      await applyWorkspaceLayout(layout, persist: true);
      _host.emitState(_host.readState().copyWith(error: null));
    } catch (error) {
      _host.emitState(_host.readState().copyWith(error: error.toString()));
      rethrow;
    }
  }

  void updateWorkbenchSplitRatio({
    required String workspaceId,
    required List<int> nodePath,
    required double ratio,
  }) {
    final tabs = _host.readState().tabsFor(workspaceId);
    final layout = _layoutForMutation(
      workspaceId,
      tabs,
    ).updateSplitRatio(nodePath, ratio).sanitize(tabs);
    applyWorkspaceLayoutInBackground(layout, persist: true);
  }
}
