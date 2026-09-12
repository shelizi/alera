part of 'workbench_controller.dart';

mixin _WorkbenchControllerSync
    on _$WorkbenchController, _WorkbenchControllerInternals {
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
      _retiredWorkspaceCleanup.cleanup(
        workspaceId: workspace.id,
        workspace: workspace,
        tabs: state.tabsFor(workspace.id),
      );
    }

    state = applyWorkbenchProjectSetSyncPlan(
      state: state,
      projects: projects,
      plan: plan,
    );
    _pruneWorktreeNavigationHistory();
    if (plan.viewPrefsChanged) {
      unawaited(_persistViewPrefs());
    }

    WorkbenchProjectWorkspaceSubscriptionCoordinator(
      workspaceSubscriptions: _workspaceSubscriptions,
      tabSubscriptions: _tabSubscriptions,
    ).sync(
      projects: projects,
      validProjectIds: validProjectIds,
      syncMetadataWatcher: _syncWorktreeMetadataWatcher,
      watchWorkspaces: _repository.watchWorkspaces,
      onWorkspacesChanged: _onWorkspacesChanged,
      ensureMainWorkspaceInBackground: (project) {
        unawaited(_ensureMainWorkspaceForProject(project));
      },
      forgetClearedLayouts: _tabLayoutOwner.clearedLayouts.forgetAll,
      pruneMetadataWatchers: _worktreeMetadataWatcherRegistry.prune,
    );
    _tabLayoutOwner.ensureSelectionHasTab();
  }

  void _onWorkspacesChanged(Project project, List<Workspace> workspaces) {
    final plan = planWorkbenchWorkspaceSetSync(
      state: state,
      project: project,
      workspaces: workspaces,
    );
    WorkbenchWorkspaceTabSubscriptionCoordinator(_tabSubscriptions).sync(
      projectId: project.id,
      workspaces: workspaces,
      cleanupRetiredWorkspace: (workspaceId) {
        _retiredWorkspaceCleanup.cleanup(
          workspaceId: workspaceId,
          workspace: _workspaceById(workspaceId),
          tabs: state.tabsFor(workspaceId),
        );
      },
      forgetClearedLayouts: _tabLayoutOwner.clearedLayouts.forgetAll,
      loadLayoutInBackground: _tabLayoutOwner.loadLayoutForWorkspace,
      watchTabs: _repository.watchWorkspaceTabs,
      onTabsChanged: _tabLayoutOwner.handleTabsChanged,
    );
    state = applyWorkbenchWorkspaceSetSyncPlan(state: state, plan: plan);
    _pruneWorktreeNavigationHistory();
    if (plan.viewPrefsChanged) {
      unawaited(_persistViewPrefs());
    }
    _tabLayoutOwner.ensureSelectionHasTab();
  }
}
