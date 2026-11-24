part of 'alera_shell_page_test.dart';

const Key _statusGlyphKey = ValueKey<String>('workspace-status-glyph');

void _registerAleraShellSidebarIdentityTests() {
  testWidgets('a workspace row keeps its element when rows reorder', (
    tester,
  ) async {
    // Rows sort by live agent activity, so without stable keys the list
    // re-matches elements by index and row content slides between them.
    final state = _stackedWorkbenchState();
    final harness = await _pumpShell(tester, state: state);
    final rowKey = ValueKey<String>(
      'workspace:all:${_firstWorkspaceId(state)}',
    );
    final before = tester.element(find.byKey(rowKey));

    harness.agentStatus.setEntries(<String, AgentStatusEntry>{
      'tab-2': _agentStatusEntry(
        terminalSessionId: 'tab-2',
        workspaceId: 'workspace-2',
        tabId: 'tab-2',
        state: .waiting,
      ),
    });
    await tester.pump();

    expect(identical(tester.element(find.byKey(rowKey)), before), isTrue);
  });

  testWidgets('a background tool update rebuilds only its sidebar row', (
    tester,
  ) async {
    // Keep enough room for both the sidebar and the active workbench.
    await tester.binding.setSurfaceSize(const Size(1440, 900));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final seeded = _linkedWorkbenchState(linkedExpanded: true);
    final state = seeded.copyWith(
      viewPrefs: seeded.viewPrefs.copyWith(
        workspaceSort: .name,
        projectSort: .name,
      ),
    );
    final main = _agentStatusEntry(
      terminalSessionId: 'tab-1',
      workspaceId: 'workspace-1',
      tabId: 'tab-1',
      state: .waiting,
      lastAssistantMessage: 'Main waiting',
    );
    AgentStatusEntry linked(String message) => _agentStatusEntry(
      terminalSessionId: 'tab-2',
      workspaceId: 'workspace-2',
      tabId: 'tab-2',
      state: .waiting,
      lastAssistantMessage: message,
    );
    final harness = await _pumpShell(
      tester,
      state: state,
      agentStatuses: {'tab-1': main, 'tab-2': linked('Old message')},
    );
    await tester.pumpAndSettle();
    final mainRow = find.byKey(
      const ValueKey<String>('workspace-row:regular:workspace-1'),
    );
    final linkedRow = find.byKey(
      const ValueKey<String>('workspace-row:regular:workspace-2'),
    );
    final beforeMain = tester.widget(mainRow);
    final beforeLinked = tester.widget(linkedRow);
    harness.agentStatus.setEntries({
      'tab-1': main,
      'tab-2': linked('New message'),
    });
    await tester.pump();
    expect(tester.widget(mainRow), same(beforeMain));
    expect(tester.widget(linkedRow), isNot(same(beforeLinked)));
    expect(find.text('New message'), findsOneWidget);
    expect(find.text('Old message'), findsNothing);
  });

  testWidgets('the status slot keeps its element when an agent starts', (
    tester,
  ) async {
    // Swapping widget types in this slot destroyed the element and restarted
    // the spinner from zero, which is the stutter under orchestration load.
    final state = _stackedWorkbenchState();
    final harness = await _pumpShell(tester, state: state);
    final before = tester.element(find.byKey(_statusGlyphKey).first);

    harness.agentStatus.setEntries(<String, AgentStatusEntry>{
      'tab-1': _agentStatusEntry(
        terminalSessionId: 'tab-1',
        workspaceId: 'workspace-1',
        tabId: 'tab-1',
        state: .working,
      ),
    });
    await tester.pump();

    expect(
      identical(tester.element(find.byKey(_statusGlyphKey).first), before),
      isTrue,
    );
  });

  testWidgets('working agents share a single spinner ticker', (tester) async {
    final state = _stackedWorkbenchState();
    await _pumpShell(
      tester,
      state: state,
      agentStatuses: <String, AgentStatusEntry>{
        'tab-1': _agentStatusEntry(
          terminalSessionId: 'tab-1',
          workspaceId: 'workspace-1',
          tabId: 'tab-1',
          state: .working,
        ),
        'tab-2': _agentStatusEntry(
          terminalSessionId: 'tab-2',
          workspaceId: 'workspace-2',
          tabId: 'tab-2',
          state: .working,
        ),
      },
    );

    expect(find.byType(AgentRunSharedSpinner), findsWidgets);
    // Other parts of Alera can legitimately show a Material progress
    // indicator. Workspace status glyphs must use the shared agent ticker.
    expect(
      find.descendant(
        of: find.byKey(_statusGlyphKey),
        matching: find.byType(CircularProgressIndicator),
      ),
      findsNothing,
    );
  });
}

String _firstWorkspaceId(WorkbenchState state) {
  return state.workspacesByProject.values.first.first.id;
}
