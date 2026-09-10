part of 'workbench_controller.dart';

mixin _WorkbenchControllerInternals on _$WorkbenchController {
  final Uuid _uuid = const Uuid();
  bool _disposed = false;

  ProjectsService get _projectsService => ref.read(projectsServiceProvider);

  WorkbenchRepository get _repository => ref.read(workbenchRepositoryProvider);

  WorkspaceGraphRepository get _workspaceGraphRepository =>
      ref.read(workspaceGraphRepositoryProvider);

  WorkspaceService get _workspaceService => ref.read(workspaceServiceProvider);

  Future<T> _withWorktreeRefreshSuspended<T>(
    String projectId,
    Future<T> Function() action,
  ) async {
    final watcher = _worktreeMetadataWatchers[projectId];
    await watcher?.suspendRefresh();
    try {
      return await action();
    } finally {
      watcher?.resumeRefresh();
    }
  }

  WorkspaceTabService get _workspaceTabService =>
      ref.read(workspaceTabServiceProvider);

  WorkbenchHostedReviewRetentionService get _hostedReviewRetention =>
      WorkbenchHostedReviewRetentionService(
        gitBackend: ref.read(gitBackendProvider),
      );

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

  WorkbenchViewPrefsRepository? get _viewPrefsRepository {
    try {
      return ref.read(workbenchViewPrefsRepositoryProvider);
    } catch (_) {
      return null;
    }
  }

  StreamSubscription<WorkspaceSectionSnapshot>? _sectionsSub;
  StreamSubscription<List<Project>>? _projectsSub;
  StreamSubscription<WorkbenchViewPrefs>? _viewPrefsSub;
  final WorkbenchWorkspaceSubscriptionRegistry _workspaceSubscriptions =
      WorkbenchWorkspaceSubscriptionRegistry();
  final Map<String, GitWorktreeMetadataWatcher> _worktreeMetadataWatchers =
      <String, GitWorktreeMetadataWatcher>{};
  final WorkbenchTabSubscriptionRegistry _tabSubscriptions =
      WorkbenchTabSubscriptionRegistry();
  final Set<String> _ensuringMainWorkspaceProjectIds = <String>{};
  final Set<String> _loadingLayoutWorkspaceIds = <String>{};
  final Set<String> _closingTabWorkspaceIds = <String>{};
  final Set<String> _workspaceIdsWithClearedLayout = <String>{};
  Future<void>? _fileOpenQueue;

  final WorkspaceTabFocusHistory _tabFocusHistory = WorkspaceTabFocusHistory();
  final WorkbenchNavigationHistoryService _navigationHistory =
      WorkbenchNavigationHistoryService();

  bool _bootstrapStarted = false;

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
    try {
      await repo.save(state.viewPrefs);
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
    // Expand the project (remove from collapsed set if a stale id lingered).
    // Selection set is a positive filter - leave it untouched so we don't
    // accidentally start showing this brand-new project alone.
    final prefs = state.viewPrefs;
    final nextCollapsed = Set<String>.from(prefs.collapsedProjectIds)
      ..remove(project.id);
    final changedPrefs =
        nextCollapsed.length != prefs.collapsedProjectIds.length;
    final expandedPrefs = changedPrefs
        ? prefs.copyWith(collapsedProjectIds: nextCollapsed)
        : prefs;
    final nextViewPrefs = expandedPrefs;
    final prefsChanged = !identical(nextViewPrefs, prefs);
    state = state.copyWith(
      viewPrefs: nextViewPrefs,
      activeProjectId: project.id,
      activeWorkspaceId: null,
      error: null,
    );
    if (prefsChanged) {
      unawaited(_persistViewPrefs());
    }
  }

  void _ensureSelectionHasTab() {
    final workspace = state.activeWorkspace;
    if (workspace == null) {
      return;
    }
    if (_closingTabWorkspaceIds.contains(workspace.id)) {
      return;
    }
    if (state.tabsFor(workspace.id).isNotEmpty &&
        state.layoutFor(workspace.id) == null) {
      unawaited(_loadLayoutForWorkspace(workspace.id));
    }
  }

  Future<void> _loadLayoutForWorkspace(String workspaceId) async {
    if (!_loadingLayoutWorkspaceIds.add(workspaceId)) {
      return;
    }
    try {
      final tabs = await _workspaceTabService.listTabs(workspaceId);
      final layout = await _ensureWorkbenchLayout(workspaceId, tabs);
      await _applyLayout(layout, persist: false);
    } catch (error) {
      if (!_disposed) {
        state = state.copyWith(error: error.toString());
      }
    } finally {
      _loadingLayoutWorkspaceIds.remove(workspaceId);
    }
  }

  Future<WorkbenchLayout> _ensureWorkbenchLayout(
    String workspaceId,
    List<WorkspaceTabRecord> tabs,
  ) async {
    final stored = await _repository.findWorkbenchLayout(workspaceId);
    final layout =
        stored ??
        WorkbenchLayout.single(
          workspaceId: workspaceId,
          tabIds: <String>[for (final tab in tabs) tab.id],
        );
    final sanitized = layout.sanitize(tabs);
    if (stored == null || sanitized != stored) {
      await _repository.upsertWorkbenchLayout(sanitized);
    }
    return sanitized;
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
    final nextLayouts = Map<String, WorkbenchLayout>.from(
      state.layoutByWorkspace,
    )..[layout.workspaceId] = layout;
    state = state.copyWith(
      layoutByWorkspace: nextLayouts,
      activeTabIdByWorkspace: _activeTabsWithLayout(layout),
    );
    final activeTabId = layout.activeTabId;
    if (activeTabId != null) {
      _tabFocusHistory.record(layout.workspaceId, activeTabId);
    }
    if (persist) {
      await _repository.upsertWorkbenchLayout(layout);
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
      _repository
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

  Map<String, String> _activeTabsWithLayout(WorkbenchLayout layout) {
    final activeTabs = Map<String, String>.from(state.activeTabIdByWorkspace);
    final activeTabId = layout.activeTabId;
    if (activeTabId == null) {
      activeTabs.remove(layout.workspaceId);
    } else {
      activeTabs[layout.workspaceId] = activeTabId;
    }
    return activeTabs;
  }

  void _setTabsForWorkspace(String workspaceId, List<WorkspaceTabRecord> tabs) {
    final nextTabs = Map<String, List<WorkspaceTabRecord>>.from(
      state.tabsByWorkspace,
    )..[workspaceId] = tabs;
    state = state.copyWith(tabsByWorkspace: nextTabs);
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
    final next = Map<String, String>.from(state.activeTabIdByWorkspace)
      ..[workspaceId] = tabId;
    state = state.copyWith(activeTabIdByWorkspace: next);
    _tabFocusHistory.record(workspaceId, tabId);
  }

  Future<void> _ensureMainWorkspaceForProject(Project project) async {
    if (!_ensuringMainWorkspaceProjectIds.add(project.id)) {
      return;
    }
    try {
      await _workspaceService.ensureMainWorkspace(project);
      await _workspaceService.reconcile(project);
    } catch (error) {
      if (!_disposed) {
        state = state.copyWith(
          error: 'Failed to prepare workspace for "${project.name}": $error',
        );
      }
    } finally {
      _ensuringMainWorkspaceProjectIds.remove(project.id);
    }
  }
}
