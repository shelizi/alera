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
    final existing = _worktreeMetadataWatchers[project.id];
    if (!project.supportsLinkedWorkspaces) {
      if (existing != null) {
        _worktreeMetadataWatchers.remove(project.id);
        unawaited(existing.dispose());
      }
      return;
    }
    if (existing != null && p.equals(existing.repoPath, project.repoPath)) {
      return;
    }
    if (existing != null) {
      _worktreeMetadataWatchers.remove(project.id);
      unawaited(existing.dispose());
    }

    final watcher = GitWorktreeMetadataWatcher(
      repoPath: project.repoPath,
      onRefresh: () => _refreshProjectWorktreesInBackground(project.id),
    );
    _worktreeMetadataWatchers[project.id] = watcher;
    watcher.start();
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
      if (_workspaceSubs.containsKey(project.id)) {
        continue;
      }
      _workspaceSubs[project.id] = _repository
          .watchWorkspaces(project.id)
          .listen(
            (workspaces) => _onWorkspacesChanged(project, workspaces),
            // Re-subscription is guarded by `containsKey`, so a subscription
            // that dies must drop out of the map or the project stops syncing
            // for the rest of the session.
            onError: (Object _) {},
            onDone: () => _workspaceSubs.remove(project.id),
            cancelOnError: false,
          );
      unawaited(_ensureMainWorkspaceForProject(project));
    }

    final removedProjectIds = _workspaceSubs.keys
        .where((projectId) => !validProjectIds.contains(projectId))
        .toList(growable: false);
    for (final projectId in removedProjectIds) {
      _workspaceSubs.remove(projectId)?.cancel();
      final removedWorkspaceIds = _tabSubProjectIds.entries
          .where((entry) => entry.value == projectId)
          .map((entry) => entry.key)
          .toList(growable: false);
      for (final workspaceId in removedWorkspaceIds) {
        _tabSubs.remove(workspaceId)?.cancel();
        _tabSubProjectIds.remove(workspaceId);
      }
      _workspaceIdsWithClearedLayout.removeAll(removedWorkspaceIds);
    }
    final removedWatcherProjectIds = _worktreeMetadataWatchers.keys
        .where((projectId) => !validProjectIds.contains(projectId))
        .toList(growable: false);
    for (final projectId in removedWatcherProjectIds) {
      final watcher = _worktreeMetadataWatchers.remove(projectId);
      if (watcher != null) {
        unawaited(watcher.dispose());
      }
    }
    _ensureSelectionHasTab();
  }

  void _onWorkspacesChanged(Project project, List<Workspace> workspaces) {
    final plan = planWorkbenchWorkspaceSetSync(
      state: state,
      project: project,
      workspaces: workspaces,
    );
    final liveWorkspaceIds = plan.liveWorkspaceIds;
    final removedWorkspaceIds = _tabSubProjectIds.entries
        .where(
          (entry) =>
              entry.value == project.id &&
              !liveWorkspaceIds.contains(entry.key),
        )
        .map((entry) => entry.key)
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
      _tabSubs.remove(workspaceId)?.cancel();
      _tabSubProjectIds.remove(workspaceId);
    }
    _workspaceIdsWithClearedLayout.removeAll(removedWorkspaceIds);
    for (final workspace in workspaces) {
      if (_tabSubs.containsKey(workspace.id)) {
        continue;
      }
      _tabSubProjectIds[workspace.id] = project.id;
      unawaited(_loadLayoutForWorkspace(workspace.id));
      _tabSubs[workspace.id] = _repository
          .watchWorkspaceTabs(workspace.id)
          .listen(
            (tabs) => _onTabsChanged(workspace.id, tabs),
            onError: (Object _) {},
            onDone: () {
              _tabSubs.remove(workspace.id);
              _tabSubProjectIds.remove(workspace.id);
            },
            cancelOnError: false,
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
    if (!_tabSubProjectIds.containsKey(workspaceId)) {
      return;
    }
    final layoutWasCleared = _workspaceIdsWithClearedLayout.contains(
      workspaceId,
    );
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
      _workspaceIdsWithClearedLayout.remove(workspaceId);
    }
    state = state.copyWith(
      tabsByWorkspace: plan.tabsByWorkspace,
      layoutByWorkspace: plan.layoutByWorkspace,
      activeTabIdByWorkspace: plan.activeTabIdByWorkspace,
    );
    if (plan.shouldLoadLayout &&
        !_loadingLayoutWorkspaceIds.contains(workspaceId)) {
      unawaited(_loadLayoutForWorkspace(workspaceId));
    }
    final layoutToPersist = plan.layoutToPersist;
    if (layoutToPersist != null) {
      _persistLayoutInBackground(layoutToPersist);
    }
    _ensureSelectionHasTab();
  }
}
