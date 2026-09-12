part of 'workbench_catalog_owner.dart';

/// Project and workspace catalog mutations: add, clone, rename and remove
/// projects plus workspace rename, pin, parent and tag updates, sleep and
/// delete.
extension WorkbenchCatalogOwnerProjects on WorkbenchCatalogOwner {
  Future<List<String>> listSourceBranches(Project project) =>
      _host.workspaceService.listSourceBranches(project);

  Future<void> reconcileProjectWorkspaces(String projectId) async {
    final project = _projectById(_host.readState().projects, projectId);
    if (project == null) return;
    try {
      await _host.workspaceService.reconcile(project);
      _host.emitState(_host.readState().copyWith(error: null));
    } catch (error) {
      _host.emitState(_host.readState().copyWith(error: error.toString()));
      rethrow;
    }
  }

  Future<Project> addLocalProject({required String path, String? name}) async {
    try {
      final project = await _host.projectsService.addLocalProject(
        path: path,
        name: name,
      );
      await _activateAddedProject(project);
      return project;
    } catch (error) {
      _host.emitState(_host.readState().copyWith(error: error.toString()));
      rethrow;
    }
  }

  Future<Project> cloneProject({
    required String gitUrl,
    required String destinationPath,
    String? name,
  }) async {
    try {
      final project = await _host.projectsService.cloneProject(
        gitUrl: gitUrl,
        destinationPath: destinationPath,
        name: name,
      );
      await _activateAddedProject(project);
      return project;
    } catch (error) {
      _host.emitState(_host.readState().copyWith(error: error.toString()));
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
      final project = await _host.projectsService.renameProject(
        projectId: projectId,
        name: name,
      );
      _host.emitState(
        applyWorkbenchProjectUpdateState(
          state: _host.readState(),
          project: project,
        ),
      );
    } catch (error) {
      _host.emitState(_host.readState().copyWith(error: error.toString()));
      rethrow;
    }
  }

  Future<void> sleepWorkspace(Workspace workspace) async {
    await _host.workspaceTabClosingScope.run(workspace.id, () async {
      try {
        final workspaceTabs = _host.readState().tabsFor(workspace.id);
        await WorkbenchSleepWorkspaceCoordinator(
          tabRemoval: _host.workbenchRepository,
          hostedReviewRetention: _host.hostedReviewRetention,
          clearedLayouts: _host.clearedLayouts,
          tabFocusHistory: _host.tabFocusHistory,
        ).sleep(workspace: workspace, tabs: workspaceTabs);
        _host.explicitResourceCleaner.closeWorkspaceLocalResources(
          workspace.id,
          workspaceTabs,
        );

        _host.emitState(
          applyWorkbenchSleepWorkspaceState(
            state: _host.readState(),
            workspaceId: workspace.id,
          ),
        );
      } catch (error) {
        _host.emitState(_host.readState().copyWith(error: error.toString()));
        rethrow;
      }
    });
  }

  Future<void> removeProject(String projectId) async {
    try {
      final state = _host.readState();
      final removedWorkspaces = <WorkbenchRemovedProjectWorkspace>[
        for (final workspace in state.workspacesFor(projectId))
          WorkbenchRemovedProjectWorkspace(
            workspace: workspace,
            tabs: List<WorkspaceTabRecord>.from(state.tabsFor(workspace.id)),
          ),
      ];
      await WorkbenchRemoveProjectCleanupCoordinator(
        closeLocalWorkspace: _host.removedProjectWorkspaceCloser,
        hostedReviewRetention: _host.hostedReviewRetention,
        tabFocusHistory: _host.tabFocusHistory,
      ).remove(
        removeProject: () => _host.projectsService.removeProject(projectId),
        removedWorkspaces: removedWorkspaces,
      );
      _host.emitState(_host.readState().copyWith(error: null));
    } catch (error) {
      _host.emitState(_host.readState().copyWith(error: error.toString()));
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
      final workspaceTabs = _host.readState().tabsFor(workspace.id);
      await _withWorktreeRefreshSuspended(
        project.id,
        () => _host.workspaceService.removeWorkspace(
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
        resourceCleaner: _host.explicitResourceCleaner,
        hostedReviewRetention: _host.hostedReviewRetention,
        tabFocusHistory: _host.tabFocusHistory,
      ).cleanup(
        workspace: workspace,
        tabs: workspaceTabs,
        fallbackWorkspacePath: project.repoPath,
      );
      _host.emitState(_host.readState().copyWith(error: null));
    } catch (error) {
      _host.emitState(_host.readState().copyWith(error: error.toString()));
      rethrow;
    }
  }

  Future<void> renameWorkspace({
    required String workspaceId,
    required String name,
  }) async {
    try {
      final workspace = await _host.workspaceService.renameWorkspace(
        workspaceId: workspaceId,
        name: name,
      );
      _host.emitState(
        applyWorkbenchWorkspaceUpdateState(
          state: _host.readState(),
          workspace: workspace,
        ).copyWith(error: null),
      );
    } catch (error) {
      _host.emitState(_host.readState().copyWith(error: error.toString()));
      rethrow;
    }
  }

  Future<void> setWorkspacePinned({
    required String workspaceId,
    required bool isPinned,
  }) async {
    try {
      final workspace = await _host.workbenchRepository.setWorkspacePinned(
        workspaceId,
        isPinned,
      );
      _host.emitState(
        applyWorkbenchWorkspaceUpdateState(
          state: _host.readState(),
          workspace: workspace,
        ).copyWith(error: null),
      );
    } catch (error) {
      _host.emitState(_host.readState().copyWith(error: error.toString()));
      rethrow;
    }
  }

  /// Pins or unpins [workspaceId] and every descendant of [workspaceId].
  /// No-ops workspaces that already match [isPinned].
  Future<void> setWorkspaceTreePinned({
    required String workspaceId,
    required bool isPinned,
  }) {
    return _workspaceTreePinMutations.run<void>(
      () =>
          WorkbenchWorkspaceTreePinCoordinator(
            setPinned: ({required workspaceId, required isPinned}) =>
                setWorkspacePinned(
                  workspaceId: workspaceId,
                  isPinned: isPinned,
                ),
          ).run(
            state: _host.readState(),
            workspaceId: workspaceId,
            isPinned: isPinned,
          ),
    );
  }

  Future<List<WorkspaceTag>> listWorkspaceTags() async {
    try {
      final tags = await _host.workspaceGraphRepository.listTags();
      _host.emitState(_host.readState().copyWith(error: null));
      return tags;
    } catch (error) {
      _host.emitState(_host.readState().copyWith(error: error.toString()));
      rethrow;
    }
  }

  Future<List<WorkspaceRelation>> listWorkspaceRelations() async {
    try {
      final relations = await _host.workspaceGraphRepository.listRelations();
      _host.emitState(_host.readState().copyWith(error: null));
      return relations;
    } catch (error) {
      _host.emitState(_host.readState().copyWith(error: error.toString()));
      rethrow;
    }
  }

  Future<WorkspaceTag> createWorkspaceTag(String name) async {
    try {
      final tag = await WorkbenchWorkspaceTagCreationService(
        _host.workspaceGraphRepository,
      ).create(name);
      _host.emitState(_host.readState().copyWith(error: null));
      return tag;
    } catch (error) {
      _host.emitState(_host.readState().copyWith(error: error.toString()));
      rethrow;
    }
  }

  Future<void> deleteWorkspaceTag(String tagId) async {
    try {
      await _host.workspaceGraphRepository.removeTag(tagId);
      _host.emitState(_host.readState().copyWith(error: null));
    } catch (error) {
      _host.emitState(_host.readState().copyWith(error: error.toString()));
      rethrow;
    }
  }

  Future<void> updateWorkspaceTags({
    required Workspace workspace,
    required Set<String> tagIds,
  }) {
    final wasTracked = _host.workspaceById(workspace.id) != null;
    final requestedTagIds = normalizeWorkbenchWorkspaceTagIds(tagIds);
    return _workspaceTagMutations.run<void>(
      workspaceId: workspace.id,
      action: () async {
        final latest = _host.workspaceById(workspace.id);
        if (wasTracked && latest == null) {
          return;
        }
        final basis = latest ?? workspace;
        final plan = planWorkbenchWorkspaceTagUpdate(
          state: _host.readState(),
          workspace: basis,
          requestedTagIds: requestedTagIds,
        );
        try {
          await WorkbenchWorkspaceTagUpdateService(
            _host.workspaceGraphRepository,
          ).apply(workspaceId: workspace.id, plan: plan);
          final latestAfter = _host.workspaceById(workspace.id);
          if (latestAfter != null) {
            _host.emitState(
              applyWorkbenchWorkspaceUpdateState(
                state: _host.readState(),
                workspace: latestAfter.copyWith(
                  tagIds: requestedTagIds.toList(growable: false),
                ),
              ),
            );
          }
          _host.emitState(_host.readState().copyWith(error: null));
        } catch (error) {
          _host.emitState(_host.readState().copyWith(error: error.toString()));
          rethrow;
        }
      },
    );
  }

  Future<void> setWorkspaceParent({
    required Workspace workspace,
    String? parentWorkspaceId,
  }) {
    final wasTracked = _host.workspaceById(workspace.id) != null;
    final requestedParentId = normalizeWorkbenchWorkspaceParentId(
      parentWorkspaceId,
    );
    return _workspaceParentMutations.run<void>(
      workspaceId: workspace.id,
      action: () async {
        final latest = _host.workspaceById(workspace.id);
        if (wasTracked && latest == null) {
          return;
        }
        final basis = latest ?? workspace;
        try {
          final changed =
              await WorkbenchWorkspaceParentUpdateService(
                _host.workspaceGraphRepository,
              ).update(
                workspace: basis,
                parentWorkspaceId: requestedParentId,
                workspaceById: _host.workspaceById,
              );
          if (!changed) return;
          final latestAfter = _host.workspaceById(workspace.id);
          if (latestAfter != null) {
            _host.emitState(
              applyWorkbenchWorkspaceUpdateState(
                state: _host.readState(),
                workspace: latestAfter.copyWith(
                  parentWorkspaceId: requestedParentId,
                ),
              ),
            );
          }
          _host.emitState(_host.readState().copyWith(error: null));
        } catch (error) {
          _host.emitState(_host.readState().copyWith(error: error.toString()));
          rethrow;
        }
      },
    );
  }
}
