part of 'settings_dialog_test.dart';

void _registerSettingsDialogAgentsTests() {
  testWidgets('edits agent status notification and awake settings', (
    tester,
  ) async {
    final container = await _pumpSettingsDialog(tester);

    // Agent hooks and behavior settings live in the Agents section.
    await tester.tap(find.text('Agents').first);
    await tester.pump();

    expect(find.text('Extra Skills'), findsWidgets);
    expect(find.text('Agent Profiles Skill'), findsOneWidget);

    await tester.ensureVisible(find.text('Agent Status Notifications'));
    await tester.pump();

    for (final label in const <String>[
      'Codex Hooks',
      'Claude Code Hooks',
      'GitHub Copilot Hooks',
      'Cursor Hooks',
      'Antigravity Hooks',
      'OpenCode Hooks',
      'OpenCode 2 Hooks',
      'Pi Hooks',
      'Amp Hooks',
      'Grok Build Hooks',
      'fx Status',
    ]) {
      expect(find.text(label), findsOneWidget);
    }
    expect(find.text('Show Tab Titles in Sidebar'), findsOneWidget);
    expect(find.text('Agent Status Notifications'), findsOneWidget);
    expect(
      find.text('Keep Computer Awake While Agents Are Working'),
      findsOneWidget,
    );

    await tester.ensureVisible(find.text('Codex Hooks'));
    await tester.pump();
    await tester.tap(find.byType(Switch).at(0));
    await tester.pump(const Duration(milliseconds: 50));
    for (final entry in const <({String label, int switchIndex})>[
      (label: 'Claude Code Hooks', switchIndex: 1),
      (label: 'GitHub Copilot Hooks', switchIndex: 2),
      (label: 'Cursor Hooks', switchIndex: 3),
      (label: 'Antigravity Hooks', switchIndex: 4),
      (label: 'OpenCode Hooks', switchIndex: 5),
      (label: 'OpenCode 2 Hooks', switchIndex: 6),
      (label: 'Pi Hooks', switchIndex: 7),
      (label: 'Amp Hooks', switchIndex: 8),
      (label: 'Grok Build Hooks', switchIndex: 9),
      (label: 'fx Status', switchIndex: 10),
    ]) {
      await tester.ensureVisible(find.text(entry.label));
      await tester.pump();
      await tester.tap(find.byType(Switch).at(entry.switchIndex));
      await tester.pump(const Duration(milliseconds: 50));
    }
    await tester.ensureVisible(find.text('Show Tab Titles in Sidebar'));
    await tester.pump();
    await tester.tap(find.byType(Switch).at(11));
    await tester.pump(const Duration(milliseconds: 50));
    await tester.ensureVisible(find.text('Agent Status Notifications'));
    await tester.pump();
    await tester.tap(find.byType(Switch).at(12));
    await tester.pump(const Duration(milliseconds: 50));
    await tester.ensureVisible(find.text('Agent Finished Notifications'));
    await tester.pump();
    await tester.tap(find.byType(Switch).at(13));
    await tester.pump(const Duration(milliseconds: 50));
    await tester.ensureVisible(
      find.text('Keep Computer Awake While Agents Are Working'),
    );
    await tester.pump();
    await tester.tap(find.byType(Switch).at(14));
    await tester.pump(const Duration(milliseconds: 50));

    final hooks = container
        .read(settingsControllerProvider)
        .agents
        .agentStatusHooks;
    expect(<bool>[
      hooks.codex,
      hooks.claude,
      hooks.copilot,
      hooks.cursor,
      hooks.agy,
      hooks.opencode,
      hooks.opencode2,
      hooks.pi,
      hooks.amp,
      hooks.grok,
      hooks.fx,
    ], everyElement(isTrue));
    final behavior = container.read(settingsControllerProvider).agents;
    expect(<bool>[
      behavior.showTabTitlesInSidebar,
      behavior.agentStatusNotificationsEnabled,
      behavior.agentStatusFinishedNotificationsEnabled,
      behavior.keepComputerAwakeWhileAgentsWork,
    ], everyElement(isTrue));

    await tester.enterText(find.byType(TextField).first, 'notification');
    await tester.pump();

    expect(find.text('Agent Status Notifications'), findsOneWidget);

    await tester.enterText(find.byType(TextField).first, 'awake');
    await tester.pump();

    expect(
      find.text('Keep Computer Awake While Agents Are Working'),
      findsOneWidget,
    );
    expect(find.text('Keep Computer Awake'), findsNothing);
  });
}
