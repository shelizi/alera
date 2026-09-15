part of 'workbench_controller_test.dart';

void _registerWorkbenchControllerViewPrefsTests() {
  test(
    'background layout load does not restore layout for a removed workspace',
    () async {
      final workspace = Workspace(
        id: 'workspace-stale-layout',
        projectId: _harness.project.id,
        name: 'Stale Layout',
        branch: 'feature/stale-layout',
        path: p.join(_harness.project.repoPath, 'stale-layout'),
        createdAt: .utc(2026, 5, 22),
        updatedAt: .utc(2026, 5, 22),
        kind: .linked,
        status: .active,
      );
      final tab = WorkspaceTabRecord(
        id: 'stale-layout-tab',
        workspaceId: workspace.id,
        title: 'Terminal 1',
        createdAt: .utc(2026, 5, 22),
        updatedAt: .utc(2026, 5, 22),
      );
      final savedLayout = WorkbenchLayout.single(
        workspaceId: workspace.id,
        tabIds: <String>[tab.id],
      );
      final layoutStarted = Completer<void>();
      final layoutRead = Completer<WorkbenchLayout?>();
      _harness.workbenchRepository.blockFindWorkbenchLayoutWith(
        layoutRead.future,
        workspaceId: workspace.id,
        started: layoutStarted,
      );
      await _harness.workbenchRepository.upsertWorkspace(workspace);
      await _harness.workbenchRepository.upsertWorkspaceTab(tab);
      await _harness.workbenchRepository.upsertWorkbenchLayout(savedLayout);

      await _controller.bootstrap();
      await layoutStarted.future;
      await _harness.workbenchRepository.removeWorkspace(workspace.id);
      await _flushUntil(
        () => !_controller.state
            .workspacesFor(_harness.project.id)
            .any((candidate) => candidate.id == workspace.id),
      );

      layoutRead.complete(savedLayout);
      await _flush();

      expect(
        _controller.state.layoutByWorkspace.containsKey(workspace.id),
        isFalse,
      );
      expect(
        _controller.state.activeTabIdByWorkspace.containsKey(workspace.id),
        isFalse,
      );
    },
  );

  test(
    'tab watcher does not overwrite a saved split before layout load finishes',
    () async {
      final workspace = Workspace(
        id: 'workspace-1',
        projectId: _harness.project.id,
        name: 'Main',
        branch: 'main',
        path: _harness.project.repoPath,
        createdAt: .utc(2026, 5, 22),
        updatedAt: .utc(2026, 5, 22),
        kind: .main,
        status: .active,
      );
      final firstTab = WorkspaceTabRecord(
        id: 'tab-1',
        workspaceId: workspace.id,
        title: 'Terminal 1',
        createdAt: .utc(2026, 5, 22),
        updatedAt: .utc(2026, 5, 22),
      );
      final secondTab = WorkspaceTabRecord(
        id: 'tab-2',
        workspaceId: workspace.id,
        title: 'Terminal 2',
        createdAt: .utc(2026, 5, 22),
        updatedAt: .utc(2026, 5, 22),
      );
      final savedLayout =
          WorkbenchLayout.single(
            workspaceId: workspace.id,
            tabIds: <String>[firstTab.id],
          ).splitWithGroup(
            targetGroupId: WorkbenchLayout.defaultGroupId(workspace.id),
            zone: .right,
            newGroup: WorkbenchPaneGroup(
              id: 'group-2',
              tabIds: <String>[secondTab.id],
              activeTabId: secondTab.id,
            ),
          );
      final layoutRead = Completer<WorkbenchLayout?>();
      _harness.workbenchRepository.blockFindWorkbenchLayoutWith(
        layoutRead.future,
      );
      await _harness.workbenchRepository.upsertWorkspace(workspace);
      await _harness.workbenchRepository.upsertWorkspaceTab(firstTab);
      await _harness.workbenchRepository.upsertWorkspaceTab(secondTab);
      await _harness.workbenchRepository.upsertWorkbenchLayout(savedLayout);

      await _controller.bootstrap();
      await _flushUntil(
        () => _harness.workbenchRepository.hasTabWatcher(workspace.id),
      );

      _harness.workbenchRepository.emitTabs(workspace.id);
      await _flush();

      expect(
        _harness.workbenchRepository
            .peekWorkbenchLayout(workspace.id)
            ?.root
            .axis,
        WorkbenchSplitAxis.horizontal,
      );
      expect(_controller.state.layoutFor(workspace.id), isNull);

      layoutRead.complete(savedLayout);
      await _flushUntil(
        () => _controller.state.layoutFor(workspace.id) != null,
      );

      final persisted = _harness.workbenchRepository.peekWorkbenchLayout(
        workspace.id,
      );
      expect(persisted?.root.axis, WorkbenchSplitAxis.horizontal);
      expect(persisted?.paneGroupIds, hasLength(2));
      expect(
        _controller.state.layoutFor(workspace.id)?.paneGroupIds,
        hasLength(2),
      );
    },
  );

  test('bootstrap ignores persisted view-prefs load failures', () async {
    _harness.viewPrefsRepository.loadError = Exception('bad prefs');

    await _controller.bootstrap();
    await _flushUntil(
      () => _controller.state.workspacesFor(_harness.project.id).isNotEmpty,
    );

    expect(_controller.state.bootstrapped, isTrue);
    expect(_controller.state.viewPrefs, WorkbenchViewPrefs.defaults);
    expect(_controller.state.error, isNull);
  });

  test('selecting a workspace preserves agent expansion prefs', () async {
    await _controller.bootstrap();
    await _flushUntil(
      () => _controller.state.workspacesFor(_harness.project.id).isNotEmpty,
    );
    final workspace = _controller.state
        .workspacesFor(_harness.project.id)
        .single;

    await _controller.selectWorkspace(
      project: _harness.project,
      workspace: workspace,
    );
    await _flush();
    expect(
      _controller.state.viewPrefs.expandedWorkspaceIds,
      isNot(contains(workspace.id)),
    );

    _controller.setWorkspaceExpanded(workspace.id, true);
    await _controller.selectWorkspace(
      project: _harness.project,
      workspace: workspace,
    );
    await _flush();
    expect(
      _controller.state.viewPrefs.expandedWorkspaceIds,
      contains(workspace.id),
    );
  });

  test('view-pref mutators update state and persist changes', () async {
    await _controller.bootstrap();
    final mainWorkspace = await _selectMainWorkspace(_controller, _harness);
    final linkedWorkspace = (await _controller.createWorkspace(
      project: _harness.project,
      sourceBranch: 'main',
      newBranchName: 'feature/view-prefs',
    )).workspace;
    await _flush();

    _controller.setWorkspaceExpanded(mainWorkspace.id, true);
    _controller.setWorkspaceExpanded(linkedWorkspace.id, true);
    _controller.toggleCollapseAll();
    expect(
      _controller.state.viewPrefs.collapsedProjectIds,
      contains(_harness.project.id),
    );
    expect(
      _controller.state.viewPrefs.expandedWorkspaceIds,
      isNot(contains(mainWorkspace.id)),
    );
    expect(
      _controller.state.viewPrefs.expandedWorkspaceIds,
      isNot(contains(linkedWorkspace.id)),
    );

    _controller.toggleCollapseAll();
    expect(
      _controller.state.viewPrefs.collapsedProjectIds,
      isNot(contains(_harness.project.id)),
    );
    expect(
      _controller.state.viewPrefs.expandedWorkspaceIds,
      containsAll(<String>[mainWorkspace.id, linkedWorkspace.id]),
    );

    _controller.setGroupBy(.none);
    _controller.setWorkspaceExpanded(mainWorkspace.id, false);
    _controller.setWorkspaceExpanded(linkedWorkspace.id, false);
    _controller.setProjectSort(.recent);
    _controller.setWorkspaceSort(.recent);
    _controller.toggleCollapseAll();
    expect(
      _controller.state.viewPrefs.expandedWorkspaceIds,
      containsAll(<String>[mainWorkspace.id, linkedWorkspace.id]),
    );

    _controller.toggleCollapseAll();
    _controller.toggleWorkspaceExpanded(mainWorkspace.id);
    _controller.setWorkspaceExpanded(mainWorkspace.id, false);
    _controller.addProjectFilter(_harness.project.id);
    _controller.toggleProjectFilter(_harness.project.id);
    _controller.clearProjectFilters();
    _controller.setSearchQuery('terminal');
    _controller.setCollapsed(true);
    _controller.setGitDiffContentMode(.diffOnly);
    _controller.setGitDiffPresentationMode(.sideBySide);
    _controller.setGitDiffWhitespaceMode('ignoreChanges');
    _controller.setSidebarWidth(AleraTokens.sidebarMaxWidth + 400);
    await _flush();

    expect(
      _controller.state.viewPrefs.gitDiffContentMode,
      GitDiffContentMode.diffOnly,
    );
    expect(
      _controller.state.viewPrefs.gitDiffPresentationMode,
      GitDiffPresentationMode.sideBySide,
    );
    expect(_controller.state.viewPrefs.gitDiffWhitespaceMode, 'ignoreChanges');
    expect(_controller.state.viewPrefs.groupBy, WorkbenchGroupBy.none);
    expect(_controller.state.viewPrefs.projectSort, WorkbenchSortBy.recent);
    expect(_controller.state.viewPrefs.workspaceSort, WorkbenchSortBy.recent);
    expect(_controller.state.viewPrefs.selectedProjectIds, isEmpty);
    expect(
      _controller.state.viewPrefs.expandedWorkspaceIds,
      isNot(contains(mainWorkspace.id)),
    );
    expect(_controller.state.searchQuery, 'terminal');
    expect(_controller.state.collapsed, isTrue);
    expect(
      _controller.state.viewPrefs.sidebarWidth,
      AleraTokens.sidebarMaxWidth,
    );
    expect(
      _harness.viewPrefsRepository.prefs.workspaceSort,
      WorkbenchSortBy.recent,
    );
    expect(
      _harness.viewPrefsRepository.prefs.gitDiffWhitespaceMode,
      'ignoreChanges',
    );
    expect(_harness.viewPrefsRepository.saveCount, greaterThan(0));
  });

  test(
    'newer view prefs save wins when an older save completes late',
    () async {
      await _controller.bootstrap();
      await _flush();
      final started = Completer<void>();
      final release = Completer<void>();
      final completed = Completer<void>();
      _harness.viewPrefsRepository
        ..saveStarted = started
        ..saveRelease = release
        ..saveCompleted = completed;

      _controller.setSidebarWidth(320);
      await started.future;
      _controller.setSidebarWidth(440);
      expect(_controller.state.viewPrefs.sidebarWidth, 440);

      release.complete();
      await completed.future;
      await _flushUntil(() => _harness.viewPrefsRepository.saveCount >= 2);

      expect(_harness.viewPrefsRepository.prefs.sidebarWidth, 440);
    },
  );

  test(
    'bootstrap completion is ignored after the controller is disposed',
    () async {
      final harness = _WorkbenchHarness();
      final controller = harness._controller;
      final loadGate = Completer<WorkbenchViewPrefs>();
      harness.viewPrefsRepository.loadOverride = loadGate.future;

      try {
        final bootstrap = controller.bootstrap();
        await _flush();
        harness.container.dispose();
        loadGate.complete(WorkbenchViewPrefs.defaults);

        await expectLater(bootstrap, completes);
      } finally {
        await harness.dispose();
      }
    },
  );

  test('concurrent bootstrap callers wait for the same bootstrap', () async {
    final loadGate = Completer<WorkbenchViewPrefs>();
    _harness.viewPrefsRepository.loadOverride = loadGate.future;

    final first = _controller.bootstrap();
    await _flush();
    var secondCompleted = false;
    final second = _controller.bootstrap().whenComplete(() {
      secondCompleted = true;
    });
    await _flush();

    expect(secondCompleted, isFalse);

    loadGate.complete(WorkbenchViewPrefs.defaults);
    await Future.wait<void>(<Future<void>>[first, second]);

    expect(_controller.state.bootstrapped, isTrue);
  });

  test('bootstrap prunes stale persisted project filters', () async {
    _harness.viewPrefsRepository.prefs = WorkbenchViewPrefs.defaults.copyWith(
      collapsedProjectIds: <String>{'stale-project', _harness.project.id},
      selectedProjectIds: <String>{'stale-project', _harness.project.id},
    );

    await _controller.bootstrap();
    await _flushUntil(
      () => _controller.state.workspacesFor(_harness.project.id).isNotEmpty,
    );

    expect(_controller.state.viewPrefs.collapsedProjectIds, <String>{
      _harness.project.id,
    });
    expect(_controller.state.viewPrefs.selectedProjectIds, <String>{
      _harness.project.id,
    });
  });

  test('bootstrap surfaces project repository failures', () async {
    _harness.projectRepository.listAllError = StateError(
      'cannot list projects',
    );

    await _controller.bootstrap();
    await _flush();

    expect(_controller.state.bootstrapped, isTrue);
    expect(
      _controller.state.error,
      contains(
        'Failed to bootstrap workbench: Bad state: cannot list projects',
      ),
    );
  });

  test(
    'workspace updates prune expansion ids for removed workspaces',
    () async {
      await _controller.bootstrap();
      await _selectMainWorkspace(_controller, _harness);
      final linkedWorkspace = (await _controller.createWorkspace(
        project: _harness.project,
        sourceBranch: 'main',
        newBranchName: 'feature/remove-expanded',
      )).workspace;
      await _flush();

      _controller.setWorkspaceExpanded(linkedWorkspace.id, true);
      await _flush();

      expect(
        _controller.state.viewPrefs.expandedWorkspaceIds,
        contains(linkedWorkspace.id),
      );

      await _harness.workbenchRepository.removeWorkspace(linkedWorkspace.id);
      await _flush();

      expect(
        _controller.state.viewPrefs.expandedWorkspaceIds,
        isNot(contains(linkedWorkspace.id)),
      );
    },
  );
}
