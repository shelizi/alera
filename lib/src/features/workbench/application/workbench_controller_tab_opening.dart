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
  }) => _tabLayoutOwner.createTerminalTab(
    workspace,
    targetGroupId: targetGroupId,
    title: title,
    initialCommand: initialCommand,
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
  }) => _tabLayoutOwner.createAgentTab(
    workspace,
    agentType: agentType,
    targetGroupId: targetGroupId,
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
}
