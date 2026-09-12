part of 'workbench_controller.dart';

mixin _WorkbenchControllerInternals on _$WorkbenchController
    implements
        WorkbenchSelectionOwnerHost,
        WorkbenchTabLayoutOwnerHost,
        WorkbenchCatalogOwnerHost {
  bool _disposed = false;

  /// Selection and navigation live behind a narrow host port so the owner
  /// stays free of provider reads.
  late final WorkbenchSelectionOwner _selectionOwner = WorkbenchSelectionOwner(
    this,
  );

  /// Tab and layout operations live behind a narrow host port so the owner
  /// stays free of provider reads.
  late final WorkbenchTabLayoutOwner _tabLayoutOwner = WorkbenchTabLayoutOwner(
    this,
  );

  /// Project/workspace/section catalog operations live behind a narrow host
  /// port so the owner stays free of provider reads.
  late final WorkbenchCatalogOwner _catalogOwner = WorkbenchCatalogOwner(this);

  @override
  WorkbenchState readState() => state;

  @override
  void emitState(WorkbenchState next) => state = next;

  @override
  bool get isDisposed => _disposed;

  @override
  WorkbenchWorkspaceSelectionTabStore get selectionTabStore =>
      _workspaceTabService;

  @override
  WorkbenchWorkspaceSelectionLayoutResolver get selectionLayoutResolver =>
      _tabLayoutOwner.layoutResolver;

  @override
  WorkbenchGitRepositoryProbe get gitRepositoryProbe => _gitRepositoryProbe;

  @override
  void applyWorkspaceTabs(String workspaceId, List<WorkspaceTabRecord> tabs) =>
      _tabLayoutOwner.applyWorkspaceTabs(workspaceId, tabs);

  @override
  Future<void> applyWorkspaceLayout(
    WorkbenchLayout layout, {
    required bool persist,
  }) => _tabLayoutOwner.applyWorkspaceLayout(layout, persist: persist);

  @override
  void applyWorkspaceLayoutInBackground(
    WorkbenchLayout layout, {
    required bool persist,
  }) => _tabLayoutOwner.applyWorkspaceLayoutInBackground(
    layout,
    persist: persist,
  );

  @override
  void activateWorkspaceTab({
    required String workspaceId,
    required String tabId,
    String? groupId,
  }) => _tabLayoutOwner.activateWorkspaceTab(
    workspaceId: workspaceId,
    tabId: tabId,
    groupId: groupId,
  );

  @override
  void updateViewPrefs(WorkbenchViewPrefs prefs) => _updateViewPrefs(prefs);

  @override
  WorkspaceTabService get workspaceTabService => _workspaceTabService;

  @override
  WorkbenchLayoutRepository get layoutRepository => _repository;

  @override
  Future<WorkspaceTabRecord?> findPersistedWorkspaceTab(String tabId) =>
      _repository.findWorkspaceTabById(tabId);

  @override
  WorkbenchHostedReviewRetentionService get hostedReviewRetention =>
      _hostedReviewRetention;

  @override
  WorkbenchExplicitResourceCleaner get explicitResourceCleaner =>
      _explicitResourceCleaner;

  @override
  WorkbenchRetiredTabsCleanupCoordinator get retiredTabsCleanup =>
      _retiredTabsCleanup;

  @override
  WorkbenchReplaceableTabEditorSessions get replaceableTabEditorSessions =>
      _replaceableTabEditorSessions;

  @override
  WorkbenchWorkspaceActivityRecorder get workspaceActivityRecorder =>
      _workspaceActivityRecorder;

  @override
  Workspace? workspaceById(String workspaceId) => _workspaceById(workspaceId);

  @override
  bool isWorkspaceTabSubscriptionActive(String workspaceId) =>
      _tabSubscriptions.contains(workspaceId);

  @override
  Future<void> selectPersistedWorkspaceTab({
    required String workspaceId,
    required String tabId,
  }) => _selectionOwner.selectWorkspaceTab(
    workspaceId: workspaceId,
    tabId: tabId,
  );

  @override
  ProjectsService get projectsService => _projectsService;

  @override
  WorkbenchRepository get workbenchRepository => _repository;

  @override
  WorkspaceService get workspaceService => _workspaceService;

  @override
  WorkspaceGraphRepository get workspaceGraphRepository =>
      _workspaceGraphRepository;

  @override
  WorkbenchViewPrefsRepository? get viewPrefsRepository => _viewPrefsRepository;

  @override
  WorkbenchRetiredWorkspaceCleanupCoordinator get retiredWorkspaceCleanup =>
      _retiredWorkspaceCleanup;

  @override
  WorkbenchRemovedProjectWorkspaceCloser get removedProjectWorkspaceCloser =>
      _closeRemovedProjectWorkspace;

  @override
  WorkbenchTabSubscriptionRegistry get tabSubscriptions => _tabSubscriptions;

  @override
  WorkbenchWorkspaceTabClosingScope get workspaceTabClosingScope =>
      _tabLayoutOwner.tabClosingScope;

  @override
  WorkbenchClearedLayoutRegistry get clearedLayouts =>
      _tabLayoutOwner.clearedLayouts;

  @override
  WorkspaceTabFocusHistory get tabFocusHistory =>
      _tabLayoutOwner.tabFocusHistory;

  @override
  void persistViewPrefs() {
    unawaited(_persistViewPrefs());
  }

  @override
  void pruneWorktreeNavigationHistory() =>
      _selectionOwner.pruneNavigationHistory();

  @override
  Future<void> selectCatalogWorkspace({
    required Project project,
    required Workspace workspace,
    required bool ensureInitialTerminal,
  }) => _selectionOwner.selectWorkspace(
    project: project,
    workspace: workspace,
    ensureInitialTerminal: ensureInitialTerminal,
  );

  @override
  Future<void> openDeferredSetupTab(WorkspaceCreationResult result) =>
      _tabLayoutOwner.openDeferredSetupTab(result);

  @override
  Future<void> loadLayoutForWorkspace(String workspaceId) =>
      _tabLayoutOwner.loadLayoutForWorkspace(workspaceId);

  @override
  void handleWorkspaceTabsChanged(
    String workspaceId,
    List<WorkspaceTabRecord> tabs,
  ) => _tabLayoutOwner.handleTabsChanged(workspaceId, tabs);

  @override
  void ensureSelectionHasTab() => _tabLayoutOwner.ensureSelectionHasTab();

  ProjectsService get _projectsService => ref.read(projectsServiceProvider);

  WorkbenchRepository get _repository => ref.read(workbenchRepositoryProvider);

  WorkspaceGraphRepository get _workspaceGraphRepository =>
      ref.read(workspaceGraphRepositoryProvider);

  WorkspaceService get _workspaceService => ref.read(workspaceServiceProvider);

  WorkbenchRemovedProjectWorkspaceCloser get _closeRemovedProjectWorkspace =>
      ref.read(terminalRuntimeLifecycleProvider).closeWorkspace;

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
        forgetFocusHistory: _tabLayoutOwner.tabFocusHistory.forget,
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

  final WorkbenchTabSubscriptionRegistry _tabSubscriptions =
      WorkbenchTabSubscriptionRegistry();
  final WorkbenchViewPrefsPersistenceQueue _viewPrefsPersistence =
      WorkbenchViewPrefsPersistenceQueue();

  bool get canGoBack => _selectionOwner.canGoBack;

  bool get canGoForward => _selectionOwner.canGoForward;

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

  void _updateViewPrefsIfChanged(WorkbenchViewPrefs? prefs) {
    if (prefs != null) {
      _updateViewPrefs(prefs);
    }
  }

  void _updateViewPrefs(WorkbenchViewPrefs prefs) {
    state = state.copyWith(viewPrefs: prefs);
    unawaited(_persistViewPrefs());
  }

  Workspace? _workspaceById(String workspaceId) {
    return state.workspacesByProject.values
        .expand((workspaces) => workspaces)
        .where((workspace) => workspace.id == workspaceId)
        .firstOrNull;
  }
}
