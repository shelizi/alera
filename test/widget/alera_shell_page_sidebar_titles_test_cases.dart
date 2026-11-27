part of 'alera_shell_page_test.dart';

void _registerAleraShellSidebarTitleTests() {
  testWidgets('sidebar agent rows can show tab titles and regenerate them', (
    tester,
  ) async {
    await _pumpShell(
      tester,
      state: _stackedWorkbenchState().copyWith(
        tabsByWorkspace: <String, List<WorkspaceTabRecord>>{
          'workspace-1': <WorkspaceTabRecord>[
            WorkspaceTabRecord(
              id: 'tab-1',
              workspaceId: 'workspace-1',
              title: 'Map Monetization',
              createdAt: DateTime.utc(2026, 5, 22),
              updatedAt: DateTime.utc(2026, 5, 22),
              payload: const <String, Object?>{
                'agentTitleSource': 'generated',
                'manualTitle': true,
              },
            ),
            WorkspaceTabRecord(
              id: 'tab-2',
              workspaceId: 'workspace-1',
              title: 'Terminal 2',
              createdAt: DateTime.utc(2026, 5, 22),
              updatedAt: DateTime.utc(2026, 5, 22),
            ),
          ],
        },
      ),
      settings: AleraSettings.defaults.copyWith(
        agents: const AgentSettings(showTabTitlesInSidebar: true),
      ),
      agentTitlesAvailable: true,
      agentStatuses: <String, AgentStatusEntry>{
        'tab-1': _agentStatusEntry(
          terminalSessionId: 'tab-1',
          workspaceId: 'workspace-1',
          tabId: 'tab-1',
          state: .waiting,
          lastAssistantMessage: '**paymentBroker**',
        ),
        'tab-2': _agentStatusEntry(
          terminalSessionId: 'tab-2',
          workspaceId: 'workspace-1',
          tabId: 'tab-2',
          state: .waiting,
          lastAssistantMessage: 'Ready to continue',
        ),
      },
    );

    await tester.tap(find.byTooltip('Show Agent Runs'));
    await tester.pumpAndSettle();

    expect(
      find.descendant(
        of: find.byType(ProjectWorkbenchSidebar),
        matching: find.text('Map Monetization'),
      ),
      findsOneWidget,
    );
    expect(
      find.descendant(
        of: find.byType(ProjectWorkbenchSidebar),
        matching: find.text('**paymentBroker**'),
      ),
      findsNothing,
    );
    expect(
      find.descendant(
        of: find.byType(ProjectWorkbenchSidebar),
        matching: find.text('Ready to continue'),
      ),
      findsNothing,
    );

    await tester.tapAt(
      tester.getCenter(
        find.descendant(
          of: find.byType(ProjectWorkbenchSidebar),
          matching: find.text('Map Monetization'),
        ),
      ),
      buttons: kSecondaryMouseButton,
    );
    await tester.pumpAndSettle();

    expect(find.text('Regenerate Title'), findsOneWidget);
    final entry = tester.widget<AleraDropdownEntry<Object?>>(
      find.ancestor(
        of: find.text('Regenerate Title'),
        matching: find.byWidgetPredicate(
          (widget) => widget is AleraDropdownEntry,
        ),
      ),
    );
    expect((entry.leading! as Icon).size, 16);
  });

  for (final enabled in <bool>[false, true]) {
    testWidgets(
      'Agent task sidebar summary is ${enabled ? 'enabled' : 'disabled'}',
      (tester) async {
        const goal = 'Finish the complete GPUI settings parity task';
        const report = 'Shared Host controls completed; visual review pending';
        const summary = 'AI summary of shared Host controls';
        const overall = 'Cumulative progress report';
        await _pumpShell(
          tester,
          state: _stackedWorkbenchState(),
          settings: AleraSettings.defaults.copyWith(
            agents: AgentSettings(showTaskSummariesInSidebar: enabled),
          ),
          agentStatuses: <String, AgentStatusEntry>{
            'tab-1': _agentStatusEntry(
              terminalSessionId: 'tab-1',
              workspaceId: 'workspace-1',
              tabId: 'tab-1',
              state: .working,
              prompt: 'short user prompt',
              toolName: 'Read',
              toolInput: 'settings.json',
              lastAssistantMessage: 'Previous reply',
              contextOverview: <String, dynamic>{
                'goal': goal,
                'progress': report,
                'reportSummary': summary,
                'overallSummary': overall,
                'timeline': <Object?>[],
              },
            ),
            'tab-2': _agentStatusEntry(
              terminalSessionId: 'tab-2',
              workspaceId: 'workspace-1',
              tabId: 'tab-2',
              state: .waiting,
              lastAssistantMessage: 'Ready to continue',
            ),
          },
        );
        await tester.tap(find.byTooltip('Show Agent Runs'));
        await tester.pumpAndSettle();

        final sidebar = find.byType(ProjectWorkbenchSidebar);
        final goalRow = find.descendant(of: sidebar, matching: find.text(goal));
        final legacyRow = find.descendant(
          of: sidebar,
          matching: find.text('Read: settings.json'),
        );
        final progressRow = find.descendant(
          of: sidebar,
          matching: find.text('最新回報：$summary'),
        );
        final summaryTooltip = find.byWidgetPredicate(
          (widget) =>
              widget is Tooltip &&
              (widget.message?.contains(summary) ?? false) &&
              (widget.message?.contains(overall) ?? false) &&
              !(widget.message?.contains(report) ?? false),
        );
        expect(
          find.descendant(of: sidebar, matching: summaryTooltip),
          findsWidgets,
        );
        final tab = find.byKey(
          const ValueKey<String>('workspace-tab-chip:tab-1'),
        );
        expect(tab, findsOneWidget);
        expect(
          find.descendant(of: tab, matching: summaryTooltip),
          findsWidgets,
        );
        expect(
          find.descendant(of: tab, matching: find.byType(AgentIdentityIcon)),
          findsOneWidget,
        );
        if (enabled) {
          expect(goalRow, findsOneWidget);
          expect(progressRow, findsOneWidget);
          expect(legacyRow, findsNothing);
          expect(tester.widget<Text>(goalRow).maxLines, 4);
          expect(
            find.descendant(of: tab, matching: find.text(goal)),
            findsOneWidget,
          );
        } else {
          expect(goalRow, findsNothing);
          expect(progressRow, findsNothing);
          expect(legacyRow, findsOneWidget);
          expect(tester.widget<Text>(legacyRow).maxLines, 1);
        }
      },
    );
  }
}
