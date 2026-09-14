part of 'settings_dialog_test.dart';

void _registerSettingsDialogAgentsTests() {
  testWidgets('edits agent status notification and awake settings', (
    tester,
  ) async {
    Future<void> toggleSwitchRow(String title) async {
      final toggle = find.descendant(
        of: find.ancestor(
          of: find.text(title),
          matching: find.byType(SettingsSwitchRow),
        ),
        matching: find.byType(Switch),
      );
      await tester.ensureVisible(toggle);
      await tester.pump();
      await tester.tap(toggle);
      await tester.pump(const Duration(milliseconds: 50));
    }

    final container = await _pumpSettingsDialog(tester);

    // Agent hooks and behavior settings live in the Agents section.
    await tester.tap(find.text('Agents').first);
    await tester.pump();

    expect(find.text('Extra Skills'), findsWidgets);
    expect(find.text('Agent Profiles Skill'), findsOneWidget);
    expect(find.text('Agent Executables'), findsOneWidget);
    expect(find.text('Devin Executable'), findsOneWidget);
    expect(find.text('Git Bash Executable'), findsOneWidget);

    final devinExecutableField = find.descendant(
      of: find.byKey(const ValueKey<String>('agent-executable-path-devin')),
      matching: find.byType(TextField),
    );
    await tester.ensureVisible(devinExecutableField);
    await tester.enterText(devinExecutableField, r'C:\Tools\devin.exe');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pump(const Duration(milliseconds: 50));
    expect(
      container
          .read(settingsControllerProvider)
          .agents
          .executablePathFor('devin'),
      r'C:\Tools\devin.exe',
    );

    final gitBashField = find.descendant(
      of: find.byKey(const ValueKey<String>('git-bash-executable-path')),
      matching: find.byType(TextField),
    );
    await tester.ensureVisible(gitBashField);
    await tester.enterText(gitBashField, r'D:\PortableGit\git-bash.exe');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pump(const Duration(milliseconds: 50));
    expect(
      container.read(settingsControllerProvider).agents.gitBashExecutablePath,
      r'D:\PortableGit\git-bash.exe',
    );

    await tester.ensureVisible(find.text('Agent Status Notifications'));
    await tester.pump();

    const hookLabels = <String>[
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
      'Devin Hooks',
      'fx Status',
    ];
    for (final label in hookLabels) {
      expect(find.text(label), findsOneWidget);
    }
    expect(find.text('Show Tab Titles in Sidebar'), findsOneWidget);
    expect(find.text('Agent Status Notifications'), findsOneWidget);
    expect(
      find.text('Keep Computer Awake While Agents Are Working'),
      findsOneWidget,
    );

    for (final label in hookLabels) {
      await toggleSwitchRow(label);
    }
    await toggleSwitchRow('Show Tab Titles in Sidebar');
    await toggleSwitchRow('Agent Status Notifications');
    await toggleSwitchRow('Agent Finished Notifications');
    await toggleSwitchRow('Keep Computer Awake While Agents Are Working');

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
      hooks.devin,
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
