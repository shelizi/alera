part of 'settings_dialog_test.dart';

void _registerSettingsDialogAiAssistTests() {
  testWidgets('keeps AI Assist reasoning selections isolated per operation', (
    tester,
  ) async {
    final container = await _pumpSettingsDialog(
      tester,
      initialSectionId: 'aiAssist',
    );
    // Jump via the subsection chip so the commit-message reasoning control is
    // in the content viewport before the dropdown opens.
    await tester.tap(find.text('Commit Messages').first);
    await tester.pumpAndSettle();

    final commitReasoning = find.byKey(
      const ValueKey<String>('ai-assist-commitMessage-reasoning-low'),
    );
    await tester.ensureVisible(commitReasoning);
    await tester.pump();
    await tester.tap(commitReasoning);
    await tester.pumpAndSettle();
    await tester.tap(find.text('High').last);
    await tester.pump(const Duration(milliseconds: 50));

    final settings = container.read(settingsControllerProvider).aiAssist;
    expect(
      settings.selectedThinkingByOperation[AiAssistOperation
          .commitMessage]?['gpt-5.5'],
      'high',
    );
    expect(settings.thinkingForOperation(.commitMessage, 'gpt-5.5'), 'high');
    expect(
      settings.thinkingForOperation(.pullRequestDetails, 'gpt-5.5'),
      isNull,
    );
    expect(settings.thinkingForModel('gpt-5.5'), isNull);
    expect(
      find.byKey(
        const ValueKey<String>('ai-assist-pullRequestDetails-reasoning-low'),
      ),
      findsOneWidget,
    );
  });

  testWidgets('edits and resets AI Assist settings', (tester) async {
    final container = await _pumpSettingsDialog(tester);
    await _selectAiAssistSectionByNav(tester);

    expect(find.text('AI Assist'), findsWidgets);
    expect(find.text('Agent'), findsWidgets);
    expect(find.text('Model'), findsWidgets);
    expect(find.text('Commit Messages'), findsWidgets);
    expect(find.text('Pull Request Details'), findsWidgets);
    expect(find.text('Workspace Identity'), findsWidgets);
    expect(
      tester
          .widgetList<MouseRegion>(
            find.descendant(
              of: find.byKey(const ValueKey<String>('ai-assist-agent-codex')),
              matching: find.byType(MouseRegion),
            ),
          )
          .first
          .cursor,
      SystemMouseCursors.click,
    );
    expect(
      tester
          .widgetList<MouseRegion>(
            find.descendant(
              of: find.byKey(
                const ValueKey<String>('ai-assist-model-codex-gpt-5.5'),
              ),
              matching: find.byType(MouseRegion),
            ),
          )
          .first
          .cursor,
      SystemMouseCursors.click,
    );
    expect(
      tester
          .widgetList<MouseRegion>(
            find.descendant(
              of: find.byKey(const ValueKey<String>('ai-assist-thinking-low')),
              matching: find.byType(MouseRegion),
            ),
          )
          .first
          .cursor,
      SystemMouseCursors.click,
    );

    await tester.tap(
      find.byKey(const ValueKey<String>('ai-assist-agent-codex')),
    );
    await tester.pumpAndSettle();
    expect(find.byType(AleraDropdownEntry<AiAssistAgent>), findsWidgets);
    expect(
      tester
          .widgetList<AleraDropdownEntry<AiAssistAgent>>(
            find.byType(AleraDropdownEntry<AiAssistAgent>),
          )
          .first
          .enabled,
      isTrue,
    );
    await tester.tap(find.text('Antigravity').last);
    await tester.pump(const Duration(milliseconds: 50));

    expect(
      container.read(settingsControllerProvider).aiAssist.agent,
      AiAssistAgent.agy,
    );
    await tester.pump(const Duration(milliseconds: 50));
    expect(
      container
          .read(settingsControllerProvider)
          .aiAssist
          .discoveredModelsFor(.agy)
          .map((model) => model.id),
      contains('gpt-5.5'),
    );

    await tester.tap(find.text('Reset AI Assist'));
    await tester.pump(const Duration(milliseconds: 50));

    expect(
      container.read(settingsControllerProvider).aiAssist.agent,
      AiAssistSettings.defaults.agent,
    );
    expect(find.text('Codex').last, findsOneWidget);

    await tester.ensureVisible(find.text('Commit Messages').last);
    expect(find.text('Pull Request Details'), findsWidgets);
    expect(find.text('Branch Names'), findsNothing);
    await tester.pump();
    await tester.enterText(
      find.widgetWithText(TextField, 'Optional instructions').first,
      'Use conventional commits.',
    );
    await _selectTerminalSectionByNav(tester);
    await tester.pump(const Duration(milliseconds: 50));

    expect(
      container
          .read(settingsControllerProvider)
          .aiAssist
          .instructionsFor(.commitMessage),
      'Use conventional commits.',
    );

    await container
        .read(settingsControllerProvider.notifier)
        .resetAiAssistSettings();
    await tester.pump(const Duration(milliseconds: 50));

    expect(
      container.read(settingsControllerProvider).aiAssist.agent,
      AiAssistSettings.defaults.agent,
    );

    await _selectAiAssistSectionByNav(tester);
    await tester.pump();
    await tester.enterText(
      find.widgetWithText(TextField, 'Optional instructions').first,
      'Draft that should not survive reset.',
    );
    await tester.ensureVisible(
      find.text('Reset AI Assist', skipOffstage: false),
    );
    await tester.pump();
    await tester.tap(find.text('Reset AI Assist'));
    await tester.pump(const Duration(milliseconds: 50));
    await _selectTerminalSectionByNav(tester);
    await tester.pump(const Duration(milliseconds: 50));

    expect(
      container
          .read(settingsControllerProvider)
          .aiAssist
          .instructionsFor(.commitMessage),
      isEmpty,
    );
  });
}
