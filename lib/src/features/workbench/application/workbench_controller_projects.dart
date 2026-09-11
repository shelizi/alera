part of 'workbench_controller.dart';

mixin _WorkbenchControllerProjects
    on
        _$WorkbenchController,
        _WorkbenchControllerInternals,
        _WorkbenchControllerTabOpening {
  Future<List<String>> listSourceBranches(Project project) =>
      _workspaceService.listSourceBranches(project);

  Future<void> reconcileProjectWorkspaces(String projectId) async {
    final project = _projectById(state.projects, projectId);
    if (project == null) return;
    try {
      await _workspaceService.reconcile(project);
      state = state.copyWith(error: null);
    } catch (error) {
      state = state.copyWith(error: error.toString());
      rethrow;
    }
  }

  Future<Project> addLocalProject({required String path, String? name}) async {
    try {
      final project = await _projectsService.addLocalProject(
        path: path,
        name: name,
      );
      await _activateAddedProject(project);
      return project;
    } catch (error) {
      state = state.copyWith(error: error.toString());
      rethrow;
    }
  }

  Future<Project> cloneProject({
    required String gitUrl,
    required String destinationPath,
    String? name,
  }) async {
    try {
      final project = await _projectsService.cloneProject(
        gitUrl: gitUrl,
        destinationPath: destinationPath,
        name: name,
      );
      await _activateAddedProject(project);
      return project;
    } catch (error) {
      state = state.copyWith(error: error.toString());
      rethrow;
    }
  }

  Future<Project> addProject({required String repoPath, String? name}) =>
      addLocalProject(path: repoPath, name: name);

  Future<void> renameProject({
    required String projectId,
    required String name,
  }) async {
    try {
      final project = await _projectsService.renameProject(
        projectId: projectId,
        name: name,
      );
      state = applyWorkbenchProjectUpdateState(state: state, project: project);
    } catch (error) {
      state = state.copyWith(error: error.toString());
      rethrow;
    }
  }

  Future<void> sleepWorkspace(Workspace workspace) async {
    await _tabClosingScope.run(workspace.id, () async {
      try {
        final workspaceTabs = state.tabsFor(workspace.id);
        await WorkbenchSleepWorkspaceCoordinator(
          tabRemoval: _repository,
          hostedReviewRetention: _hostedReviewRetention,
          clearedLayouts: _clearedLayouts,
          tabFocusHistory: _tabFocusHistory,
        ).sleep(workspace: workspace, tabs: workspaceTabs);
        _explicitResourceCleaner.closeWorkspaceLocalResources(
          workspace.id,
          workspaceTabs,
        );

        state = applyWorkbenchSleepWorkspaceState(
          state: state,
          workspaceId: workspace.id,
        );
      } catch (error) {
        state = state.copyWith(error: error.toString());
        rethrow;
      }
    });
  }

  Future<void> removeProject(String projectId) async {
    try {
      final removedWorkspaces = <WorkbenchRemovedProjectWorkspace>[
        for (final workspace in state.workspacesFor(projectId))
          WorkbenchRemovedProjectWorkspace(
            workspace: workspace,
            tabs: List<WorkspaceTabRecord>.from(state.tabsFor(workspace.id)),
          ),
      ];
      await WorkbenchRemoveProjectCleanupCoordinator(
        closeLocalWorkspace: _closeRemovedProjectWorkspace,
        hostedReviewRetention: _hostedReviewRetention,
        tabFocusHistory: _tabFocusHistory,
      ).remove(
        removeProject: () => _projectsService.removeProject(projectId),
        removedWorkspaces: removedWorkspaces,
      );
      state = state.copyWith(error: null);
    } catch (error) {
      state = state.copyWith(error: error.toString());
      rethrow;
    }
  }

  Future<void> deleteWorkspace({
    required Project project,
    required Workspace workspace,
    bool deleteBranch = true,
    String? activeWorkspaceId,
  }) async {
    try {
      final workspaceTabs = state.tabsFor(workspace.id);
      await _withWorktreeRefreshSuspended(
        project.id,
        () => _workspaceService.removeWorkspace(
          project: project,
          workspace: workspace,
          deleteBranch: deleteBranch,
          activeWorkspaceId: activeWorkspaceId,
        ),
      );
      // The managed runtime has already stopped the process trees. Preserve the
      // local -> hosted-review -> observer cleanup failure boundary in one
      // application coordinator.
      await WorkbenchDeleteWorkspaceCleanupCoordinator(
        resourceCleaner: _explicitResourceCleaner,
        hostedReviewRetention: _hostedReviewRetention,
        tabFocusHistory: _tabFocusHistory,
      ).cleanup(
        workspace: workspace,
        tabs: workspaceTabs,
        fallbackWorkspacePath: project.repoPath,
      );
      state = state.copyWith(error: null);
    } catch (error) {
      state = state.copyWith(error: error.toString());
      rethrow;
    }
  }

  Future<void> renameWorkspace({
    required String workspaceId,
    required String name,
  }) async {
    try {
      final workspace = await _workspaceService.renameWorkspace(
        workspaceId: workspaceId,
        name: name,
      );
      state = applyWorkbenchWorkspaceUpdateState(
        state: state,
        workspace: workspace,
      ).copyWith(error: null);
    } catch (error) {
      state = state.copyWith(error: error.toString());
      rethrow;
    }
  }

  Future<void> setWorkspacePinned({
    required String workspaceId,
    required bool isPinned,
  }) async {
    try {
      final workspace = await _repository.setWorkspacePinned(
        workspaceId,
        isPinned,
      );
      state = applyWorkbenchWorkspaceUpdateState(
        state: state,
        workspace: workspace,
      ).copyWith(error: null);
    } catch (error) {
      state = state.copyWith(error: error.toString());
      rethrow;
    }
  }

  /// Pins or unpins [workspaceId] and every descendant of [workspaceId].
  /// No-ops workspaces that already match [isPinned].
  Future<void> setWorkspaceTreePinned({
    required String workspaceId,
    required bool isPinned,
  }) async {
    await WorkbenchWorkspaceTreePinCoordinator(
      setPinned: ({required workspaceId, required isPinned}) =>
          setWorkspacePinned(workspaceId: workspaceId, isPinned: isPinned),
    ).run(state: state, workspaceId: workspaceId, isPinned: isPinned);
  }

  Future<List<WorkspaceTag>> listWorkspaceTags() async {
    try {
      final tags = await _workspaceGraphRepository.listTags();
      state = state.copyWith(error: null);
      return tags;
    } catch (error) {
      state = state.copyWith(error: error.toString());
      rethrow;
    }
  }

  Future<List<WorkspaceRelation>> listWorkspaceRelations() async {
    try {
      final relations = await _workspaceGraphRepository.listRelations();
      state = state.copyWith(error: null);
      return relations;
    } catch (error) {
      state = state.copyWith(error: error.toString());
      rethrow;
    }
  }

  Future<WorkspaceTag> createWorkspaceTag(String name) async {
    try {
      final tag = await WorkbenchWorkspaceTagCreationService(
        _workspaceGraphRepository,
      ).create(name);
      state = state.copyWith(error: null);
      return tag;
    } catch (error) {
      state = state.copyWith(error: error.toString());
      rethrow;
    }
  }

  Future<void> deleteWorkspaceTag(String tagId) async {
    try {
      await _workspaceGraphRepository.removeTag(tagId);
      state = state.copyWith(error: null);
    } catch (error) {
      state = state.copyWith(error: error.toString());
      rethrow;
    }
  }

  Future<void> updateWorkspaceTags({
    required Workspace workspace,
    required Set<String> tagIds,
  }) async {
    final plan = planWorkbenchWorkspaceTagUpdate(
      state: state,
      workspace: workspace,
      requestedTagIds: tagIds,
    );
    try {
      await WorkbenchWorkspaceTagUpdateService(_workspaceGraphRepository)
          .apply(workspaceId: workspace.id, plan: plan);
      state = state.copyWith(error: null);
    } catch (error) {
      state = state.copyWith(error: error.toString());
      rethrow;
    }
  }

  Future<void> setWorkspaceParent({
    required Workspace workspace,
    String? parentWorkspaceId,
  }) async {
    try {
      final changed = await WorkbenchWorkspaceParentUpdateService(
        _workspaceGraphRepository,
      ).update(workspace: workspace, parentWorkspaceId: parentWorkspaceId);
      if (!changed) return;
      state = state.copyWith(error: null);
    } catch (error) {
      state = state.copyWith(error: error.toString());
      rethrow;
    }
  }

  Future<void> selectWorkspace({
    required Project project,
    required Workspace workspace,
  }) {
    return _selectWorkspace(
      project: project,
      workspace: workspace,
      ensureInitialTerminal: true,
    );
  }

  Future<void> _selectWorkspace({
    required Project project,
    required Workspace workspace,
    required bool ensureInitialTerminal,
    bool recordHistory = true,
  }) =>
      WorkbenchWorkspaceSelectionCoordinator(
        activateSelection: () {
          state = selectWorkbenchWorkspace(
            state: state,
            project: project,
            workspace: workspace,
          );
        },
        hydrate: ({required workspaceId, required ensureInitialTerminal}) =>
            WorkbenchWorkspaceSelectionHydrator(
              tabStore: _workspaceTabService,
              layoutResolver: _layoutResolver,
            ).hydrate(
              workspaceId: workspaceId,
              ensureInitialTerminal: ensureInitialTerminal,
            ),
        applyTabs: (tabs) => _setTabsForWorkspace(workspace.id, tabs),
        applyLayout: (layout) => _applyLayout(layout, persist: false),
        recordHistory: () =>
            _navigationHistory.record(project: project, workspace: workspace),
        notifyHistoryChanged: _notifyNavigationHistoryChanged,
      ).select(
        workspaceId: workspace.id,
        ensureInitialTerminal: ensureInitialTerminal,
        shouldRecordHistory: recordHistory,
      );

  Future<void> activateProject(Project project) async {
    state = activateWorkbenchProject(state: state, project: project);
  }
}
