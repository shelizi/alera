part of 'workbench_controller.dart';

/// Opening tabs: every entry point that adds a tab to a workspace.
///
/// Split out of `workbench_controller_tabs.dart`, which keeps the lifecycle of
/// tabs that already exist: closing, renaming, moving and splitting.
mixin _WorkbenchControllerTabOpening
    on _$WorkbenchController, _WorkbenchControllerInternals {
  Future<WorkspaceTabRecord> createTerminalTab(
    Workspace workspace, {
    String? targetGroupId,
    String? title,
    String? initialCommand,
    bool spawnOnCreate = false,
    bool initialCommandOnce = false,
    bool autoCloseOnSuccess = false,
  }) async {
    try {
      final previousTabs = state.tabsFor(workspace.id);
      final layout = _layoutForMutation(workspace.id, previousTabs);
      final tab = await WorkbenchTabPlacementCoordinator(
        openTab: () => _workspaceTabService.createTerminalTab(
          workspace.id,
          title: title,
          initialCommand: initialCommand,
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
        applyTabs: (tabs) => _setTabsForWorkspace(workspace.id, tabs),
        applyLayout: (layout) => _applyLayout(layout, persist: true),
        afterPlaced: (_) => _workspaceActivityRecorder.recordActivity(
          workspace.id,
          DateTime.now().toUtc(),
        ),
      ).run();
      state = state.copyWith(error: null);
      return tab;
    } catch (error) {
      state = state.copyWith(error: error.toString());
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
  }) {
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
  Future<void> _openDeferredSetupTab(WorkspaceCreationResult result) async {
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
      state = state.copyWith(error: error.toString());
    }
  }

  Future<WorkspaceTabRecord> openMermanPreviewTab({
    required Workspace workspace,
    required String relativePath,
    String? targetGroupId,
  }) async {
    try {
      final previousTabs = state.tabsFor(workspace.id);
      final layout = _layoutForMutation(workspace.id, previousTabs);
      final tab = await WorkbenchTabPlacementCoordinator(
        openTab: () => _workspaceTabService.openOrCreateMermanPreviewTab(
          workspaceId: workspace.id,
          relativePath: relativePath,
        ),
        planPlacement: (tab) => planWorkbenchReusableTabToGroup(
          previousTabs: previousTabs,
          layout: layout,
          tab: tab,
          targetGroupId: targetGroupId,
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
}
