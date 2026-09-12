part of 'workbench_controller.dart';

/// Facade over the catalog owner: workspace creation and branch switching.
///
/// Kept separate from project lifecycle actions because the From Prompt flow
/// deliberately persists its agent terminal before the Setup terminal.
mixin _WorkbenchControllerWorkspaceCreation
    on _$WorkbenchController, _WorkbenchControllerInternals {
  Future<WorkspaceCreationResult> createWorkspace({
    required Project project,
    required String sourceBranch,
    required String newBranchName,
    bool reuseExistingBranch = false,
    String? name,
    String? parentWorkspaceId,
  }) => _catalogOwner.createWorkspace(
    project: project,
    sourceBranch: sourceBranch,
    newBranchName: newBranchName,
    reuseExistingBranch: reuseExistingBranch,
    name: name,
    parentWorkspaceId: parentWorkspaceId,
  );

  /// Creates the workspace record for the From Prompt flow without opening a
  /// blank terminal or the deferred Setup tab. The agent profile launch creates
  /// its terminal first, then [completePromptWorkspaceCreation] synchronizes
  /// that tab and appends Setup.
  Future<WorkspaceCreationResult> createWorkspaceForPrompt({
    required Project project,
    required String sourceBranch,
    required String newBranchName,
    required String name,
    String? parentWorkspaceId,
  }) => _catalogOwner.createWorkspaceForPrompt(
    project: project,
    sourceBranch: sourceBranch,
    newBranchName: newBranchName,
    name: name,
    parentWorkspaceId: parentWorkspaceId,
  );

  Future<Workspace> switchWorkspaceBranch({
    required Project project,
    required Workspace workspace,
    required String branch,
  }) => _catalogOwner.switchWorkspaceBranch(
    project: project,
    workspace: workspace,
    branch: branch,
  );

  /// Activates a workspace created from a prompt after the host has persisted
  /// the agent tab, then appends Setup and restores focus to the agent.
  Future<void> completePromptWorkspaceCreation({
    required WorkspaceCreationResult creation,
    String? agentTabId,
  }) => _catalogOwner.completePromptWorkspaceCreation(
    creation: creation,
    agentTabId: agentTabId,
  );
}
