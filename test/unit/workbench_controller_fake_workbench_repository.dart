part of 'workbench_controller_test.dart';

class _FakeWorkbenchRepository implements WorkbenchRepository {
  final Map<String, List<Workspace>> _workspacesByProject = {};
  final Map<String, List<WorkspaceTabRecord>> _tabsByWorkspace =
      <String, List<WorkspaceTabRecord>>{};
  final Map<String, WorkbenchLayout> _layoutsByWorkspace =
      <String, WorkbenchLayout>{};
  final Map<String, StreamController<List<Workspace>>> _workspaceControllers =
      <String, StreamController<List<Workspace>>>{};
  final Map<String, StreamController<List<WorkspaceTabRecord>>>
  _tabControllers = <String, StreamController<List<WorkspaceTabRecord>>>{};
  Future<WorkbenchLayout?>? _findWorkbenchLayoutOverride;
  String? _findWorkbenchLayoutOverrideWorkspaceId;
  Completer<void>? _findWorkbenchLayoutStarted;
  String? _blockedListWorkspaceTabsId;
  Completer<void>? _listWorkspaceTabsStarted;
  Completer<void>? _listWorkspaceTabsRelease;
  Completer<void>? _setWorkspacePinnedStarted;
  Completer<void>? _setWorkspacePinnedRelease;
  Completer<void>? _upsertWorkspaceTabStarted;
  Completer<void>? _upsertWorkspaceTabRelease;
  Completer<void>? _upsertWorkbenchLayoutStarted;
  Completer<void>? _upsertWorkbenchLayoutRelease;
  Completer<void>? _upsertWorkbenchLayoutCompleted;
  Object? upsertWorkspaceError, upsertWorkspaceTabError;
  Object? upsertWorkbenchLayoutError, removeWorkspaceTabError;
  int upsertWorkbenchLayoutCalls = 0;
  @override
  Future<List<Workspace>> listWorkspaces(String projectId) async {
    return List<Workspace>.from(
      _workspacesByProject[projectId] ?? const <Workspace>[],
    );
  }

  @override
  Stream<List<Workspace>> watchWorkspaces(String projectId) {
    return _workspaceControllers
        .putIfAbsent(
          projectId,
          () => StreamController<List<Workspace>>.broadcast(),
        )
        .stream;
  }

  @override
  Future<Workspace?> findWorkspaceById(String workspaceId) async {
    for (final workspaces in _workspacesByProject.values) {
      for (final workspace in workspaces) {
        if (workspace.id == workspaceId) {
          return workspace;
        }
      }
    }
    return null;
  }

  @override
  Future<Workspace> upsertWorkspace(Workspace workspace) async {
    if (upsertWorkspaceError case final Object error) {
      throw error;
    }
    final current = List<Workspace>.from(
      _workspacesByProject[workspace.projectId] ?? const <Workspace>[],
    );
    final index = current.indexWhere((entry) => entry.id == workspace.id);
    if (index == -1) {
      current.add(workspace);
    } else {
      current[index] = workspace;
    }
    current.sort((left, right) {
      if (left.isMain != right.isMain) {
        return left.isMain ? -1 : 1;
      }
      return left.createdAt.compareTo(right.createdAt);
    });
    _workspacesByProject[workspace.projectId] = current;
    _workspaceControllers[workspace.projectId]?.add(
      List<Workspace>.from(current),
    );
    return workspace;
  }

  @override
  Future<Workspace> setWorkspacePinned(
    String workspaceId,
    bool isPinned,
  ) async {
    final current = (await findWorkspaceById(workspaceId))!;
    final updated = await upsertWorkspace(current.copyWith(isPinned: isPinned));
    final started = _setWorkspacePinnedStarted;
    final release = _setWorkspacePinnedRelease;
    _setWorkspacePinnedStarted = null;
    _setWorkspacePinnedRelease = null;
    if (started != null && !started.isCompleted) {
      started.complete();
    }
    if (release != null) {
      await release.future;
    }
    return updated;
  }

  void blockNextWorkspacePinnedReturn({
    required Completer<void> started,
    required Completer<void> release,
  }) {
    _setWorkspacePinnedStarted = started;
    _setWorkspacePinnedRelease = release;
  }

  @override
  Future<void> removeWorkspace(
    String workspaceId, {
    bool cascadeTabs = true,
  }) async {
    String? projectId;
    for (final entry in _workspacesByProject.entries) {
      if (entry.value.any((workspace) => workspace.id == workspaceId)) {
        projectId = entry.key;
        entry.value.removeWhere((workspace) => workspace.id == workspaceId);
        break;
      }
    }
    if (projectId != null) {
      _workspaceControllers[projectId]?.add(
        List<Workspace>.from(
          _workspacesByProject[projectId] ?? const <Workspace>[],
        ),
      );
    }
    if (cascadeTabs) {
      await removeWorkspaceTabsForWorkspace(workspaceId);
    }
    await removeWorkbenchLayout(workspaceId);
  }

  @override
  Future<void> removeWorkspacesForProject(String projectId) async {
    final workspaces =
        _workspacesByProject.remove(projectId) ?? const <Workspace>[];
    _workspaceControllers[projectId]?.add(const <Workspace>[]);
    for (final workspace in workspaces) {
      await removeWorkspaceTabsForWorkspace(workspace.id);
      await removeWorkbenchLayout(workspace.id);
    }
  }

  @override
  Future<List<WorkspaceTabRecord>> listWorkspaceTabs(String workspaceId) async {
    if (_blockedListWorkspaceTabsId == workspaceId) {
      final started = _listWorkspaceTabsStarted;
      if (started != null && !started.isCompleted) {
        started.complete();
      }
      final release = _listWorkspaceTabsRelease;
      if (release != null) {
        await release.future;
      }
      _blockedListWorkspaceTabsId = null;
      _listWorkspaceTabsStarted = null;
      _listWorkspaceTabsRelease = null;
    }
    return List<WorkspaceTabRecord>.from(
      _tabsByWorkspace[workspaceId] ?? const <WorkspaceTabRecord>[],
    );
  }

  void blockNextWorkspaceTabsList({
    required String workspaceId,
    required Completer<void> started,
    required Completer<void> release,
  }) {
    _blockedListWorkspaceTabsId = workspaceId;
    _listWorkspaceTabsStarted = started;
    _listWorkspaceTabsRelease = release;
  }

  @override
  Stream<List<WorkspaceTabRecord>> watchWorkspaceTabs(String workspaceId) {
    return _tabControllers
        .putIfAbsent(
          workspaceId,
          () => StreamController<List<WorkspaceTabRecord>>.broadcast(),
        )
        .stream;
  }

  @override
  Future<WorkspaceTabRecord?> findWorkspaceTabById(String tabId) async {
    for (final tabs in _tabsByWorkspace.values) {
      for (final tab in tabs) {
        if (tab.id == tabId) {
          return tab;
        }
      }
    }
    return null;
  }

  @override
  Future<WorkbenchLayout?> findWorkbenchLayout(String workspaceId) async {
    final override = _findWorkbenchLayoutOverride;
    final overrideWorkspaceId = _findWorkbenchLayoutOverrideWorkspaceId;
    if (override != null &&
        (overrideWorkspaceId == null || overrideWorkspaceId == workspaceId)) {
      final started = _findWorkbenchLayoutStarted;
      _findWorkbenchLayoutStarted = null;
      if (started != null && !started.isCompleted) {
        started.complete();
      }
      return override;
    }
    return _layoutsByWorkspace[workspaceId];
  }

  @override
  Future<WorkspaceTabRecord> upsertWorkspaceTab(
    WorkspaceTabRecord tab, {
    bool manualRename = false,
  }) async {
    if (upsertWorkspaceTabError case final Object error) {
      throw error;
    }
    final current = List<WorkspaceTabRecord>.from(
      _tabsByWorkspace[tab.workspaceId] ?? const <WorkspaceTabRecord>[],
    );
    final index = current.indexWhere((entry) => entry.id == tab.id);
    if (index == -1) {
      current.add(tab);
    } else {
      current[index] = tab;
    }
    current.sort((left, right) => left.createdAt.compareTo(right.createdAt));
    _tabsByWorkspace[tab.workspaceId] = current;
    _tabControllers[tab.workspaceId]?.add(
      List<WorkspaceTabRecord>.from(current),
    );
    final started = _upsertWorkspaceTabStarted;
    final release = _upsertWorkspaceTabRelease;
    if (started != null) {
      _upsertWorkspaceTabStarted = null;
      _upsertWorkspaceTabRelease = null;
      if (!started.isCompleted) {
        started.complete();
      }
      if (release != null) {
        await release.future;
      }
    }
    return tab;
  }

  void blockNextWorkspaceTabUpsert({
    required Completer<void> started,
    required Completer<void> release,
  }) {
    _upsertWorkspaceTabStarted = started;
    _upsertWorkspaceTabRelease = release;
  }

  @override
  Future<WorkbenchLayout> upsertWorkbenchLayout(WorkbenchLayout layout) async {
    upsertWorkbenchLayoutCalls += 1;
    if (upsertWorkbenchLayoutError case final Object error) {
      throw error;
    }
    final started = _upsertWorkbenchLayoutStarted;
    final release = _upsertWorkbenchLayoutRelease;
    final completed = _upsertWorkbenchLayoutCompleted;
    if (started != null) {
      _upsertWorkbenchLayoutStarted = null;
      _upsertWorkbenchLayoutRelease = null;
      _upsertWorkbenchLayoutCompleted = null;
      if (!started.isCompleted) {
        started.complete();
      }
      if (release != null) {
        await release.future;
      }
    }
    _layoutsByWorkspace[layout.workspaceId] = layout;
    if (completed != null && !completed.isCompleted) {
      completed.complete();
    }
    return layout;
  }

  void blockNextWorkbenchLayoutUpsert({
    required Completer<void> started,
    required Completer<void> release,
    required Completer<void> completed,
  }) {
    _upsertWorkbenchLayoutStarted = started;
    _upsertWorkbenchLayoutRelease = release;
    _upsertWorkbenchLayoutCompleted = completed;
  }

  void blockFindWorkbenchLayoutWith(
    Future<WorkbenchLayout?> future, {
    String? workspaceId,
    Completer<void>? started,
  }) {
    _findWorkbenchLayoutOverride = future;
    _findWorkbenchLayoutOverrideWorkspaceId = workspaceId;
    _findWorkbenchLayoutStarted = started;
    future.whenComplete(() {
      if (identical(_findWorkbenchLayoutOverride, future)) {
        _findWorkbenchLayoutOverride = null;
        _findWorkbenchLayoutOverrideWorkspaceId = null;
      }
    });
  }

  WorkbenchLayout? peekWorkbenchLayout(String workspaceId) {
    return _layoutsByWorkspace[workspaceId];
  }

  bool hasWorkspaceWatcher(String projectId) {
    return _workspaceControllers.containsKey(projectId);
  }

  void killWorkspaceWatcher(String projectId) {
    final controller = _workspaceControllers.remove(projectId);
    if (controller == null) {
      return;
    }
    controller.addError(StateError('terminal host connection closed'));
    unawaited(controller.close());
  }

  bool hasTabWatcher(String workspaceId) {
    return _tabControllers.containsKey(workspaceId);
  }

  void killTabWatcher(String workspaceId) {
    final controller = _tabControllers.remove(workspaceId);
    if (controller == null) {
      return;
    }
    controller.addError(StateError('terminal host connection closed'));
    unawaited(controller.close());
  }

  void emitTabs(String workspaceId) {
    _tabControllers[workspaceId]?.add(
      List<WorkspaceTabRecord>.from(
        _tabsByWorkspace[workspaceId] ?? const <WorkspaceTabRecord>[],
      ),
    );
  }

  @override
  Future<void> removeWorkspaceTab(String tabId) async {
    if (removeWorkspaceTabError case final Object error) {
      throw error;
    }
    for (final entry in _tabsByWorkspace.entries) {
      final previousLength = entry.value.length;
      entry.value.removeWhere((tab) => tab.id == tabId);
      if (entry.value.length != previousLength) {
        _tabControllers[entry.key]?.add(
          List<WorkspaceTabRecord>.from(entry.value),
        );
        return;
      }
    }
  }

  @override
  Future<void> removeWorkspaceTabsForWorkspace(String workspaceId) async {
    _tabsByWorkspace.remove(workspaceId);
    _layoutsByWorkspace.remove(workspaceId);
    _tabControllers[workspaceId]?.add(const <WorkspaceTabRecord>[]);
  }

  @override
  Future<void> removeWorkbenchLayout(String workspaceId) async {
    _layoutsByWorkspace.remove(workspaceId);
  }

  Future<void> dispose() async {
    for (final controller in _workspaceControllers.values) {
      await controller.close();
    }
    for (final controller in _tabControllers.values) {
      await controller.close();
    }
  }
}
