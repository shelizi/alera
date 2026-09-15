part of 'workbench_controller.dart';

mixin _WorkbenchControllerViewPrefs
    on _$WorkbenchController, _WorkbenchControllerInternals {
  void toggleExpanded(String projectId) {
    toggleProjectCollapsed(projectId);
  }

  void toggleProjectCollapsed(String projectId) {
    _updateViewPrefs(
      toggleWorkbenchProjectCollapsedPrefs(
        prefs: state.viewPrefs,
        projectId: projectId,
      ),
    );
  }

  void setGroupBy(WorkbenchGroupBy groupBy) {
    if (state.viewPrefs.groupBy == groupBy) {
      return;
    }
    _updateViewPrefs(state.viewPrefs.copyWith(groupBy: groupBy));
  }

  void setProjectSort(WorkbenchSortBy sort) {
    if (state.viewPrefs.projectSort == sort) {
      return;
    }
    _updateViewPrefs(state.viewPrefs.copyWith(projectSort: sort));
  }

  void setWorkspaceSort(WorkbenchSortBy sort) {
    if (state.viewPrefs.workspaceSort == sort) {
      return;
    }
    _updateViewPrefs(state.viewPrefs.copyWith(workspaceSort: sort));
  }

  void setWorkspaceKindFilter(WorkspaceKindFilter filter) {
    if (state.viewPrefs.workspaceKindFilter == filter) {
      return;
    }
    _updateViewPrefs(state.viewPrefs.copyWith(workspaceKindFilter: filter));
  }

  void setShowActiveWorkspacesOnly(bool show) {
    if (state.viewPrefs.showActiveWorkspacesOnly == show) {
      return;
    }
    _updateViewPrefs(state.viewPrefs.copyWith(showActiveWorkspacesOnly: show));
  }

  void setShowPinnedWorkspacesBelow(bool show) {
    if (state.viewPrefs.showPinnedWorkspacesBelow == show) {
      return;
    }
    _updateViewPrefs(state.viewPrefs.copyWith(showPinnedWorkspacesBelow: show));
  }

  void toggleProjectFilter(String projectId) {
    _updateViewPrefsIfChanged(
      nextWorkbenchProjectFilterPrefs(
        prefs: state.viewPrefs,
        id: projectId,
        mutation: WorkbenchFilterMutation.toggle,
      ),
    );
  }

  void addProjectFilter(String projectId) {
    _updateViewPrefsIfChanged(
      nextWorkbenchProjectFilterPrefs(
        prefs: state.viewPrefs,
        id: projectId,
        mutation: WorkbenchFilterMutation.add,
      ),
    );
  }

  void removeProjectFilter(String projectId) {
    _updateViewPrefsIfChanged(
      nextWorkbenchProjectFilterPrefs(
        prefs: state.viewPrefs,
        id: projectId,
        mutation: WorkbenchFilterMutation.remove,
      ),
    );
  }

  void clearProjectFilters() {
    _updateViewPrefsIfChanged(
      nextWorkbenchProjectFilterPrefs(
        prefs: state.viewPrefs,
        mutation: WorkbenchFilterMutation.clear,
      ),
    );
  }

  void toggleTagFilter(String tagId) {
    _updateViewPrefsIfChanged(
      nextWorkbenchTagFilterPrefs(
        prefs: state.viewPrefs,
        id: tagId,
        mutation: WorkbenchFilterMutation.toggle,
      ),
    );
  }

  void addTagFilter(String tagId) {
    _updateViewPrefsIfChanged(
      nextWorkbenchTagFilterPrefs(
        prefs: state.viewPrefs,
        id: tagId,
        mutation: WorkbenchFilterMutation.add,
      ),
    );
  }

  void removeTagFilter(String tagId) {
    _updateViewPrefsIfChanged(
      nextWorkbenchTagFilterPrefs(
        prefs: state.viewPrefs,
        id: tagId,
        mutation: WorkbenchFilterMutation.remove,
      ),
    );
  }

  void clearTagFilters() {
    _updateViewPrefsIfChanged(
      nextWorkbenchTagFilterPrefs(
        prefs: state.viewPrefs,
        mutation: WorkbenchFilterMutation.clear,
      ),
    );
  }

  void toggleParentWorkspaceCollapsed(String workspaceId) {
    _updateViewPrefs(
      toggleWorkbenchParentWorkspaceCollapsedPrefs(
        prefs: state.viewPrefs,
        workspaceId: workspaceId,
      ),
    );
  }

  void togglePinnedSectionCollapsed() {
    _updateViewPrefs(
      state.viewPrefs.copyWith(
        pinnedSectionCollapsed: !state.viewPrefs.pinnedSectionCollapsed,
      ),
    );
  }

  void toggleAllSectionCollapsed() {
    _updateViewPrefs(
      state.viewPrefs.copyWith(
        allSectionCollapsed: !state.viewPrefs.allSectionCollapsed,
      ),
    );
  }

  /// Collapses or expands every sidebar-visible grouping surface: project
  /// groups, parent workspace child trees, and workspace agent-run sections.
  void toggleCollapseAll() {
    final nextPrefs = nextWorkbenchCollapseAllPrefs(state);
    if (nextPrefs == null) {
      return;
    }
    _updateViewPrefs(nextPrefs);
  }

  void toggleWorkspaceExpanded(String workspaceId) {
    _updateViewPrefs(
      toggleWorkbenchWorkspaceExpandedPrefs(
        prefs: state.viewPrefs,
        workspaceId: workspaceId,
      ),
    );
  }

  void setWorkspaceExpanded(String workspaceId, bool expanded) {
    _updateViewPrefsIfChanged(
      nextWorkbenchWorkspaceExpandedPrefs(
        prefs: state.viewPrefs,
        workspaceId: workspaceId,
        expanded: expanded,
      ),
    );
  }

  void setRightSidebarVisible(bool visible) {
    if (state.viewPrefs.rightSidebarVisible == visible) {
      return;
    }
    _updateViewPrefs(state.viewPrefs.copyWith(rightSidebarVisible: visible));
  }

  void toggleRightSidebarVisible() {
    setRightSidebarVisible(!state.viewPrefs.rightSidebarVisible);
  }

  void setRightSidebarWidth(double value) {
    final clamped = value.clamp(
      AleraTokens.sidebarMinWidth,
      AleraTokens.sidebarMaxWidth,
    );
    if ((state.viewPrefs.rightSidebarWidth - clamped).abs() < 0.5) {
      return;
    }
    _updateViewPrefs(state.viewPrefs.copyWith(rightSidebarWidth: clamped));
  }

  void setContextPanelTab(WorkbenchContextPanelTab tab) {
    if (state.viewPrefs.activeContextPanelTab == tab) {
      return;
    }
    _updateViewPrefs(state.viewPrefs.copyWith(activeContextPanelTab: tab));
  }

  void revealInExplorer({
    required Workspace workspace,
    required String relativePath,
  }) {
    final normalized = normalizeWorkspaceRelativePath(relativePath);
    if (normalized == null) {
      return;
    }
    _workspaceExplorerReveal.reveal(
      workspaceId: workspace.id,
      relativePath: normalized,
    );
    setRightSidebarVisible(true);
    setContextPanelTab(.explorer);
  }

  void setExplorerMode(WorkspaceExplorerMode mode) {
    if (state.viewPrefs.explorerMode == mode) {
      return;
    }
    _updateViewPrefs(state.viewPrefs.copyWith(explorerMode: mode));
  }

  void setActiveContextPanelTab(WorkbenchContextPanelTab tab) {
    setContextPanelTab(tab);
  }

  void setGitDiffViewMode(GitDiffViewMode mode) {
    if (state.viewPrefs.gitDiffViewMode == mode) {
      return;
    }
    _updateViewPrefs(state.viewPrefs.copyWith(gitDiffViewMode: mode));
  }

  void setGitDiffGroupMode(GitDiffGroupMode mode) {
    if (state.viewPrefs.gitDiffGroupMode == mode) {
      return;
    }
    _updateViewPrefs(state.viewPrefs.copyWith(gitDiffGroupMode: mode));
  }

  void setGitDiffContentMode(GitDiffContentMode mode) {
    if (state.viewPrefs.gitDiffContentMode == mode) {
      return;
    }
    _updateViewPrefs(state.viewPrefs.copyWith(gitDiffContentMode: mode));
  }

  void setGitDiffPresentationMode(GitDiffPresentationMode mode) {
    if (state.viewPrefs.gitDiffPresentationMode == mode) {
      return;
    }
    _updateViewPrefs(state.viewPrefs.copyWith(gitDiffPresentationMode: mode));
  }

  void setGitDiffWhitespaceMode(String mode) {
    if (state.viewPrefs.gitDiffWhitespaceMode == mode) {
      return;
    }
    _updateViewPrefs(state.viewPrefs.copyWith(gitDiffWhitespaceMode: mode));
  }

  void setPullRequestCreateAction(PullRequestCreateAction action) {
    if (state.viewPrefs.pullRequestCreateAction == action) {
      return;
    }
    _updateViewPrefs(state.viewPrefs.copyWith(pullRequestCreateAction: action));
  }

  Future<bool> focusSourceControlFolder({
    required Workspace workspace,
    required String relativePath,
  }) => _selectionOwner.focusSourceControlFolder(
    workspace: workspace,
    relativePath: relativePath,
  );

  void clearFocusedSourceControlFolder({required Workspace workspace}) =>
      _selectionOwner.clearFocusedSourceControlFolder(workspace: workspace);

  void syncSourceControlRootAfterPathMove({
    required Workspace workspace,
    required String oldRelativePath,
    required String newRelativePath,
  }) {
    _updateViewPrefsIfChanged(
      syncWorkbenchSourceControlRootAfterPathMovePrefs(
        prefs: state.viewPrefs,
        workspaceId: workspace.id,
        oldRelativePath: oldRelativePath,
        newRelativePath: newRelativePath,
      ),
    );
  }

  void setSearchQuery(String query) => _selectionOwner.setSearchQuery(query);

  void setCollapsed(bool value) => _selectionOwner.setCollapsed(value);

  void setSidebarWidth(double value) {
    final clamped = value.clamp(
      AleraTokens.sidebarMinWidth,
      AleraTokens.sidebarMaxWidth,
    );
    if ((state.viewPrefs.sidebarWidth - clamped).abs() < 0.5) {
      return;
    }
    _updateViewPrefs(state.viewPrefs.copyWith(sidebarWidth: clamped));
  }
}
