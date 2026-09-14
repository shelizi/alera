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
    Map<String, Object?>? initialManagedAgentLaunch,
    bool spawnOnCreate = false,
    bool initialCommandOnce = false,
    bool autoCloseOnSuccess = false,
  }) => _tabLayoutOwner.createTerminalTab(
    workspace,
    targetGroupId: targetGroupId,
    title: title,
    initialCommand: initialCommand,
    initialManagedAgentLaunch: initialManagedAgentLaunch,
    spawnOnCreate: spawnOnCreate,
    initialCommandOnce: initialCommandOnce,
    autoCloseOnSuccess: autoCloseOnSuccess,
  );

  /// A terminal tab that launches a locally installed agent CLI. The command
  /// stays on the record so a transparent PTY remint re-enters the agent,
  /// which is what keeps the tab's agent identity across host restarts.
  Future<WorkspaceTabRecord> createAgentTab(
    Workspace workspace, {
    required AgentType agentType,
    String? targetGroupId,
    String? executablePath,
  }) => _tabLayoutOwner.createAgentTab(
    workspace,
    agentType: agentType,
    targetGroupId: targetGroupId,
    executablePath: executablePath,
  );

  Future<WorkspaceTabRecord> openMermanPreviewTab({
    required Workspace workspace,
    required String relativePath,
    String? targetGroupId,
  }) => _tabLayoutOwner.openMermanPreviewTab(
    workspace: workspace,
    relativePath: relativePath,
    targetGroupId: targetGroupId,
  );

  /// The main-area commit graph tab for a workspace's source-control scope.
  Future<WorkspaceTabRecord> openGitHistoryTab({
    required Workspace workspace,
    String? gitDiffRoot,
    String? targetGroupId,
  }) => _tabLayoutOwner.openGitHistoryTab(
    workspace: workspace,
    gitDiffRoot: gitDiffRoot,
    targetGroupId: targetGroupId,
  );

  /// Persists the commit-graph tab's All Branches toggle.
  Future<void> setGitHistoryAllBranches({
    required String tabId,
    required bool allBranches,
  }) => _tabLayoutOwner.setGitHistoryAllBranches(
    tabId: tabId,
    allBranches: allBranches,
  );
}
