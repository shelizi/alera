part of 'workbench_controller.dart';

/// Creates workspaces and initializes their first tabs.
///
/// Kept separate from project lifecycle actions because the From Prompt flow
/// deliberately persists its agent terminal before the Setup terminal.
mixin _WorkbenchControllerWorkspaceCreation
    on
        _$WorkbenchController,
        _WorkbenchControllerInternals,
        _WorkbenchControllerTabOpening,
        _WorkbenchControllerProjects {
  Future<WorkspaceCreationResult> createWorkspace({
    required Project project,
    required String sourceBranch,
    required String newBranchName,
    bool reuseExistingBranch = false,
    String? name,
    String? parentWorkspaceId,
  }) {
    return _createWorkspace(
      project: project,
      sourceBranch: sourceBranch,
      newBranchName: newBranchName,
      reuseExistingBranch: reuseExistingBranch,
      name: name,
      parentWorkspaceId: parentWorkspaceId,
      initializeTabs: true,
    );
  }

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
  }) {
    return _createWorkspace(
      project: project,
      sourceBranch: sourceBranch,
      newBranchName: newBranchName,
      reuseExistingBranch: false,
      name: name,
      parentWorkspaceId: parentWorkspaceId,
      initializeTabs: false,
    );
  }

  Future<WorkspaceCreationResult> _createWorkspace({
    required Project project,
    required String sourceBranch,
    required String newBranchName,
    required bool reuseExistingBranch,
    required bool initializeTabs,
    String? name,
    String? parentWorkspaceId,
  }) async {
    try {
      final completed =
          await WorkbenchWorkspaceCreationCoordinator(
            createWorkspace: () => _withWorktreeRefreshSuspended(
              project.id,
              () => _workspaceService.createLinkedWorkspace(
                project: project,
                sourceBranch: sourceBranch,
                newBranchName: newBranchName,
                reuseExistingBranch: reuseExistingBranch,
                name: name,
              ),
            ),
            reconcileWorkspace: _reconcileWorkspace,
            selectWorkspace: (project, workspace) =>
                selectWorkspace(project: project, workspace: workspace),
            openDeferredSetupTab: _openDeferredSetupTab,
            attachParent: ({required result, parentWorkspaceId}) =>
                WorkbenchWorkspaceCreationParentLinkService(
                  _workspaceGraphRepository,
                ).attach(result: result, parentWorkspaceId: parentWorkspaceId),
          ).run(
            project: project,
            initializeTabs: initializeTabs,
            parentWorkspaceId: parentWorkspaceId,
          );
      state = state.copyWith(error: null);
      return completed;
    } catch (error) {
      state = state.copyWith(error: error.toString());
      rethrow;
    }
  }

  Future<Workspace> switchWorkspaceBranch({
    required Project project,
    required Workspace workspace,
    required String branch,
  }) async {
    try {
      final switched = await _workspaceService.switchWorkspaceBranch(
        project: project,
        workspace: workspace,
        branch: branch,
      );
      state = applyWorkbenchWorkspaceUpdateState(
        state: state,
        workspace: switched,
      ).copyWith(error: null);
      return switched;
    } catch (error) {
      state = state.copyWith(error: error.toString());
      rethrow;
    }
  }

  /// Activates a workspace created from a prompt after the host has persisted
  /// the agent tab, then appends Setup and restores focus to the agent.
  Future<void> completePromptWorkspaceCreation({
    required WorkspaceCreationResult creation,
    String? agentTabId,
  }) async {
    final workspace = creation.workspace;
    final project = state.projects
        .where((candidate) => candidate.id == workspace.projectId)
        .firstOrNull;
    if (project == null) {
      throw StateError('Workspace project not found: ${workspace.projectId}');
    }
    await WorkbenchPromptWorkspaceCompletionCoordinator(
      selectWorkspace: ({required ensureInitialTerminal}) => _selectWorkspace(
        project: project,
        workspace: workspace,
        ensureInitialTerminal: ensureInitialTerminal,
      ),
      openDeferredSetupTab: _openDeferredSetupTab,
      refocusAgentTab: (tabId) {
        final groupId = state.layoutFor(workspace.id)?.groupIdForTab(tabId);
        _setActiveTabInternal(
          workspaceId: workspace.id,
          tabId: tabId,
          groupId: groupId,
        );
      },
    ).run(creation: creation, agentTabId: agentTabId);
  }

  void _reconcileWorkspace(Project project, Workspace workspace) {
    state = reconcileWorkbenchWorkspaceState(
      state: state,
      projectId: project.id,
      workspace: workspace,
    );
  }
}
