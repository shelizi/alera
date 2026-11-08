part of 'workbench_controller.dart';

/// An existing checkout must be activated, not checked out again in a
/// different worktree (which Git rejects and could disturb local edits).
enum BranchWorkspaceNavigationResult { selectedWorktree, switchedBranch }

Workspace? existingWorktreeForBranch({
  required WorkbenchState state,
  required Workspace currentWorkspace,
  required String branch,
}) => findExistingBranchWorktree(
  workspaces: state.workspacesFor(currentWorkspace.projectId),
  currentWorkspace: currentWorkspace,
  branch: branch,
);

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

  /// Follows the same selection path as clicking an existing left-sidebar
  /// worktree. Only branches without a registered worktree are checked out.
  Future<BranchWorkspaceNavigationResult> switchBranchOrSelectWorktree({
    required Project project,
    required Workspace workspace,
    required String branch,
  }) async {
    if (state.activeWorkspaceId != workspace.id) {
      throw StateError('Workspace changed before branch selection');
    }
    final target = existingWorktreeForBranch(
      state: state,
      currentWorkspace: workspace,
      branch: branch,
    );
    if (target != null) {
      // Reveal the target even when its project or parent worktrees are
      // collapsed. Do not change the current checkout or the target's files.
      var prefs = state.viewPrefs;
      if (prefs.collapsedProjectIds.contains(project.id)) {
        prefs = toggleWorkbenchProjectCollapsedPrefs(
          prefs: prefs,
          projectId: project.id,
        );
      }
      final parents = <String, Workspace>{
        for (final item in state.workspacesFor(project.id)) item.id: item,
      };
      final visited = <String>{};
      var parentId = target.parentWorkspaceId;
      while (parentId != null && visited.add(parentId)) {
        if (prefs.collapsedParentWorkspaceIds.contains(parentId)) {
          prefs = toggleWorkbenchParentWorkspaceCollapsedPrefs(
            prefs: prefs,
            workspaceId: parentId,
          );
        }
        parentId = parents[parentId]?.parentWorkspaceId;
      }
      // The sidebar may be grouped by section rather than project. Reveal
      // the destination in that layout too, without resetting user filters.
      if (prefs.groupBy == WorkbenchGroupBy.section) {
        final sectionId = target.sectionId;
        final knownSection =
            sectionId != null &&
            state.sections.any((section) => section.id == sectionId);
        if (knownSection && prefs.collapsedSectionIds.contains(sectionId)) {
          prefs = prefs.copyWith(
            collapsedSectionIds: Set<String>.of(prefs.collapsedSectionIds)
              ..remove(sectionId),
          );
        } else if (!knownSection && prefs.othersSectionCollapsed) {
          prefs = prefs.copyWith(othersSectionCollapsed: false);
        }
        if (target.isPinned && prefs.pinnedSectionCollapsed) {
          prefs = prefs.copyWith(pinnedSectionCollapsed: false);
        }
      } else if (prefs.groupBy == WorkbenchGroupBy.none &&
          prefs.allSectionCollapsed) {
        prefs = prefs.copyWith(allSectionCollapsed: false);
      }
      if (!identical(prefs, state.viewPrefs)) {
        _updateViewPrefs(prefs);
      }
      if (target.isArchived) {
        // Archived workspaces still have a Git checkout: restore first.
        await _catalogOwner.restoreWorkspace(target, select: true);
      } else {
        await _selectionOwner.selectWorkspace(
          project: project,
          workspace: target,
          ensureInitialTerminal: true,
        );
      }
      return BranchWorkspaceNavigationResult.selectedWorktree;
    }
    await switchWorkspaceBranch(
      project: project,
      workspace: workspace,
      branch: branch,
    );
    return BranchWorkspaceNavigationResult.switchedBranch;
  }

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
