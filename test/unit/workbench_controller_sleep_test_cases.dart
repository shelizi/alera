part of 'workbench_controller_test.dart';

void _registerWorkbenchControllerSleepTests() {
  test(
    'sleep removes every tab and layout then deselects the workspace',
    () async {
      await _controller.bootstrap();
      final workspace = await _selectMainWorkspace(_controller, _harness);
      await _controller.openEditorTab(
        workspace: workspace,
        relativePath: 'notes.txt',
      );
      await _flush();

      final terminal = _controller.state
          .tabsFor(workspace.id)
          .firstWhere((tab) => tab.kind == WorkspaceTabKind.terminal);
      final editor = _controller.state
          .tabsFor(workspace.id)
          .firstWhere((tab) => tab.kind == WorkspaceTabKind.editor);
      _harness.terminalRuntime.sessionFor(workspace: workspace, tab: terminal);
      final registry = _harness.container.read(editorSessionRegistryProvider);
      registry.documentFor(editor.id)
        ..acceptLoaded(
          native_files.WorkspaceEditorTextFile(
            rawContent: 'original',
            displayContent: 'original',
            contentToken: 'sleep-token',
            modifiedMillis: 0,
            size: .zero,
            encoding: native_files.WorkspaceTextEncoding.utf8,
          ),
        )
        ..updateCurrentText('unsaved');
      expect(registry.isDirty(editor.id), isTrue);

      expect(
        _controller.state.tabsFor(workspace.id).map((tab) => tab.kind),
        <WorkspaceTabKind>[WorkspaceTabKind.terminal, WorkspaceTabKind.editor],
      );
      expect(_controller.state.layoutFor(workspace.id), isNotNull);

      await _controller.sleepWorkspace(workspace);
      await _flush();

      expect(_controller.state.tabsFor(workspace.id), isEmpty);
      expect(_controller.state.layoutFor(workspace.id), isNull);
      expect(_controller.state.activeTabIdByWorkspace[workspace.id], isNull);
      expect(_controller.state.activeWorkspace, isNull);
      expect(
        _harness.terminalRuntime.closedWorkspaceIds,
        contains(workspace.id),
      );
      expect(_harness.terminalRuntime.sessions, isEmpty);
      expect(registry.isDirty(editor.id), isFalse);
      expect(
        await _harness.workbenchRepository.findWorkbenchLayout(workspace.id),
        isNull,
      );
      expect(
        await _harness.workbenchRepository.listWorkspaceTabs(workspace.id),
        isEmpty,
      );
    },
  );
}
