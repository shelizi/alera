part of 'workbench_controller_test.dart';

void _registerWorkbenchControllerLayoutPersistenceTests() {
  test(
    'bootstrap does not rewrite unchanged layouts for 15 workspaces',
    () async {
      final projects = <Project>[_harness.project];
      for (var index = 1; index < 15; index += 1) {
        projects.add(
          await _harness.addProject('project-$index', 'Project $index'),
        );
      }
      for (var index = 0; index < projects.length; index += 1) {
        final project = projects[index];
        final workspace = Workspace(
          id: 'workspace-$index',
          projectId: project.id,
          name: 'Main',
          branch: 'main',
          path: project.repoPath,
          createdAt: .utc(2026, 5, 22),
          updatedAt: .utc(2026, 5, 22),
          kind: .main,
          status: .active,
        );
        await _harness.workbenchRepository.upsertWorkspace(workspace);
        await _harness.workbenchRepository.upsertWorkbenchLayout(
          .single(workspaceId: workspace.id, tabIds: const []),
        );
      }
      _harness.workbenchRepository.upsertWorkbenchLayoutCalls = 0;

      await _controller.bootstrap();
      await _flushUntil(
        () => List<int>.generate(projects.length, (index) => index).every(
          (index) =>
              _harness.workbenchRepository.hasTabWatcher('workspace-$index'),
        ),
        attempts: 100,
      );
      await Future.pause(const Duration(milliseconds: 10));

      expect(_harness.workbenchRepository.upsertWorkbenchLayoutCalls, 0);
    },
  );

  test('layout bootstrap persists a missing layout exactly once', () async {
    await _controller.bootstrap();
    final workspace = await _selectMainWorkspace(_controller, _harness);
    await _harness.workbenchRepository.removeWorkbenchLayout(workspace.id);
    _controller.state = _controller.state.copyWith(
      layoutByWorkspace: <String, WorkbenchLayout>{},
    );
    _harness.workbenchRepository.upsertWorkbenchLayoutCalls = 0;

    _harness.workbenchRepository.emitTabs(workspace.id);
    await _flushUntil(() => _controller.state.layoutFor(workspace.id) != null);

    expect(_harness.workbenchRepository.upsertWorkbenchLayoutCalls, 1);
  });

  test('unchanged tab refresh skips layout persistence', () async {
    await _controller.bootstrap();
    final workspace = await _selectMainWorkspace(_controller, _harness);
    _harness.workbenchRepository.upsertWorkbenchLayoutCalls = 0;

    _harness.workbenchRepository.emitTabs(workspace.id);
    await _flush();

    expect(_harness.workbenchRepository.upsertWorkbenchLayoutCalls, 0);
  });

  test(
    'newer layout persistence wins when an older write completes late',
    () async {
      await _controller.bootstrap();
      final workspace = await _selectMainWorkspace(_controller, _harness);
      final groupId = _controller.state.layoutFor(workspace.id)!.activeGroupId;
      await _controller.splitWorkbenchGroupWithTerminal(
        workspace: workspace,
        groupId: groupId,
        zone: .right,
      );
      await _flush();

      final baselineCalls =
          _harness.workbenchRepository.upsertWorkbenchLayoutCalls;
      final started = Completer<void>();
      final release = Completer<void>();
      final completed = Completer<void>();
      _harness.workbenchRepository.blockNextWorkbenchLayoutUpsert(
        started: started,
        release: release,
        completed: completed,
      );

      _controller.updateWorkbenchSplitRatio(
        workspaceId: workspace.id,
        nodePath: const <int>[],
        ratio: 0.3,
      );
      await started.future;
      _controller.updateWorkbenchSplitRatio(
        workspaceId: workspace.id,
        nodePath: const <int>[],
        ratio: 0.7,
      );
      expect(_controller.state.layoutFor(workspace.id)!.root.ratio, 0.7);

      release.complete();
      await completed.future;
      await _flushUntil(
        () =>
            _harness.workbenchRepository.upsertWorkbenchLayoutCalls >=
            baselineCalls + 2,
      );

      expect(
        _harness.workbenchRepository
            .peekWorkbenchLayout(workspace.id)!
            .root
            .ratio,
        0.7,
      );
    },
  );

  test('background layout failures are recorded instead of escaping', () async {
    await _controller.bootstrap();
    final workspace = await _selectMainWorkspace(_controller, _harness);
    _harness.workbenchRepository.upsertWorkbenchLayoutError = StateError(
      'layout connection closed',
    );

    _controller.updateWorkbenchSplitRatio(
      workspaceId: workspace.id,
      nodePath: const <int>[],
      ratio: 0.6,
    );
    await _flushUntil(() => _controller.state.error != null);

    expect(_controller.state.error, contains('layout connection closed'));
  });
}
