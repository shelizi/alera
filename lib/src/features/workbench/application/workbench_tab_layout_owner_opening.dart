part of 'workbench_tab_layout_owner.dart';

/// Opening tabs: every entry point that adds a tab to a workspace.
extension WorkbenchTabLayoutOwnerOpening on WorkbenchTabLayoutOwner {
  Future<WorkspaceTabRecord> createTerminalTab(
    Workspace workspace, {
    String? targetGroupId,
    String? title,
    String? initialCommand,
    Map<String, Object?>? initialManagedAgentLaunch,
    bool spawnOnCreate = false,
    bool initialCommandOnce = false,
    bool autoCloseOnSuccess = false,
  }) async {
    try {
      final previousTabs = _host.readState().tabsFor(workspace.id);
      final layout = _layoutForMutation(workspace.id, previousTabs);
      final tab = await WorkbenchTabPlacementCoordinator(
        openTab: () => _host.workspaceTabService.createTerminalTab(
          workspace.id,
          title: title,
          initialCommand: initialCommand,
          initialManagedAgentLaunch: initialManagedAgentLaunch,
          spawnOnCreate: spawnOnCreate,
          initialCommandOnce: initialCommandOnce,
          autoCloseOnSuccess: autoCloseOnSuccess,
        ),
        planPlacement: (tab) => planWorkbenchTabAddedToGroup(
          previousTabs: previousTabs,
          layout: layout,
          tab: tab,
          targetGroupId: targetGroupId,
        ),
        applyTabs: (tabs) => applyWorkspaceTabs(workspace.id, tabs),
        applyLayout: (layout) => applyWorkspaceLayout(layout, persist: true),
        afterPlaced: (_) => _host.workspaceActivityRecorder.recordActivity(
          workspace.id,
          DateTime.now().toUtc(),
        ),
      ).run();
      _host.emitState(_host.readState().copyWith(error: null));
      return tab;
    } catch (error) {
      _host.emitState(_host.readState().copyWith(error: error.toString()));
      rethrow;
    }
  }

  /// A terminal tab that launches a locally installed agent CLI. The command
  /// stays on the record so a transparent PTY remint re-enters the agent,
  /// which is what keeps the tab's agent identity across host restarts.
  Future<WorkspaceTabRecord> createAgentTab(
    Workspace workspace, {
    required AgentType agentType,
    String? targetGroupId,
    String? executablePath,
  }) {
    final configuredExecutable = executablePath?.trim();
    if (configuredExecutable != null && configuredExecutable.isNotEmpty) {
      return createTerminalTab(
        workspace,
        targetGroupId: targetGroupId,
        title: agentDisplayName(agentType),
        initialManagedAgentLaunch: <String, Object?>{
          'executable': configuredExecutable,
          'arguments': <String>[],
        },
        spawnOnCreate: true,
      );
    }
    final command = agentProfileDefaultCommands[agentType];
    if (command == null) {
      throw StateError('No default launch command for agent ${agentType.key}.');
    }
    return createTerminalTab(
      workspace,
      targetGroupId: targetGroupId,
      title: agentDisplayName(agentType),
      initialCommand: command,
      spawnOnCreate: true,
    );
  }

  /// Opens the "Setup" terminal for a workspace whose worktree setup the host
  /// prepared instead of running, so a long `pnpm install` is visible work
  /// rather than a spinner on the create dialog.
  ///
  /// A failure here does not fail the creation: the workspace exists and the
  /// setup can be run by hand, so it is reported as an error on the state
  /// instead of unwinding the flow.
  Future<void> openDeferredSetupTab(WorkspaceCreationResult result) async {
    final command = result.deferredSetupCommand?.trim();
    if (command == null || command.isEmpty) {
      return;
    }
    try {
      await createTerminalTab(
        result.workspace,
        title: 'Setup',
        initialCommand: command,
        spawnOnCreate: true,
        initialCommandOnce: true,
        autoCloseOnSuccess: true,
      );
    } catch (error) {
      _host.emitState(_host.readState().copyWith(error: error.toString()));
    }
  }

  Future<WorkspaceTabRecord> openMermanPreviewTab({
    required Workspace workspace,
    required String relativePath,
    String? targetGroupId,
  }) async {
    try {
      final previousTabs = _host.readState().tabsFor(workspace.id);
      final layout = _layoutForMutation(workspace.id, previousTabs);
      final tab = await WorkbenchTabPlacementCoordinator(
        openTab: () => _host.workspaceTabService.openOrCreateMermanPreviewTab(
          workspaceId: workspace.id,
          relativePath: relativePath,
        ),
        planPlacement: (tab) => planWorkbenchReusableTabToGroup(
          previousTabs: previousTabs,
          layout: layout,
          tab: tab,
          targetGroupId: targetGroupId,
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

  Future<WorkspaceTabRecord> openGitHistoryTab({
    required Workspace workspace,
    String? gitDiffRoot,
    String? targetGroupId,
  }) async {
    try {
      final previousTabs = _host.readState().tabsFor(workspace.id);
      final layout = _layoutForMutation(workspace.id, previousTabs);
      final tab = await WorkbenchTabPlacementCoordinator(
        openTab: () => _host.workspaceTabService.openOrCreateGitHistoryTab(
          workspaceId: workspace.id,
          gitDiffRoot: gitDiffRoot,
        ),
        planPlacement: (tab) => planWorkbenchReusableTabToGroup(
          previousTabs: previousTabs,
          layout: layout,
          tab: tab,
          targetGroupId: targetGroupId,
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

  Future<void> setGitHistoryAllBranches({
    required String tabId,
    required bool allBranches,
    String? selectedRef,
  }) async {
    try {
      final tab = await _host.workspaceTabService.setGitHistoryAllBranches(
        tabId: tabId,
        allBranches: allBranches,
        selectedRef: selectedRef,
      );
      if (tab != null && !_host.isDisposed) {
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

  Future<void> setGitHistoryMergedBranchFilter({
    required String tabId,
    required WorkspaceGitHistoryMergedBranchVisibility visibility,
    String? mergedIntoRef,
  }) async {
    try {
      final tab = await _host.workspaceTabService
          .setGitHistoryMergedBranchFilter(
            tabId: tabId,
            visibility: visibility,
            mergedIntoRef: mergedIntoRef,
          );
      if (tab != null && !_host.isDisposed) {
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

  Future<WorkspaceTabRecord> openGitPullRequestDiffTab({
    required Workspace workspace,
    String? gitDiffRoot,
    required int pullRequestNumber,
    required String commitOid,
    required String parentOid,
    required String retentionId,
    String? subject,
    String? targetGroupId,
  }) async {
    try {
      final previousTabs = _host.readState().tabsFor(workspace.id);
      final layout = _layoutForMutation(workspace.id, previousTabs);
      final tab =
          await WorkbenchPullRequestDiffTabOpenCoordinator(
            tabStore: WorkbenchPullRequestDiffTabStoreAdapter(
              _host.workspaceTabService,
            ),
            hostedReviewRetention: _host.hostedReviewRetention,
          ).open(
            workspace: workspace,
            previousTabs: previousTabs,
            gitDiffRoot: gitDiffRoot,
            pullRequestNumber: pullRequestNumber,
            commitOid: commitOid,
            parentOid: parentOid,
            retentionId: retentionId,
            subject: subject,
            planPlacement: (tab) => planWorkbenchReusableTabToGroup(
              previousTabs: previousTabs,
              layout: layout,
              tab: tab,
              targetGroupId: targetGroupId,
            ),
            applyTabs: (tabs) => applyWorkspaceTabs(workspace.id, tabs),
            applyLayout: (layout) =>
                applyWorkspaceLayout(layout, persist: true),
          );
      _host.emitState(_host.readState().copyWith(error: null));
      return tab;
    } catch (error) {
      _host.emitState(_host.readState().copyWith(error: error.toString()));
      rethrow;
    }
  }

  Future<void> openPersistedWorkspaceTab({
    required String workspaceId,
    required String tabId,
  }) => WorkbenchPersistedTabOpenCoordinator(
    findTab: _host.findPersistedWorkspaceTab,
    isDisposed: () => _host.isDisposed,
    readCurrentTabs: () => _host.readState().tabsFor(workspaceId),
    layoutForMutation: (tabs) => _layoutForMutation(workspaceId, tabs),
    applyTabs: (tabs) => applyWorkspaceTabs(workspaceId, tabs),
    applyLayout: (layout) => applyWorkspaceLayout(layout, persist: true),
    selectTab: ({required workspaceId, required tabId}) => _host
        .selectPersistedWorkspaceTab(workspaceId: workspaceId, tabId: tabId),
  ).open(workspaceId: workspaceId, tabId: tabId);
}
