part of 'workbench_catalog_owner.dart';

/// Creates workspaces and initializes their first tabs.
///
/// Kept separate from project lifecycle actions because the From Prompt flow
/// deliberately persists its agent terminal before the Setup terminal.
extension WorkbenchCatalogOwnerWorkspaceCreation on WorkbenchCatalogOwner {
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
              () => _host.workspaceService.createLinkedWorkspace(
                project: project,
                sourceBranch: sourceBranch,
                newBranchName: newBranchName,
                reuseExistingBranch: reuseExistingBranch,
                name: name,
              ),
            ),
            reconcileWorkspace: _reconcileWorkspace,
            selectWorkspace: (project, workspace) =>
                _host.selectCatalogWorkspace(
                  project: project,
                  workspace: workspace,
                  ensureInitialTerminal: true,
                ),
            openDeferredSetupTab: _host.openDeferredSetupTab,
            attachParent: ({required result, parentWorkspaceId}) =>
                WorkbenchWorkspaceCreationParentLinkService(
                  _host.workspaceGraphRepository,
                ).attach(result: result, parentWorkspaceId: parentWorkspaceId),
          ).run(
            project: project,
            initializeTabs: initializeTabs,
            parentWorkspaceId: parentWorkspaceId,
          );
      _host.emitState(_host.readState().copyWith(error: null));
      return completed;
    } catch (error) {
      _host.emitState(_host.readState().copyWith(error: error.toString()));
      rethrow;
    }
  }

  Future<Workspace> switchWorkspaceBranch({
    required Project project,
    required Workspace workspace,
    required String branch,
  }) async {
    try {
      final switched = await _host.workspaceService.switchWorkspaceBranch(
        project: project,
        workspace: workspace,
        branch: branch,
      );
      _host.emitState(
        applyWorkbenchWorkspaceUpdateState(
          state: _host.readState(),
          workspace: switched,
        ).copyWith(error: null),
      );
      return switched;
    } catch (error) {
      _host.emitState(_host.readState().copyWith(error: error.toString()));
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
    final project = _host
        .readState()
        .projects
        .where((candidate) => candidate.id == workspace.projectId)
        .firstOrNull;
    if (project == null) {
      throw StateError('Workspace project not found: ${workspace.projectId}');
    }
    await WorkbenchPromptWorkspaceCompletionCoordinator(
      selectWorkspace: ({required ensureInitialTerminal}) =>
          _host.selectCatalogWorkspace(
            project: project,
            workspace: workspace,
            ensureInitialTerminal: ensureInitialTerminal,
          ),
      openDeferredSetupTab: _host.openDeferredSetupTab,
      refocusAgentTab: (tabId) {
        final groupId = _host
            .readState()
            .layoutFor(workspace.id)
            ?.groupIdForTab(tabId);
        _host.activateWorkspaceTab(
          workspaceId: workspace.id,
          tabId: tabId,
          groupId: groupId,
        );
      },
    ).run(creation: creation, agentTabId: agentTabId);
  }

  void _reconcileWorkspace(Project project, Workspace workspace) {
    _host.emitState(
      reconcileWorkbenchWorkspaceState(
        state: _host.readState(),
        projectId: project.id,
        workspace: workspace,
      ),
    );
  }
}
