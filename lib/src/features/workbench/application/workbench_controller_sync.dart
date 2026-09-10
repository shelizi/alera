part of 'workbench_controller.dart';

mixin _WorkbenchControllerSync
    on _$WorkbenchController, _WorkbenchControllerInternals {
  /// Frees the live terminal handles and editor documents of a workspace that
  /// no longer exists in persisted state.
  ///
  /// The managed runtime stops PTYs before publishing removal. Releasing local
  /// handles also covers changes from another client without sending a second
  /// termination request for sessions this client no longer owns.
  void _releaseRetiredWorkspaceSessions(String workspaceId) {
    _tabFocusHistory.forget(workspaceId);
    _retiredResourceCleaner.releaseWorkspace(
      workspaceId,
      state.tabsFor(workspaceId),
    );
  }

  void _syncWorktreeMetadataWatcher(Project project) {
    _worktreeMetadataWatcherRegistry.sync(
      project,
      onRefresh: _refreshProjectWorktreesInBackground,
    );
  }

  Future<void> _refreshProjectWorktreesInBackground(String projectId) async {
    if (_disposed) {
      return;
    }
    final project = _projectById(state.projects, projectId);
    if (project == null || !project.supportsLinkedWorkspaces) {
      return;
    }
    try {
      await _workspaceService.reconcile(project);
    } catch (_) {
      // Watcher refresh is best-effort. Manual refresh reports errors to UI.
    }
  }

  void _onProjectsChanged(List<Project> projects) {
    final plan = planWorkbenchProjectSetSync(state: state, projects: projects);
    final validProjectIds = plan.validProjectIds;

    for (final workspace in plan.removedWorkspaces) {
      _hostedReviewRetention.releaseTabsInBackground(
        workspace,
        state.tabsFor(workspace.id),
      );
      _releaseRetiredWorkspaceSessions(workspace.id);
    }

    state = state.copyWith(
      projects: projects,
      workspacesByProject: plan.workspacesByProject,
      tabsByWorkspace: plan.tabsByWorkspace,
      viewPrefs: plan.viewPrefs,
      activeProjectId: plan.activeProjectId,
      activeWorkspaceId: plan.activeWorkspaceId,
      activeTabIdByWorkspace: plan.activeTabIdByWorkspace,
      layoutByWorkspace: plan.layoutByWorkspace,
    );
    _pruneWorktreeNavigationHistory();
    if (plan.viewPrefsChanged) {
      unawaited(_persistViewPrefs());
    }

    for (final project in projects) {
      _syncWorktreeMetadataWatcher(project);
      if (_workspaceSubscriptions.contains(project.id)) {
        continue;
      }
      _workspaceSubscriptions.watch(
        projectId: project.id,
        stream: _repository.watchWorkspaces(project.id),
        onData: (workspaces) => _onWorkspacesChanged(project, workspaces),
      );
      unawaited(_ensureMainWorkspaceForProject(project));
    }

    final removedProjectIds = _workspaceSubscriptions.projectIds
        .where((projectId) => !validProjectIds.contains(projectId))
        .toList(growable: false);
    for (final projectId in removedProjectIds) {
      _workspaceSubscriptions.cancelProject(projectId);
      final removedWorkspaceIds = _tabSubscriptions.workspaceIdsForProject(
        projectId,
      );
      for (final workspaceId in removedWorkspaceIds) {
        _tabSubscriptions.cancelWorkspace(workspaceId);
      }
      _clearedLayouts.forgetAll(removedWorkspaceIds);
    }
    _worktreeMetadataWatcherRegistry.prune(validProjectIds);
    _ensureSelectionHasTab();
  }

  void _onWorkspacesChanged(Project project, List<Workspace> workspaces) {
    final plan = planWorkbenchWorkspaceSetSync(
      state: state,
      project: project,
      workspaces: workspaces,
    );
    final liveWorkspaceIds = plan.liveWorkspaceIds;
    final removedWorkspaceIds = _tabSubscriptions
        .workspaceIdsForProject(project.id)
        .where((workspaceId) => !liveWorkspaceIds.contains(workspaceId))
        .toList(growable: false);
    for (final workspaceId in removedWorkspaceIds) {
      final workspace = _workspaceById(workspaceId);
      if (workspace != null) {
        _hostedReviewRetention.releaseTabsInBackground(
          workspace,
          state.tabsFor(workspaceId),
        );
      }
      _releaseRetiredWorkspaceSessions(workspaceId);
    }
    for (final workspaceId in removedWorkspaceIds) {
      _tabSubscriptions.cancelWorkspace(workspaceId);
    }
    _clearedLayouts.forgetAll(removedWorkspaceIds);
    for (final workspace in workspaces) {
      if (_tabSubscriptions.contains(workspace.id)) {
        continue;
      }
      unawaited(_loadLayoutForWorkspace(workspace.id));
      _tabSubscriptions.watch(
        projectId: project.id,
        workspaceId: workspace.id,
        stream: _repository.watchWorkspaceTabs(workspace.id),
        onData: (tabs) => _onTabsChanged(workspace.id, tabs),
      );
    }
    state = state.copyWith(
      workspacesByProject: plan.workspacesByProject,
      viewPrefs: plan.viewPrefs,
      activeProjectId: plan.activeProjectId,
      activeWorkspaceId: plan.activeWorkspaceId,
      layoutByWorkspace: plan.layoutByWorkspace,
      tabsByWorkspace: plan.tabsByWorkspace,
      activeTabIdByWorkspace: plan.activeTabIdByWorkspace,
    );
    _pruneWorktreeNavigationHistory();
    if (plan.viewPrefsChanged) {
      unawaited(_persistViewPrefs());
    }
    _ensureSelectionHasTab();
  }

  void _onTabsChanged(String workspaceId, List<WorkspaceTabRecord> tabs) {
    if (!_tabSubscriptions.contains(workspaceId)) {
      return;
    }
    final layoutWasCleared = _clearedLayouts.contains(workspaceId);
    final plan = planWorkbenchTabSetSync(
      state: state,
      workspaceId: workspaceId,
      tabs: tabs,
      layoutWasCleared: layoutWasCleared,
    );
    final removedTabs = plan.removedTabs;
    final workspace = _workspaceById(workspaceId);
    if (workspace != null) {
      _hostedReviewRetention.releaseTabsInBackground(workspace, removedTabs);
    }
    // A tab record that disappeared from persisted state can never reach its
    // live terminal handle again, so release the client-local terminal,
    // editor, and observer resources without terminating a PTY that another
    // client may still own.
    _retiredResourceCleaner.releaseTabs(removedTabs);
    if (tabs.isNotEmpty) {
      _clearedLayouts.forget(workspaceId);
    }
    state = state.copyWith(
      tabsByWorkspace: plan.tabsByWorkspace,
      layoutByWorkspace: plan.layoutByWorkspace,
      activeTabIdByWorkspace: plan.activeTabIdByWorkspace,
    );
    if (plan.shouldLoadLayout) {
      unawaited(_loadLayoutForWorkspace(workspaceId));
    }
    final layoutToPersist = plan.layoutToPersist;
    if (layoutToPersist != null) {
      _persistLayoutInBackground(layoutToPersist);
    }
    _ensureSelectionHasTab();
  }
}
