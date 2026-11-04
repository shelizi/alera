part of 'workbench_controller_test.dart';

void _registerWorkbenchControllerExplorerRevealTests() {
  test(
    'MCP Explorer ACK cannot succeed before visual reveal or after replacement',
    () async {
      final sink = _harness.container.read(
        workspaceExplorerRevealControllerProvider.notifier,
      );
      sink.reveal(workspaceId: 'ws', relativePath: 'first.dart');
      final first = _harness.container.read(
        workspaceExplorerRevealControllerProvider,
      )!;
      final firstCompletion = sink.waitForConsumed(first);
      sink.reveal(workspaceId: 'ws', relativePath: 'second.dart');
      expect(await firstCompletion, isFalse);
      final second = _harness.container.read(
        workspaceExplorerRevealControllerProvider,
      )!;
      expect(
        await sink.waitForConsumed(
          second,
          timeout: const Duration(milliseconds: 10),
        ),
        isFalse,
      );
      sink.consume(second);
      expect(await sink.waitForConsumed(second), isTrue);
    },
  );

  test('revealInExplorer switches to explorer and queues the path', () async {
    await _controller.bootstrap();
    final workspace = await _selectMainWorkspace(_controller, _harness);
    _controller.setContextPanelTab(.gitDiff);
    await _flush();

    _controller.revealInExplorer(
      workspace: workspace,
      relativePath: 'lib/src/dirty.dart',
    );
    await _flush();

    expect(
      _controller.state.viewPrefs.activeContextPanelTab,
      WorkbenchContextPanelTab.explorer,
    );
    expect(_controller.state.viewPrefs.rightSidebarVisible, isTrue);
    final request = _harness.container.read(
      workspaceExplorerRevealControllerProvider,
    );
    expect(request?.workspaceId, workspace.id);
    expect(request?.relativePath, 'lib/src/dirty.dart');
  });
}
