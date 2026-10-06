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
    String? filePath,
    String? targetGroupId,
  }) => _tabLayoutOwner.openGitHistoryTab(
    workspace: workspace,
    gitDiffRoot: gitDiffRoot,
    filePath: filePath,
    targetGroupId: targetGroupId,
  );

  /// Persists the commit-graph branch perspective.
  Future<void> setGitHistoryAllBranches({
    required String tabId,
    required bool allBranches,
    String? selectedRef,
  }) => _tabLayoutOwner.setGitHistoryAllBranches(
    tabId: tabId,
    allBranches: allBranches,
    selectedRef: selectedRef,
  );

  Future<void> setGitHistoryMergedBranchFilter({
    required String tabId,
    required WorkspaceGitHistoryMergedBranchVisibility visibility,
    String? mergedIntoRef,
  }) => _tabLayoutOwner.setGitHistoryMergedBranchFilter(
    tabId: tabId,
    visibility: visibility,
    mergedIntoRef: mergedIntoRef,
  );
}
