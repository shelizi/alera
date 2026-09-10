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
    ref.read(terminalRuntimeProvider).releaseWorkspace(workspaceId);
    final editorSessions = ref.read(editorSessionRegistryProvider);
    for (final tab in state.tabsFor(workspaceId)) {
      editorSessions.forget(tab.id);
      if (tab.kind == WorkspaceTabKind.terminal &&
          ref.exists(agentHookReceiverProvider)) {
        ref
            .read(agentHookReceiverProvider)
            .clearTerminalSession(tab.terminalSessionId);
      }
    }
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
      _releaseHostedReviewTabsInBackground(
        workspace,
        state.tabsFor(workspace.id),
      );
      _releaseRetiredWorkspaceSessions(workspace.id);
    }

    final updatedWorkspaces = plan.workspacesByProject;
    final updatedTabs = plan.tabsByWorkspace;
    final updatedLayouts = plan.layoutByWorkspace;
    final updatedActiveTabs = plan.activeTabIdByWorkspace;
    final prunedViewPrefs = plan.viewPrefs;
    final prefsChanged = plan.viewPrefsChanged;

    final activeProjectId =
        state.activeProjectId != null &&
            validProjectIds.contains(state.activeProjectId)
        ? state.activeProjectId
        : (projects.isNotEmpty ? projects.first.id : null);
    final activeWorkspaceId = _resolveActiveWorkspaceId(
      activeProjectId: activeProjectId,
      workspacesByProject: updatedWorkspaces,
      preferredWorkspaceId: state.activeWorkspaceId,
    );
    final nextViewPrefs = prunedViewPrefs;
    final viewPrefsChanged = prefsChanged;

    state = state.copyWith(
      projects: projects,
      workspacesByProject: updatedWorkspaces,
      tabsByWorkspace: updatedTabs,
      viewPrefs: nextViewPrefs,
      activeProjectId: activeProjectId,
      activeWorkspaceId: activeWorkspaceId,
      activeTabIdByWorkspace: updatedActiveTabs,
      layoutByWorkspace: updatedLayouts,
    );
    _pruneWorktreeNavigationHistory();
    if (viewPrefsChanged) {
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
        _releaseHostedReviewTabsInBackground(
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
    final liveTabIds = <String>{for (final tab in tabs) tab.id};
    final removedTabs = state
        .tabsFor(workspaceId)
        .where((tab) => !liveTabIds.contains(tab.id));
    final workspace = _workspaceById(workspaceId);
    if (workspace != null) {
      _releaseHostedReviewTabsInBackground(workspace, removedTabs);
    }
    // A tab record that disappeared from persisted state can never reach its
    // live terminal handle again, so the emulator buffer and the editor
    // document have to go now. Release rather than close: the PTY may still
    // belong to whichever client removed the record.
    final runtime = ref.read(terminalRuntimeProvider);
    final editorSessions = ref.read(editorSessionRegistryProvider);
    for (final tab in removedTabs) {
      runtime.releaseTab(tab.id);
      editorSessions.forget(tab.id);
      if (tab.kind == WorkspaceTabKind.terminal &&
          ref.exists(agentHookReceiverProvider)) {
        // The host may already have stopped the process before the explicit
        // close reaches this client. Its transcript poller still has to go.
        ref
            .read(agentHookReceiverProvider)
            .clearTerminalSession(tab.terminalSessionId);
      }
    }
    final nextTabs = Map<String, List<WorkspaceTabRecord>>.from(
      state.tabsByWorkspace,
    )..[workspaceId] = tabs;
    if (tabs.isEmpty && _workspaceIdsWithClearedLayout.contains(workspaceId)) {
      final nextLayouts = Map<String, WorkbenchLayout>.from(
        state.layoutByWorkspace,
      )..remove(workspaceId);
      final activeTabs = Map<String, String>.from(state.activeTabIdByWorkspace)
        ..remove(workspaceId);
      state = state.copyWith(
        tabsByWorkspace: nextTabs,
        layoutByWorkspace: nextLayouts,
        activeTabIdByWorkspace: activeTabs,
      );
      _ensureSelectionHasTab();
      return;
    }
    if (tabs.isNotEmpty) {
      _workspaceIdsWithClearedLayout.remove(workspaceId);
    }
    final currentLayout = state.layoutFor(workspaceId);
    if (currentLayout == null) {
      state = state.copyWith(tabsByWorkspace: nextTabs);
      if (!_loadingLayoutWorkspaceIds.contains(workspaceId)) {
        unawaited(_loadLayoutForWorkspace(workspaceId));
      }
      _ensureSelectionHasTab();
      return;
    }

    final layout = currentLayout.sanitize(tabs);
    final nextLayouts = Map<String, WorkbenchLayout>.from(
      state.layoutByWorkspace,
    )..[workspaceId] = layout;
    final activeTabs = _activeTabsWithLayout(layout);
    state = state.copyWith(
      tabsByWorkspace: nextTabs,
      layoutByWorkspace: nextLayouts,
      activeTabIdByWorkspace: activeTabs,
    );
    if (layout != currentLayout) {
      _persistLayoutInBackground(layout);
    }
    _ensureSelectionHasTab();
  }
}
