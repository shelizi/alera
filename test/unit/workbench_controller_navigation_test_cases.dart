part of 'workbench_controller_test.dart';

void _registerWorkbenchControllerNavigationTests() {
  test(
    'opening a persisted fork selects it before and after a delayed tab event',
    () async {
      await _controller.bootstrap();
      final workspace = await _selectMainWorkspace(_controller, _harness);
      final original = await _controller.createTerminalTab(workspace);
      await _flush();
      final fork = original.copyWith(id: 'fork-tab', title: 'Fork');
      final tabs = [
        ...await _harness.workbenchRepository.listWorkspaceTabs(workspace.id),
        fork,
      ];
      _harness.workbenchRepository._tabsByWorkspace[workspace.id] = tabs;
      expect(
        _controller.state.tabsFor(workspace.id).any((tab) => tab.id == fork.id),
        isFalse,
      );
      await _controller.openPersistedWorkspaceTab(
        workspaceId: workspace.id,
        tabId: fork.id,
      );
      expect(_controller.state.activeWorkspaceTab?.id, fork.id);
      expect(_controller.state.layoutFor(workspace.id)?.activeTabId, fork.id);
      _harness.workbenchRepository._tabControllers[workspace.id]?.add(tabs);
      await _flush();
      expect(_controller.state.activeWorkspaceTab?.id, fork.id);
      expect(_controller.state.layoutFor(workspace.id)?.activeTabId, fork.id);
      expect(
        _controller.state
            .tabsFor(workspace.id)
            .where((tab) => tab.id == original.id),
        hasLength(1),
      );
      expect(
        _controller.state
            .tabsFor(workspace.id)
            .where((tab) => tab.id == fork.id),
        hasLength(1),
      );
    },
  );

  test('opens a verified saved history tab across worktrees without duplicate tabs or stale focus', () async {
    await _controller.bootstrap();
    final mainWorkspace = await _selectMainWorkspace(_controller, _harness);
    final otherWorkspace = (await _controller.createWorkspace(
      project: _harness.project,
      sourceBranch: 'main',
      newBranchName: 'feature/exact-history-owner',
    )).workspace;
    final existingTab = _controller.state.tabsFor(otherWorkspace.id).first;
    final savedHistory = existingTab.copyWith(
      id: 'historical-claude-tab',
      title: 'Verified Claude history',
    );
    final saved = [
      ...await _harness.workbenchRepository.listWorkspaceTabs(
        otherWorkspace.id,
      ),
      savedHistory,
    ];
    _harness.workbenchRepository._tabsByWorkspace[otherWorkspace.id] = saved;

    await _controller.selectWorkspace(
      project: _harness.project,
      workspace: mainWorkspace,
    );
    expect(_controller.state.activeWorkspaceId, mainWorkspace.id);
    expect(
      _controller.state
          .tabsFor(otherWorkspace.id)
          .any((tab) => tab.id == savedHistory.id),
      isFalse,
    );

    await _controller.openPersistedWorkspaceTab(
      workspaceId: otherWorkspace.id,
      tabId: savedHistory.id,
    );
    expect(_controller.state.activeWorkspaceId, otherWorkspace.id);
    expect(_controller.state.activeWorkspaceTab?.id, savedHistory.id);
    expect(
      _controller.state.layoutFor(otherWorkspace.id)?.activeTabId,
      savedHistory.id,
    );
    await _controller.openPersistedWorkspaceTab(
      workspaceId: otherWorkspace.id,
      tabId: savedHistory.id,
    );
    _harness.workbenchRepository._tabControllers[otherWorkspace.id]?.add(saved);
    await _flush();
    expect(_controller.state.activeWorkspaceId, otherWorkspace.id);
    expect(_controller.state.activeWorkspaceTab?.id, savedHistory.id);
    expect(
      _controller.state
          .tabsFor(otherWorkspace.id)
          .where((tab) => tab.id == savedHistory.id),
      hasLength(1),
    );
    expect(
      _controller.state
          .tabsFor(mainWorkspace.id)
          .where((tab) => tab.id == savedHistory.id),
      isEmpty,
    );
  });

  test(
    'persisted history refuses to open a Tab in a different Worktree',
    () async {
      await _controller.bootstrap();
      final mainWorkspace = await _selectMainWorkspace(_controller, _harness);
      final otherWorkspace = (await _controller.createWorkspace(
        project: _harness.project,
        sourceBranch: 'main',
        newBranchName: 'feature/history-wrong-owner',
      )).workspace;
      final otherTab = _controller.state.tabsFor(otherWorkspace.id).first;
      await _controller.selectWorkspace(
        project: _harness.project,
        workspace: mainWorkspace,
      );

      await expectLater(
        _controller.openPersistedWorkspaceTab(
          workspaceId: mainWorkspace.id,
          tabId: otherTab.id,
        ),
        throwsStateError,
      );
      expect(_controller.state.activeWorkspaceId, mainWorkspace.id);
      expect(
        _controller.state
            .tabsFor(mainWorkspace.id)
            .where((tab) => tab.id == otherTab.id),
        isEmpty,
      );
    },
  );
  test('late history hydration cannot steal focus after another Worktree is selected', () async {
    await _controller.bootstrap();
    final mainWorkspace = await _selectMainWorkspace(_controller, _harness);
    final otherWorkspace = (await _controller.createWorkspace(
      project: _harness.project,
      sourceBranch: 'main',
      newBranchName: 'feature/history-navigation-race',
    )).workspace;
    final savedHistory = _controller.state
        .tabsFor(otherWorkspace.id)
        .first
        .copyWith(id: 'history-navigation-race-tab');
    _harness.workbenchRepository._tabsByWorkspace[otherWorkspace.id] = [
      ...await _harness.workbenchRepository.listWorkspaceTabs(
        otherWorkspace.id,
      ),
      savedHistory,
    ];
    await _controller.selectWorkspace(
      project: _harness.project,
      workspace: mainWorkspace,
    );

    final started = Completer<void>();
    final release = Completer<void>();
    _harness.workbenchRepository.blockNextWorkspaceTabsList(
      workspaceId: otherWorkspace.id,
      started: started,
      release: release,
    );
    final navigating = _controller.openPersistedWorkspaceTab(
      workspaceId: otherWorkspace.id,
      tabId: savedHistory.id,
    );
    await started.future;
    // An explicit newer selection must invalidate the old navigation.
    await _controller.selectWorkspace(
      project: _harness.project,
      workspace: mainWorkspace,
    );
    release.complete();
    await navigating;
    await _flush();

    expect(_controller.state.activeWorkspaceId, mainWorkspace.id);
    expect(_controller.state.activeWorkspaceTab?.id, isNot(savedHistory.id));
    expect(
      _controller.state
          .tabsFor(mainWorkspace.id)
          .where((tab) => tab.id == savedHistory.id),
      isEmpty,
    );
  });
  test(
    'records worktree selection and replays back and forward safely',
    () async {
      await _controller.bootstrap();
      final mainWorkspace = await _selectMainWorkspace(_controller, _harness);
      expect(_controller.canGoBack, isFalse);
      expect(_controller.canGoForward, isFalse);

      await _controller.selectWorkspace(
        project: _harness.project,
        workspace: mainWorkspace,
      );
      expect(_controller.canGoBack, isFalse);

      final first = (await _controller.createWorkspace(
        project: _harness.project,
        sourceBranch: 'main',
        newBranchName: 'feature/navigation-first',
      )).workspace;
      final second = (await _controller.createWorkspace(
        project: _harness.project,
        sourceBranch: 'main',
        newBranchName: 'feature/navigation-second',
      )).workspace;
      expect(_controller.state.activeWorkspaceId, second.id);

      await _controller.goBack();
      expect(_controller.state.activeWorkspaceId, first.id);
      expect(_controller.canGoForward, isTrue);

      _controller.setSearchQuery('does-not-match');
      await _controller.goBack();
      expect(_controller.state.activeWorkspaceId, mainWorkspace.id);
      expect(_controller.canGoForward, isTrue);

      await _controller.goForward();
      expect(_controller.state.activeWorkspaceId, first.id);

      await _controller.selectWorkspace(
        project: _harness.project,
        workspace: mainWorkspace,
      );
      expect(_controller.canGoForward, isFalse);
    },
  );

  test('project activation while back navigation hydrates does not move history cursor', () async {
    await _controller.bootstrap();
    await _selectMainWorkspace(_controller, _harness);
    final first = (await _controller.createWorkspace(
      project: _harness.project,
      sourceBranch: 'main',
      newBranchName: 'feature/back-project-invalidation-first',
    )).workspace;
    final second = (await _controller.createWorkspace(
      project: _harness.project,
      sourceBranch: 'main',
      newBranchName: 'feature/back-project-invalidation-second',
    )).workspace;
    expect(_controller.state.activeWorkspaceId, second.id);
    expect(_controller.canGoForward, isFalse);

    final started = Completer<void>();
    final release = Completer<void>();
    _harness.workbenchRepository.blockNextWorkspaceTabsList(
      workspaceId: first.id,
      started: started,
      release: release,
    );
    final navigating = _controller.goBack();
    await started.future;
    // activateProject does not call selectWorkspace. Its explicit deselection
    // must still invalidate the older navigation transaction.
    await _controller.activateProject(_harness.project);
    expect(_controller.state.activeWorkspaceId, isNull);
    release.complete();
    await navigating;
    expect(_controller.state.activeWorkspaceId, isNull);
    expect(_controller.canGoForward, isFalse);
    await _controller.goBack();
    expect(_controller.state.activeWorkspaceId, first.id);
  });

  test('skips deleted workspaces in navigation history', () async {
    await _controller.bootstrap();
    final mainWorkspace = await _selectMainWorkspace(_controller, _harness);
    final removedWorkspace = (await _controller.createWorkspace(
      project: _harness.project,
      sourceBranch: 'main',
      newBranchName: 'feature/navigation-removed',
    )).workspace;
    final currentWorkspace = (await _controller.createWorkspace(
      project: _harness.project,
      sourceBranch: 'main',
      newBranchName: 'feature/navigation-current',
    )).workspace;

    await _harness.workbenchRepository.removeWorkspace(removedWorkspace.id);
    await _flushUntil(
      () => !_controller.state
          .workspacesFor(_harness.project.id)
          .any((workspace) => workspace.id == removedWorkspace.id),
    );

    expect(_controller.state.activeWorkspaceId, currentWorkspace.id);
    expect(_controller.canGoBack, isTrue);
    await _controller.goBack();
    expect(_controller.state.activeWorkspaceId, mainWorkspace.id);
    expect(_controller.state.searchQuery, isEmpty);
  });

  test(
    'selectWorkspaceTab switches workspaces before focusing the tab',
    () async {
      await _controller.bootstrap();
      final mainWorkspace = await _selectMainWorkspace(_controller, _harness);
      final otherWorkspace = (await _controller.createWorkspace(
        project: _harness.project,
        sourceBranch: 'main',
        newBranchName: 'feature/codex-resume-target',
      )).workspace;
      final targetTab = _controller.state.tabsFor(otherWorkspace.id).first;

      await _controller.selectWorkspace(
        project: _harness.project,
        workspace: mainWorkspace,
      );
      expect(_controller.state.activeWorkspaceId, mainWorkspace.id);

      await _controller.selectWorkspaceTab(
        workspaceId: otherWorkspace.id,
        tabId: targetTab.id,
      );

      expect(_controller.state.activeWorkspaceId, otherWorkspace.id);
      expect(_controller.state.activeWorkspaceTab?.id, targetTab.id);
    },
  );

  test('selectWorkspaceTab does not focus a tab removed while workspace selection is in flight', () async {
    await _controller.bootstrap();
    final mainWorkspace = await _selectMainWorkspace(_controller, _harness);
    final otherWorkspace = (await _controller.createWorkspace(
      project: _harness.project,
      sourceBranch: 'main',
      newBranchName: 'feature/stale-tab-selection',
    )).workspace;
    final targetTab = _controller.state.tabsFor(otherWorkspace.id).first;
    await _controller.selectWorkspace(
      project: _harness.project,
      workspace: mainWorkspace,
    );

    final started = Completer<void>();
    final release = Completer<void>();
    _harness.workbenchRepository.blockNextWorkspaceTabsList(
      workspaceId: otherWorkspace.id,
      started: started,
      release: release,
    );
    final selecting = _controller.selectWorkspaceTab(
      workspaceId: otherWorkspace.id,
      tabId: targetTab.id,
    );
    await started.future;

    await _harness.workbenchRepository.removeWorkspaceTab(targetTab.id);
    await _flush();
    release.complete();
    await selecting;
    await _flush();

    expect(_controller.state.activeWorkspaceId, otherWorkspace.id);
    expect(
      _controller.state
          .tabsFor(otherWorkspace.id)
          .any((tab) => tab.id == targetTab.id),
      isFalse,
    );
    expect(
      _controller.state.activeTabIdByWorkspace[otherWorkspace.id],
      isNot(targetTab.id),
    );
  });

  test('selectWorkspace drops hydration for a workspace removed while selection is in flight', () async {
    await _controller.bootstrap();
    final mainWorkspace = await _selectMainWorkspace(_controller, _harness);
    final otherWorkspace = (await _controller.createWorkspace(
      project: _harness.project,
      sourceBranch: 'main',
      newBranchName: 'feature/stale-workspace-selection',
    )).workspace;
    await _controller.selectWorkspace(
      project: _harness.project,
      workspace: mainWorkspace,
    );

    final started = Completer<void>();
    final release = Completer<void>();
    _harness.workbenchRepository.blockNextWorkspaceTabsList(
      workspaceId: otherWorkspace.id,
      started: started,
      release: release,
    );
    final selecting = _controller.selectWorkspace(
      project: _harness.project,
      workspace: otherWorkspace,
    );
    await started.future;

    await _harness.workbenchRepository.removeWorkspace(otherWorkspace.id);
    await _flushUntil(
      () => !_controller.state
          .workspacesFor(_harness.project.id)
          .any((workspace) => workspace.id == otherWorkspace.id),
    );
    release.complete();
    await selecting;
    await _flush();

    expect(_controller.state.activeWorkspace, isNull);
    expect(
      _controller.state.tabsByWorkspace.containsKey(otherWorkspace.id),
      isFalse,
    );
    expect(
      _controller.state.layoutByWorkspace.containsKey(otherWorkspace.id),
      isFalse,
    );
  });

  test('prunes navigation entries when their project is removed', () async {
    await _controller.bootstrap();
    await _selectMainWorkspace(_controller, _harness);
    final secondWorkspace = (await _controller.createWorkspace(
      project: _harness.project,
      sourceBranch: 'main',
      newBranchName: 'feature/navigation-project',
    )).workspace;
    expect(_controller.state.activeWorkspaceId, secondWorkspace.id);

    final otherProject = await _harness.addProject('project-2', 'Other');
    await _flushUntil(
      () => _controller.state.projects.any(
        (project) => project.id == otherProject.id,
      ),
    );
    await _flushUntil(
      () => _controller.state.workspacesFor(otherProject.id).isNotEmpty,
    );
    final otherWorkspace = _controller.state
        .workspacesFor(otherProject.id)
        .single;
    await _controller.selectWorkspace(
      project: otherProject,
      workspace: otherWorkspace,
    );

    await _harness.projectRepository.remove(_harness.project.id);
    await _flushUntil(
      () => !_controller.state.projects.any(
        (project) => project.id == _harness.project.id,
      ),
    );

    expect(_controller.canGoBack, isFalse);
    expect(_controller.state.activeWorkspaceId, otherWorkspace.id);
  });
}
