part of 'workbench_controller_test.dart';

void _registerWorkbenchControllerFailureSurfaceTests() {
  test('surfaces project and workspace action failures in state', () async {
    await expectLater(_controller.addLocalProject(path: ''), throwsStateError);
    expect(_controller.state.error, contains('Project path must not be empty'));

    await expectLater(
      _controller.cloneProject(
        gitUrl: 'https://example.com/repo.git',
        destinationPath: '',
      ),
      throwsStateError,
    );
    expect(
      _controller.state.error,
      contains('Destination path must not be empty'),
    );

    await expectLater(
      _controller.renameProject(projectId: 'missing', name: 'Renamed'),
      throwsStateError,
    );
    expect(_controller.state.error, contains('project not found'));

    _harness.projectRepository.removeError = StateError('cannot remove');
    await expectLater(
      _controller.removeProject(_harness.project.id),
      throwsStateError,
    );
    expect(_controller.state.error, contains('cannot remove'));

    await expectLater(
      _controller.createWorkspace(
        project: _harness.project,
        sourceBranch: '',
        newBranchName: 'feature/failure',
      ),
      throwsA(isA<WorkspaceException>()),
    );
    expect(_controller.state.error, contains('Source branch is required'));

    await expectLater(
      _controller.renameWorkspace(workspaceId: 'missing', name: 'Renamed'),
      throwsA(isA<WorkspaceException>()),
    );
    expect(_controller.state.error, contains('Workspace not found'));

    await _controller.bootstrap();
    final mainWorkspace = await _selectMainWorkspace(_controller, _harness);

    await expectLater(
      _controller.deleteWorkspace(
        project: _harness.project,
        workspace: mainWorkspace,
      ),
      throwsA(isA<WorkspaceException>()),
    );
    expect(
      _controller.state.error,
      contains('The main workspace cannot be removed'),
    );
  });

  test('surfaces tab and layout failures in state', () async {
    await _controller.bootstrap();
    final workspace = await _selectMainWorkspace(_controller, _harness);
    final activeTab = _controller.state.activeWorkspaceTab!;

    _harness.workbenchRepository.upsertWorkspaceTabError = StateError(
      'cannot create tab',
    );
    await expectLater(
      _controller.createTerminalTab(workspace),
      throwsStateError,
    );
    expect(_controller.state.error, contains('cannot create tab'));
    _harness.workbenchRepository.upsertWorkspaceTabError = null;

    _harness.workbenchRepository.removeWorkspaceTabError = StateError(
      'cannot close tab',
    );
    await expectLater(
      _controller.closeWorkspaceTabs(
        workspace: workspace,
        tabIds: <String>[activeTab.id],
      ),
      throwsStateError,
    );
    expect(_controller.state.error, contains('cannot close tab'));
    _harness.workbenchRepository.removeWorkspaceTabError = null;

    await expectLater(
      _controller.renameWorkspaceTab(tabId: activeTab.id, title: '   '),
      throwsStateError,
    );
    expect(_controller.state.error, contains('Tab title must not be empty'));

    final firstGroupId = _controller.state
        .layoutFor(workspace.id)!
        .activeGroupId;
    final splitTab = await _controller.splitWorkbenchGroupWithTerminal(
      workspace: workspace,
      groupId: firstGroupId,
      zone: .right,
    );
    await _flush();

    final splitLayout = _controller.state.layoutFor(workspace.id)!;
    final splitGroupId = splitLayout.groupIdForTab(splitTab.id)!;
    final targetGroupId = splitLayout.paneGroupIds.firstWhere(
      (groupId) => groupId != splitGroupId,
    );

    _harness.workbenchRepository.upsertWorkbenchLayoutError = StateError(
      'cannot persist layout',
    );
    await expectLater(
      _controller.moveWorkspaceTab(
        workspaceId: workspace.id,
        tabId: splitTab.id,
        targetGroupId: targetGroupId,
        zone: .center,
      ),
      throwsStateError,
    );
    expect(_controller.state.error, contains('cannot persist layout'));

    await expectLater(
      _controller.mergeWorkbenchGroupIntoSibling(
        workspaceId: workspace.id,
        groupId: splitGroupId,
      ),
      throwsStateError,
    );
    expect(_controller.state.error, contains('cannot persist layout'));
    _harness.workbenchRepository.upsertWorkbenchLayoutError = null;

    await _controller.mergeWorkbenchGroupIntoSibling(
      workspaceId: workspace.id,
      groupId: splitGroupId,
    );
    await _flush();
    expect(_controller.state.error, isNull);

    _harness.workbenchRepository.upsertWorkspaceTabError = StateError(
      'cannot split tab',
    );
    await expectLater(
      _controller.splitWorkbenchGroupWithTerminal(
        workspace: workspace,
        groupId: targetGroupId,
        zone: .down,
      ),
      throwsStateError,
    );
    expect(_controller.state.error, contains('cannot split tab'));
  });
}
