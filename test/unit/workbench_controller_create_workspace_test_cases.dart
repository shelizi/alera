part of 'workbench_controller_test.dart';

void _registerWorkbenchControllerCreateWorkspaceTests() {
  test(
    'branch navigation selects existing Worktree without checkout',
    () async {
      await _controller.bootstrap();
      final current = await _selectMainWorkspace(_controller, _harness);
      final target = await _harness.workbenchRepository.upsertWorkspace(
        current.copyWith(
          id: 'linked-feature',
          kind: WorkspaceKind.linked,
          name: 'Feature',
          branch: 'feature/existing',
          path: p.join(_harness.tempDir.path, 'linked-feature'),
        ),
      );
      await _flushUntil(
        () => _controller.state
            .workspacesFor(current.projectId)
            .any((workspace) => workspace.id == target.id),
      );
      _controller.toggleProjectCollapsed(_harness.project.id);

      final outcome = await _controller.switchBranchOrSelectWorktree(
        project: _harness.project,
        workspace: current,
        branch: 'feature/existing',
      );

      expect(outcome, BranchWorkspaceNavigationResult.selectedWorktree);
      expect(_controller.state.activeWorkspaceId, target.id);
      expect(
        _controller.state.viewPrefs.collapsedProjectIds,
        isNot(contains(_harness.project.id)),
      );
      expect(
        _harness.gitBackend.calls.where(
          (call) => call.method == 'checkoutBranch',
        ),
        isEmpty,
      );
    },
  );

  test(
    'branch navigation restores archived Worktree instead of checkout',
    () async {
      await _controller.bootstrap();
      final current = await _selectMainWorkspace(_controller, _harness);
      final archived = await _harness.workbenchRepository.upsertWorkspace(
        current.copyWith(
          id: 'archived-feature',
          kind: WorkspaceKind.linked,
          branch: 'feature/archived',
          name: 'Archived',
          path: p.join(_harness.tempDir.path, 'archived-feature'),
          archivedAt: DateTime.utc(2026, 8, 1),
        ),
      );
      await _flushUntil(
        () => _controller.state
            .workspacesFor(current.projectId)
            .any((workspace) => workspace.id == archived.id),
      );

      final outcome = await _controller.switchBranchOrSelectWorktree(
        project: _harness.project,
        workspace: current,
        branch: 'feature/archived',
      );

      expect(outcome, BranchWorkspaceNavigationResult.selectedWorktree);
      expect(_controller.state.activeWorkspaceId, archived.id);
      expect(_controller.state.activeWorkspace?.isArchived, isFalse);
      expect(
        _harness.gitBackend.calls.where(
          (call) => call.method == 'checkoutBranch',
        ),
        isEmpty,
      );
    },
  );

  test('branch navigation checks out only when Worktree is absent', () async {
    await _controller.bootstrap();
    final current = await _selectMainWorkspace(_controller, _harness);
    _harness.gitBackend.sourceBranches.add('feature/unclaimed');

    final outcome = await _controller.switchBranchOrSelectWorktree(
      project: _harness.project,
      workspace: current,
      branch: 'feature/unclaimed',
    );

    expect(outcome, BranchWorkspaceNavigationResult.switchedBranch);
    expect(_controller.state.activeWorkspaceId, current.id);
    expect(_controller.state.activeWorkspace?.branch, 'feature/unclaimed');
    expect(
      _harness.gitBackend.calls.where(
        (call) => call.method == 'checkoutBranch',
      ),
      hasLength(1),
    );
  });

  test(
    'branch navigation preserves existing Editor and Terminal tabs',
    () async {
      await _controller.bootstrap();
      final main = await _selectMainWorkspace(_controller, _harness);
      final linked = (await _controller.createWorkspace(
        project: _harness.project,
        sourceBranch: 'main',
        newBranchName: 'feature/keep-tabs',
      )).workspace;
      await _flush();

      final editor = await _controller.openEditorTab(
        workspace: linked,
        relativePath: 'docs/keep-open.md',
      );
      await _flush();
      final originalTabs = _controller.state.tabsFor(linked.id);
      final originalTerminalIds = originalTabs
          .where((tab) => tab.kind == WorkspaceTabKind.terminal)
          .map((tab) => tab.id)
          .toList();
      expect(originalTerminalIds, isNotEmpty);
      expect(originalTabs.map((tab) => tab.id), contains(editor.id));

      await _controller.selectWorkspace(
        project: _harness.project,
        workspace: main,
      );
      final result = await _controller.switchBranchOrSelectWorktree(
        project: _harness.project,
        workspace: main,
        branch: 'feature/keep-tabs',
      );

      expect(result, BranchWorkspaceNavigationResult.selectedWorktree);
      expect(_controller.state.activeWorkspaceId, linked.id);
      final restoredTabs = _controller.state.tabsFor(linked.id);
      expect(
        restoredTabs.map((tab) => tab.id).toSet(),
        originalTabs.map((tab) => tab.id).toSet(),
        reason: 'Navigating should neither discard tabs nor add duplicates',
      );
      expect(restoredTabs.map((tab) => tab.id), contains(editor.id));
      expect(
        restoredTabs
            .where((tab) => tab.kind == WorkspaceTabKind.terminal)
            .map((tab) => tab.id),
        containsAll(originalTerminalIds),
      );
      expect(
        _harness.gitBackend.calls.where(
          (call) => call.method == 'checkoutBranch',
        ),
        isEmpty,
      );
    },
  );

  test(
    'createWorkspace surfaces service failures in state and rethrows',
    () async {
      _harness.gitBackend.failingWorktreeAddBranches.add('feature/broken');

      await expectLater(
        _controller.createWorkspace(
          project: _harness.project,
          sourceBranch: 'main',
          newBranchName: 'feature/broken',
        ),
        throwsA(isA<Exception>()),
      );

      expect(_controller.state.error, isNotNull);
      expect(
        _controller.state
            .workspacesFor(_harness.project.id)
            .where((workspace) => workspace.branch == 'feature/broken'),
        isEmpty,
      );
    },
  );

  test('createWorkspace ignores a blank parent workspace id', () async {
    final result = await _controller.createWorkspace(
      project: _harness.project,
      sourceBranch: 'main',
      newBranchName: 'feature/no-parent',
      parentWorkspaceId: '   ',
    );

    expect(result.hasParentLinkError, isFalse);
    expect(_harness.workspaceGraphRepository.linkedWorkspaces, isEmpty);
    expect(_controller.state.activeWorkspaceId, result.workspace.id);
  });

  test(
    'createWorkspace selects the workspace before its watcher catches up',
    () async {
      await _harness.dispose();
      _harness = _WorkbenchHarness(
        const _ManagedWorkspaceRuntimeWithoutWatcher(),
      );
      _controller = _harness._controller;
      await _controller.bootstrap();
      await _flushUntil(
        () => _controller.state.workspacesFor(_harness.project.id).isNotEmpty,
      );

      final result = await _controller.createWorkspace(
        project: _harness.project,
        sourceBranch: 'main',
        newBranchName: 'feature/delayed-workspace-event',
      );

      expect(_controller.state.activeProjectId, _harness.project.id);
      expect(_controller.state.activeWorkspace, result.workspace);
      expect(_controller.state.activeWorkspaceTab?.title, 'Terminal 1');
      expect(_controller.state.activeLayout?.workspaceId, result.workspace.id);

      await _harness.workbenchRepository.upsertWorkspace(result.workspace);
      await _flush();

      expect(_controller.state.activeWorkspace, result.workspace);
      expect(_controller.state.activeWorkspaceTab?.title, 'Terminal 1');
    },
  );

  test('createWorkspace clears a previous error on success', () async {
    _harness.gitBackend.failingWorktreeAddBranches.add('feature/first-try');
    await expectLater(
      _controller.createWorkspace(
        project: _harness.project,
        sourceBranch: 'main',
        newBranchName: 'feature/first-try',
      ),
      throwsA(isA<Exception>()),
    );
    expect(_controller.state.error, isNotNull);

    final result = await _controller.createWorkspace(
      project: _harness.project,
      sourceBranch: 'main',
      newBranchName: 'feature/second-try',
    );

    expect(result.workspace.branch, 'feature/second-try');
    expect(_controller.state.error, isNull);
  });

  test(
    'createWorkspace opens a Setup terminal for a deferred worktree setup',
    () async {
      await _harness.dispose();
      _harness = _WorkbenchHarness(
        const _ManagedWorkspaceRuntimeWithDeferredSetup(_setupCommand),
      );
      _controller = _harness._controller;
      await _controller.bootstrap();
      await _flushUntil(
        () => _controller.state.workspacesFor(_harness.project.id).isNotEmpty,
      );

      final result = await _controller.createWorkspace(
        project: _harness.project,
        sourceBranch: 'main',
        newBranchName: 'feature/deferred-setup',
      );

      final tabs = _controller.state.tabsFor(result.workspace.id);
      expect(tabs.map((tab) => tab.title), <String>['Terminal 1', 'Setup']);
      final setup = tabs.last;
      expect(setup.initialCommand, _setupCommand);
      expect(setup.initialCommandOnce, isTrue);
      expect(setup.spawnOnCreate, isTrue);
      expect(setup.autoCloseOnSuccess, isTrue);
      expect(_controller.state.activeWorkspaceTab?.title, 'Setup');
      expect(_controller.state.error, isNull);
    },
  );

  test('prompt creation opens the agent first and Setup second without a blank terminal', () async {
    await _harness.dispose();
    _harness = _WorkbenchHarness(
      const _ManagedWorkspaceRuntimeWithDeferredSetup(_setupCommand),
    );
    _controller = _harness._controller;
    await _controller.bootstrap();
    await _flushUntil(
      () => _controller.state.workspacesFor(_harness.project.id).isNotEmpty,
    );

    final result = await _controller.createWorkspaceForPrompt(
      project: _harness.project,
      sourceBranch: 'main',
      newBranchName: 'feature/prompt-terminal-order',
      name: 'Prompt Terminal Order',
    );

    expect(_controller.state.tabsFor(result.workspace.id), isEmpty);
    final now = DateTime.utc(2026, 5, 22, 4);
    await _harness.workbenchRepository.upsertWorkspaceTab(
      WorkspaceTabRecord(
        id: 'agent-tab',
        workspaceId: result.workspace.id,
        title: 'Codex',
        createdAt: now,
        updatedAt: now,
        payload: const <String, Object?>{
          workspaceTabTerminalSessionIdPayloadKey: 'agent-tab',
        },
      ),
    );

    await _controller.completePromptWorkspaceCreation(
      creation: result,
      agentTabId: 'agent-tab',
    );

    final tabs = _controller.state.tabsFor(result.workspace.id);
    expect(tabs.map((tab) => tab.title), <String>['Codex', 'Setup']);
    expect(tabs.where((tab) => tab.title.startsWith('Terminal ')), isEmpty);
    expect(tabs.last.initialCommand, _setupCommand);
    expect(tabs.last.initialCommandOnce, isTrue);
    expect(tabs.last.autoCloseOnSuccess, isTrue);
    expect(_controller.state.activeWorkspaceTab?.id, 'agent-tab');
  });

  test('createWorkspace leaves the workspace with one terminal when nothing is deferred', () async {
    await _harness.dispose();
    _harness = _WorkbenchHarness(
      const _ManagedWorkspaceRuntimeWithDeferredSetup(null),
    );
    _controller = _harness._controller;
    await _controller.bootstrap();
    await _flushUntil(
      () => _controller.state.workspacesFor(_harness.project.id).isNotEmpty,
    );

    final result = await _controller.createWorkspace(
      project: _harness.project,
      sourceBranch: 'main',
      newBranchName: 'feature/no-setup',
    );

    expect(
      _controller.state.tabsFor(result.workspace.id).map((tab) => tab.title),
      <String>['Terminal 1'],
    );
  });
}

const String _setupCommand = '/bin/sh "/run/alera/worktree-setup-ws.sh"';

class const _ManagedWorkspaceRuntimeWithoutWatcher()
    implements ManagedWorkspaceRuntime {
  @override
  Future<WorkspaceCreationResult> createLinkedWorkspace({
    required Project project,
    required String sourceBranch,
    required String newBranchName,
    required bool reuseExistingBranch,
    String? name,
  }) async {
    final now = DateTime.utc(2026, 5, 22, 2);
    return WorkspaceCreationResult(
      workspace: Workspace(
        id: 'workspace-with-delayed-event',
        projectId: project.id,
        name: name ?? newBranchName,
        branch: newBranchName,
        path: p.join(project.repoPath, 'delayed-workspace'),
        createdAt: now,
        updatedAt: now,
        kind: .linked,
        status: .active,
        sourceBranch: reuseExistingBranch ? null : sourceBranch,
        reusesExistingBranch: reuseExistingBranch,
      ),
      setupReport: .empty,
    );
  }

  @override
  Future<Workspace> switchWorkspaceBranch({
    required Workspace workspace,
    required String branch,
  }) async => workspace.copyWith(branch: branch);

  @override
  Future<void> removeWorkspace({
    required Workspace workspace,
    bool? deleteBranch,
    String? activeWorkspaceId,
    bool allowExternalWorktreeRemoval = false,
  }) async {}
}

/// Stands in for a host that prepared the worktree setup instead of running it.
/// A null command is what a host without deferral support reports.
class const _ManagedWorkspaceRuntimeWithDeferredSetup(
  final String? deferredSetupCommand,
) implements ManagedWorkspaceRuntime {
  @override
  Future<WorkspaceCreationResult> createLinkedWorkspace({
    required Project project,
    required String sourceBranch,
    required String newBranchName,
    required bool reuseExistingBranch,
    String? name,
  }) async {
    final now = DateTime.utc(2026, 5, 22, 3);
    return WorkspaceCreationResult(
      workspace: Workspace(
        id: 'workspace-with-deferred-setup',
        projectId: project.id,
        name: name ?? newBranchName,
        branch: newBranchName,
        path: p.join(project.repoPath, 'deferred-workspace'),
        createdAt: now,
        updatedAt: now,
        kind: .linked,
        status: .active,
        sourceBranch: reuseExistingBranch ? null : sourceBranch,
        reusesExistingBranch: reuseExistingBranch,
      ),
      setupReport: .empty,
      deferredSetupCommand: deferredSetupCommand,
    );
  }

  @override
  Future<Workspace> switchWorkspaceBranch({
    required Workspace workspace,
    required String branch,
  }) async => workspace.copyWith(branch: branch);

  @override
  Future<void> removeWorkspace({
    required Workspace workspace,
    bool? deleteBranch,
    String? activeWorkspaceId,
    bool allowExternalWorktreeRemoval = false,
  }) async {}
}
