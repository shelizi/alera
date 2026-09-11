part of 'workbench_controller.dart';

mixin _WorkbenchControllerInternals on _$WorkbenchController {
  final Uuid _uuid = const Uuid();
  bool _disposed = false;

  ProjectsService get _projectsService => ref.read(projectsServiceProvider);

  WorkbenchRepository get _repository => ref.read(workbenchRepositoryProvider);

  WorkspaceGraphRepository get _workspaceGraphRepository =>
      ref.read(workspaceGraphRepositoryProvider);

  WorkspaceService get _workspaceService => ref.read(workspaceServiceProvider);

  WorkbenchRemovedProjectWorkspaceCloser get _closeRemovedProjectWorkspace =>
      ref.read(terminalRuntimeLifecycleProvider).closeWorkspace;

  WorkbenchSequencedLayoutRepository? _layoutRepository;

  WorkbenchSequencedLayoutRepository get _sequencedLayoutRepository =>
      _layoutRepository ??= WorkbenchSequencedLayoutRepository(_repository);

  WorkbenchLayoutResolver get _layoutResolver =>
      WorkbenchLayoutResolver(_sequencedLayoutRepository);

  Future<T> _withWorktreeRefreshSuspended<T>(
    String projectId,
    Future<T> Function() action,
  ) {
    return _worktreeMetadataWatcherRegistry.withRefreshSuspended(
      projectId,
      action,
    );
  }

  WorkspaceTabService get _workspaceTabService =>
      ref.read(workspaceTabServiceProvider);
  WorkbenchReplaceableTabEditorSessions get _replaceableTabEditorSessions =>
      ref.read(editorSessionRegistryProvider);
  WorkbenchWorkspaceExplorerRevealSink get _workspaceExplorerReveal =>
      ref.read(workspaceExplorerRevealControllerProvider.notifier);

  WorkbenchWorkspaceActivityRecorder get _workspaceActivityRecorder =>
      ref.read(workspaceActivityControllerProvider.notifier);

  WorkbenchHostedReviewRetentionService get _hostedReviewRetention =>
      WorkbenchHostedReviewRetentionService(
        gitBackend: ref.read(gitBackendProvider),
      );
  WorkbenchGitRepositoryProbe get _gitRepositoryProbe =>
      WorkbenchGitRepositoryProbeAdapter(ref.read(gitBackendProvider));

  WorkbenchExplicitResourceCleaner get _explicitResourceCleaner {
    final clearTerminalSession = ref.exists(agentHookReceiverProvider)
        ? ref.read(agentHookReceiverProvider).clearTerminalSession
        : null;
    return WorkbenchExplicitResourceCleaner(
      runtimeLifecycle: ref.read(terminalRuntimeLifecycleProvider),
      forgetEditorSession: ref.read(editorSessionRegistryProvider).forget,
      removeWorkspaceActivity: ref
          .read(workspaceActivityControllerProvider.notifier)
          .removeWorkspace,
      clearAgentWorkspace: ref
          .read(agentStatusControllerProvider.notifier)
          .clearWorkspace,
      clearTerminalSession: clearTerminalSession,
      clearTerminalOverlays: ref
          .read(agentRuntimeOverlayServiceProvider)
          .clearTerminalOverlays,
    );
  }

  WorkbenchRetiredResourceCleaner get _retiredResourceCleaner {
    final clearTerminalSession = ref.exists(agentHookReceiverProvider)
        ? ref.read(agentHookReceiverProvider).clearTerminalSession
        : null;
    return WorkbenchRetiredResourceCleaner(
      runtimeLifecycle: ref.read(terminalRuntimeLifecycleProvider),
      forgetEditorSession: ref.read(editorSessionRegistryProvider).forget,
      clearTerminalSession: clearTerminalSession,
    );
  }

  WorkbenchRetiredWorkspaceCleanupCoordinator get _retiredWorkspaceCleanup =>
      WorkbenchRetiredWorkspaceCleanupCoordinator(
        releaseHostedReviewTabsInBackground:
            _hostedReviewRetention.releaseTabsInBackground,
        forgetFocusHistory: _tabFocusHistory.forget,
        releaseLocalWorkspace: _retiredResourceCleaner.releaseWorkspace,
      );
  WorkbenchRetiredTabsCleanupCoordinator get _retiredTabsCleanup =>
      WorkbenchRetiredTabsCleanupCoordinator(
        releaseHostedReviewTabsInBackground:
            _hostedReviewRetention.releaseTabsInBackground,
        releaseLocalTabs: _retiredResourceCleaner.releaseTabs,
      );

  WorkbenchViewPrefsRepository? get _viewPrefsRepository {
    try {
      return ref.read(workbenchViewPrefsRepositoryProvider);
    } catch (_) {
      return null;
    }
  }

  final WorkbenchRootSubscriptionRegistry _rootSubscriptions =
      WorkbenchRootSubscriptionRegistry();
  final WorkbenchWorkspaceSubscriptionRegistry _workspaceSubscriptions =
      WorkbenchWorkspaceSubscriptionRegistry();
  final WorkbenchWorktreeMetadataWatcherRegistry
  _worktreeMetadataWatcherRegistry = WorkbenchWorktreeMetadataWatcherRegistry();
  final WorkbenchTabSubscriptionRegistry _tabSubscriptions =
      WorkbenchTabSubscriptionRegistry();
  final WorkbenchMainWorkspacePreparationCoordinator _mainWorkspacePreparation =
      WorkbenchMainWorkspacePreparationCoordinator();
  final WorkbenchLayoutLoadCoordinator _layoutLoading =
      WorkbenchLayoutLoadCoordinator();
  final WorkbenchWorkspaceTabClosingScope _tabClosingScope =
      WorkbenchWorkspaceTabClosingScope();
  final WorkbenchClearedLayoutRegistry _clearedLayouts =
      WorkbenchClearedLayoutRegistry();
  final WorkbenchFileTabMutationQueue _fileTabMutations =
      WorkbenchFileTabMutationQueue();
  final WorkbenchViewPrefsPersistenceQueue _viewPrefsPersistence =
      WorkbenchViewPrefsPersistenceQueue();
  final WorkbenchWorkspaceTagMutationQueue _workspaceTagMutations =
      WorkbenchWorkspaceTagMutationQueue();
  final WorkbenchWorkspaceParentMutationQueue _workspaceParentMutations =
      WorkbenchWorkspaceParentMutationQueue();
  final WorkbenchSerialMutationQueue _workspaceTreePinMutations =
      WorkbenchSerialMutationQueue();

  final WorkspaceTabFocusHistory _tabFocusHistory = WorkspaceTabFocusHistory();
  final WorkbenchNavigationHistoryService _navigationHistory =
      WorkbenchNavigationHistoryService();
  final WorkbenchBootstrapGate _bootstrapGate = WorkbenchBootstrapGate();

  bool get canGoBack => _navigationHistory.canGoBack(state);

  bool get canGoForward => _navigationHistory.canGoForward(state);

  void _pruneWorktreeNavigationHistory() {
    _navigationHistory.prune(state);
  }

  void _notifyNavigationHistoryChanged() {
    if (!_disposed) {
      state = state.copyWith();
    }
  }

  Future<void> _persistViewPrefs() async {
    final repo = _viewPrefsRepository;
    if (repo == null) {
      return;
    }
    final prefs = state.viewPrefs;
    try {
      await _viewPrefsPersistence.save(repository: repo, prefs: prefs);
    } catch (_) {
      // Persistence is best-effort; never surface an error from the UI path.
    }
  }

  Project? _projectById(Iterable<Project> projects, String? projectId) {
    if (projectId == null) {
      return null;
    }
    for (final project in projects) {
      if (project.id == projectId) {
        return project;
      }
    }
    return null;
  }

  Workspace? _workspaceById(String workspaceId) {
    return state.workspacesByProject.values
        .expand((workspaces) => workspaces)
        .where((workspace) => workspace.id == workspaceId)
        .firstOrNull;
  }

  Future<void> _activateAddedProject(Project project) async {
    await _ensureMainWorkspaceForProject(project);
    final plan = planWorkbenchAddedProjectActivation(
      state: state,
      project: project,
    );
    state = plan.state;
    if (plan.viewPrefsChanged) {
      unawaited(_persistViewPrefs());
    }
  }

  void _ensureSelectionHasTab() {
    final workspace = state.activeWorkspace;
    if (workspace == null) {
      return;
    }
    if (_tabClosingScope.isClosing(workspace.id)) {
      return;
    }
    if (state.tabsFor(workspace.id).isNotEmpty &&
        state.layoutFor(workspace.id) == null) {
      unawaited(_loadLayoutForWorkspace(workspace.id));
    }
  }

  Future<void> _loadLayoutForWorkspace(String workspaceId) {
    final tabService = _workspaceTabService;
    final layoutResolver = _layoutResolver;
    return _layoutLoading.load(
      workspaceId: workspaceId,
      listTabs: tabService.listTabs,
      resolveLayout: layoutResolver.resolve,
      applyLayout: (layout) => _applyLayout(layout, persist: false),
      onError: (error) {
        if (!_disposed) {
          state = state.copyWith(error: error.toString());
        }
      },
    );
  }

  WorkbenchLayout _layoutForMutation(
    String workspaceId,
    List<WorkspaceTabRecord> tabs,
  ) {
    return (state.layoutFor(workspaceId) ??
            WorkbenchLayout.single(
              workspaceId: workspaceId,
              tabIds: <String>[for (final tab in tabs) tab.id],
            ))
        .sanitize(tabs);
  }

  Future<void> _applyLayout(
    WorkbenchLayout layout, {
    required bool persist,
  }) async {
    state = applyWorkbenchLayoutState(state: state, layout: layout);
    final activeTabId = layout.activeTabId;
    if (activeTabId != null) {
      _tabFocusHistory.record(layout.workspaceId, activeTabId);
    }
    if (persist) {
      await _sequencedLayoutRepository.upsertWorkbenchLayout(layout);
    }
  }

  void _applyLayoutInBackground(
    WorkbenchLayout layout, {
    required bool persist,
  }) {
    unawaited(
      _applyLayout(layout, persist: persist).catchError(_recordLayoutError),
    );
  }

  void _persistLayoutInBackground(WorkbenchLayout layout) {
    unawaited(
      _sequencedLayoutRepository
          .upsertWorkbenchLayout(layout)
          .then<void>((_) {})
          .catchError(_recordLayoutError),
    );
  }

  void _recordLayoutError(Object error) {
    if (!_disposed) {
      state = state.copyWith(error: error.toString());
    }
  }

  void _setTabsForWorkspace(String workspaceId, List<WorkspaceTabRecord> tabs) {
    state = applyWorkbenchTabsState(
      state: state,
      workspaceId: workspaceId,
      tabs: tabs,
    );
  }

  String _newPaneGroupId() => 'pane-${_uuid.v4()}';

  void _setActiveTabInternal({
    required String workspaceId,
    required String tabId,
    String? groupId,
  }) {
    final layout = state.layoutFor(workspaceId);
    final resolvedGroupId = groupId ?? layout?.groupIdForTab(tabId);
    if (layout != null && resolvedGroupId != null) {
      final nextLayout = layout.setActiveTab(
        groupId: resolvedGroupId,
        tabId: tabId,
      );
      _applyLayoutInBackground(nextLayout, persist: true);
      return;
    }
    state = applyWorkbenchActiveTabState(
      state: state,
      workspaceId: workspaceId,
      tabId: tabId,
    );
    _tabFocusHistory.record(workspaceId, tabId);
  }

  Future<void> _ensureMainWorkspaceForProject(Project project) {
    final workspaceService = _workspaceService;
    return _mainWorkspacePreparation.prepare(
      project: project,
      ensureMainWorkspace: workspaceService.ensureMainWorkspace,
      reconcile: workspaceService.reconcile,
      onError: (project, error) {
        if (!_disposed) {
          state = state.copyWith(
            error: 'Failed to prepare workspace for "${project.name}": $error',
          );
        }
      },
    );
  }
}
