part of 'workbench_controller_test.dart';

/// Tabs that launch a locally installed agent CLI keep their launch command on
/// the record so a PTY remint re-enters the agent.
void _registerWorkbenchControllerAgentTabTests() {
  test(
    'createAgentTab creates a spawning terminal tab for the agent CLI',
    () async {
      await _controller.bootstrap();
      final workspace = await _selectMainWorkspace(_controller, _harness);

      final tab = await _controller.createAgentTab(
        workspace,
        agentType: AgentType.claude,
      );

      expect(tab.title, 'Claude Code');
      expect(tab.initialCommand, 'claude');
      expect(tab.spawnOnCreate, isTrue);
      expect(tab.initialCommandOnce, isFalse);
      expect(tab.hasManualTitle, isTrue);
      expect(tab.kind, WorkspaceTabKind.terminal);
      expect(_controller.state.tabsFor(workspace.id), contains(tab));
      expect(_controller.state.layoutFor(workspace.id)?.activeTabId, tab.id);
    },
  );

  test('createAgentTab places the tab in the requested pane group', () async {
    await _controller.bootstrap();
    final workspace = await _selectMainWorkspace(_controller, _harness);
    final firstGroupId = _controller.state
        .layoutFor(workspace.id)!
        .activeGroupId;
    final splitTab = await _controller.splitWorkbenchGroupWithTerminal(
      workspace: workspace,
      groupId: firstGroupId,
      zone: .right,
    );
    final splitGroupId = _controller.state
        .layoutFor(workspace.id)!
        .groupIdForTab(splitTab.id)!;

    final tab = await _controller.createAgentTab(
      workspace,
      agentType: AgentType.codex,
      targetGroupId: splitGroupId,
    );

    final layout = _controller.state.layoutFor(workspace.id)!;
    expect(tab.initialCommand, 'codex');
    expect(layout.groupIdForTab(tab.id), splitGroupId);
    expect(layout.activeGroupId, splitGroupId);
  });
}
