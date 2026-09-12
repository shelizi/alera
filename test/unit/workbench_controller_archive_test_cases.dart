part of 'workbench_controller_test.dart';

void _registerWorkbenchControllerArchiveTests() {
  test(
    'archives a linked workspace: tabs close and archivedAt persists',
    () async {
      await _controller.bootstrap();
      final main = await _selectMainWorkspace(_controller, _harness);
      final linked = await _harness.workbenchRepository.upsertWorkspace(
        main.copyWith(
          id: 'linked-archive',
          kind: WorkspaceKind.linked,
          name: 'Feature',
          path: p.join(_harness.tempDir.path, 'repo', 'linked-archive'),
        ),
      );
      await _flushUntil(
        () => _controller.state
            .workspacesFor(main.projectId)
            .any((workspace) => workspace.id == linked.id),
      );
      await _controller.selectWorkspace(
        project: _harness.project,
        workspace: linked,
      );
      await _flush();
      expect(_controller.state.tabsFor(linked.id), isNotEmpty);

      await _controller.archiveWorkspace(linked);
      await _flush();

      final stored = await _harness.workbenchRepository.findWorkspaceById(
        linked.id,
      );
      expect(stored?.isArchived, isTrue);
      expect(stored?.archivedAt, isNotNull);
      // Archiving is a display state: the row stays active for listings.
      expect(stored?.status, WorkspaceStatus.active);
      final inState = _controller.state
          .workspacesFor(main.projectId)
          .firstWhere((workspace) => workspace.id == linked.id);
      expect(inState.isArchived, isTrue);
      // The sleep half of archiving closed the tabs and dropped the selection.
      expect(_controller.state.tabsFor(linked.id), isEmpty);
      expect(_controller.state.activeWorkspaceId, isNull);
      expect(_controller.state.error, isNull);
    },
  );

  test(
    'restore clears archivedAt, bumps updatedAt and reopens the workspace',
    () async {
      await _controller.bootstrap();
      final main = await _selectMainWorkspace(_controller, _harness);
      final linked = await _harness.workbenchRepository.upsertWorkspace(
        main.copyWith(
          id: 'linked-restore',
          kind: WorkspaceKind.linked,
          name: 'Stale',
          archivedAt: DateTime.utc(2026, 8, 1),
        ),
      );
      await _flushUntil(
        () => _controller.state
            .workspacesFor(main.projectId)
            .any((workspace) => workspace.id == linked.id),
      );
      final archived = _controller.state
          .workspacesFor(main.projectId)
          .firstWhere((workspace) => workspace.id == linked.id);
      expect(archived.isArchived, isTrue);

      await _controller.restoreWorkspace(archived, select: true);
      await _flushUntil(() => _controller.state.activeWorkspaceId == linked.id);

      final stored = await _harness.workbenchRepository.findWorkspaceById(
        linked.id,
      );
      expect(stored?.isArchived, isFalse);
      expect(stored?.archivedAt, isNull);
      expect(stored!.updatedAt.isAfter(archived.updatedAt), isTrue);
      expect(_controller.state.activeWorkspaceId, linked.id);
      expect(_controller.state.tabsFor(linked.id), isNotEmpty);
      expect(_controller.state.error, isNull);
    },
  );

  test('archiveWorkspace ignores the main workspace', () async {
    await _controller.bootstrap();
    final main = await _selectMainWorkspace(_controller, _harness);

    await _controller.archiveWorkspace(main);

    final stored = await _harness.workbenchRepository.findWorkspaceById(
      main.id,
    );
    expect(stored?.isArchived, isFalse);
    expect(_controller.state.activeWorkspaceId, main.id);
    expect(_controller.state.error, isNull);
  });

  test(
    'archive failure surfaces the error without flagging the workspace',
    () async {
      await _controller.bootstrap();
      final main = await _selectMainWorkspace(_controller, _harness);
      final linked = await _harness.workbenchRepository.upsertWorkspace(
        main.copyWith(id: 'linked-fails', kind: WorkspaceKind.linked),
      );
      await _flushUntil(
        () => _controller.state
            .workspacesFor(main.projectId)
            .any((workspace) => workspace.id == linked.id),
      );
      _harness.workbenchRepository.upsertWorkspaceError = StateError(
        'archive failed',
      );

      await expectLater(_controller.archiveWorkspace(linked), throwsStateError);

      expect(_controller.state.error, contains('archive failed'));
      final stored = await _harness.workbenchRepository.findWorkspaceById(
        linked.id,
      );
      expect(stored?.isArchived, isFalse);
    },
  );
}
