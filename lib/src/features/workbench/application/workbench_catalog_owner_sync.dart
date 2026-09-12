part of 'workbench_catalog_owner.dart';

/// Watcher-driven catalog sync: repository watchers push project and
/// workspace snapshots that become sync plans applied to the state.
extension WorkbenchCatalogOwnerSync on WorkbenchCatalogOwner {
  void _syncWorktreeMetadataWatcher(Project project) {
    _host.worktreeMetadataWatcherRegistry.sync(
      project,
      onRefresh: _refreshProjectWorktreesInBackground,
    );
  }

  Future<void> _refreshProjectWorktreesInBackground(String projectId) async {
    if (_host.isDisposed) {
      return;
    }
    final project = _projectById(_host.readState().projects, projectId);
    if (project == null || !project.supportsLinkedWorkspaces) {
      return;
    }
    try {
      await _host.workspaceService.reconcile(project);
    } catch (_) {
      // Watcher refresh is best-effort. Manual refresh reports errors to UI.
    }
  }

  void _onProjectsChanged(List<Project> projects) {
    final plan = planWorkbenchProjectSetSync(
      state: _host.readState(),
      projects: projects,
    );
    final validProjectIds = plan.validProjectIds;

    for (final workspace in plan.removedWorkspaces) {
      _host.retiredWorkspaceCleanup.cleanup(
        workspaceId: workspace.id,
        workspace: workspace,
        tabs: _host.readState().tabsFor(workspace.id),
      );
    }

    _host.emitState(
      applyWorkbenchProjectSetSyncPlan(
        state: _host.readState(),
        projects: projects,
        plan: plan,
      ),
    );
    _host.pruneWorktreeNavigationHistory();
    if (plan.viewPrefsChanged) {
      _host.persistViewPrefs();
    }

    WorkbenchProjectWorkspaceSubscriptionCoordinator(
      workspaceSubscriptions: _host.workspaceSubscriptions,
      tabSubscriptions: _host.tabSubscriptions,
    ).sync(
      projects: projects,
      validProjectIds: validProjectIds,
      syncMetadataWatcher: _syncWorktreeMetadataWatcher,
      watchWorkspaces: _host.workbenchRepository.watchWorkspaces,
      onWorkspacesChanged: _onWorkspacesChanged,
      ensureMainWorkspaceInBackground: (project) {
        unawaited(_ensureMainWorkspaceForProject(project));
      },
      forgetClearedLayouts: _host.clearedLayouts.forgetAll,
      pruneMetadataWatchers: _host.worktreeMetadataWatcherRegistry.prune,
    );
    _host.ensureSelectionHasTab();
  }

  void _onWorkspacesChanged(Project project, List<Workspace> workspaces) {
    final plan = planWorkbenchWorkspaceSetSync(
      state: _host.readState(),
      project: project,
      workspaces: workspaces,
    );
    WorkbenchWorkspaceTabSubscriptionCoordinator(_host.tabSubscriptions).sync(
      projectId: project.id,
      workspaces: workspaces,
      cleanupRetiredWorkspace: (workspaceId) {
        _host.retiredWorkspaceCleanup.cleanup(
          workspaceId: workspaceId,
          workspace: _host.workspaceById(workspaceId),
          tabs: _host.readState().tabsFor(workspaceId),
        );
      },
      forgetClearedLayouts: _host.clearedLayouts.forgetAll,
      loadLayoutInBackground: _host.loadLayoutForWorkspace,
      watchTabs: _host.workbenchRepository.watchWorkspaceTabs,
      onTabsChanged: _host.handleWorkspaceTabsChanged,
    );
    _host.emitState(
      applyWorkbenchWorkspaceSetSyncPlan(state: _host.readState(), plan: plan),
    );
    _host.pruneWorktreeNavigationHistory();
    if (plan.viewPrefsChanged) {
      _host.persistViewPrefs();
    }
    _host.ensureSelectionHasTab();
  }
}
