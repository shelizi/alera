part of 'workbench_controller_test.dart';

void _registerWorkbenchControllerGitHistoryTabTests() {
  test('openGitHistoryTab creates and selects the commit graph tab', () async {
    await _controller.bootstrap();
    final workspace = await _selectMainWorkspace(_controller, _harness);

    final tab = await _controller.openGitHistoryTab(workspace: workspace);
    await _flush();

    expect(tab.kind, WorkspaceTabKind.gitHistory);
    expect(tab.title, 'Commit Graph');
    expect(tab.gitHistoryAllBranches, isTrue);
    expect(_controller.state.activeWorkspaceTab?.id, tab.id);
    expect(
      _controller.state
          .tabsFor(workspace.id)
          .where((tab) => tab.kind == WorkspaceTabKind.gitHistory),
      hasLength(1),
    );
  });

  test(
    'openGitHistoryTab refocuses the existing tab for the same root',
    () async {
      await _controller.bootstrap();
      final workspace = await _selectMainWorkspace(_controller, _harness);

      final first = await _controller.openGitHistoryTab(
        workspace: workspace,
        gitDiffRoot: './packages\\app',
      );
      final otherRoot = await _controller.openGitHistoryTab(
        workspace: workspace,
        gitDiffRoot: 'packages/tools',
      );
      final reopened = await _controller.openGitHistoryTab(
        workspace: workspace,
        gitDiffRoot: 'packages/app',
      );
      await _flush();

      expect(first.gitDiffRoot, 'packages/app');
      expect(otherRoot.id, isNot(first.id));
      expect(reopened.id, first.id);
      expect(_controller.state.activeWorkspaceTab?.id, first.id);
      expect(
        _controller.state
            .tabsFor(workspace.id)
            .where((tab) => tab.kind == WorkspaceTabKind.gitHistory),
        hasLength(2),
      );
    },
  );

  test('setGitHistoryAllBranches persists the toggle on the tab', () async {
    await _controller.bootstrap();
    final workspace = await _selectMainWorkspace(_controller, _harness);
    final tab = await _controller.openGitHistoryTab(workspace: workspace);
    await _flush();

    await _controller.setGitHistoryAllBranches(
      tabId: tab.id,
      allBranches: false,
    );
    await _flush();

    final stored = _controller.state
        .tabsFor(workspace.id)
        .singleWhere((candidate) => candidate.id == tab.id);
    expect(stored.gitHistoryAllBranches, isFalse);
    expect(
      _harness.workbenchRepository.findWorkspaceTabById(tab.id),
      completion(
        isA<WorkspaceTabRecord>().having(
          (tab) => tab.gitHistoryAllBranches,
          'gitHistoryAllBranches',
          isFalse,
        ),
      ),
    );
  });
}
