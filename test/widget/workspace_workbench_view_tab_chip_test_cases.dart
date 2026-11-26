part of 'workspace_workbench_view_test.dart';

class _McpStatusTestHost implements RuntimeHostClient {
  final events = StreamController<RuntimeHostEvent>.broadcast();
  List<Map<String, Object?>> sessions = [];
  int requests = 0;
  @override
  Stream<RuntimeHostEvent> get runtimeEvents => events.stream;

  @override
  Future<Object?> runtimeRequest(
    String type, [
    Map<String, Object?> payload = const {},
    Duration? timeout,
  ]) async {
    if (type != 'desktopSettings.get') return null;
    requests++;
    return {
      'builtinMcpDiagnostics': {'sessions': sessions},
    };
  }
}

void _registerWorkspaceWorkbenchViewTabChipTests() {
  testWidgets(
    'local MCP badge follows exact Host identity and clears on disconnect',
    (tester) async {
      final tab = _tab(
        'mcp-agent',
        title: 'Codex',
      ).copyWith(payload: {'agentType': 'codex'});
      final other = _tab(
        'other-agent',
        title: 'Claude',
      ).copyWith(payload: {'agentType': 'claude'});
      final host = _McpStatusTestHost();
      addTearDown(host.events.close);
      Map<String, Object?> row(
        WorkspaceTabRecord t,
        String status, {
        String? workspaceId,
      }) => {
        'workspaceId': workspaceId ?? t.workspaceId,
        'tabId': t.id,
        'handle': t.terminalSessionId,
        'status': status,
      };
      host.sessions = [
        row(tab, 'connected'),
        row(other, 'connected', workspaceId: 'wrong'),
      ];
      await _pumpWorkbenchView(
        tester,
        tabs: [tab, other],
        terminalRuntime: terminalRuntime,
        layout: .single(workspaceId: _workspaceId, tabIds: [tab.id, other.id]),
        createdTabs: createdTabs,
        selectedTabs: selectedTabs,
        closedTabs: closedTabs,
        closedTabGroups: closedTabGroups,
        renamedTabs: renamedTabs,
        movedTabs: movedTabs,
        splitGroups: splitGroups,
        mergedGroups: mergedGroups,
        updatedRatios: updatedRatios,
        agentStatuses: null,
        mcpHost: host,
        size: const Size(900, 280),
      );
      await tester.pump();
      expect(find.byTooltip('MCP connected'), findsOneWidget);
      expect(host.requests, 1, reason: 'all tabs share one status request');
      host.sessions = [
        row(tab, 'disconnected'),
        row(other, 'awaiting_discovery'),
      ];
      host.events.add(const RuntimeHostEvent('builtinMcpChanged', {}));
      await tester.pump();
      await tester.pump();
      expect(find.byTooltip('MCP connected'), findsNothing);
      expect(find.byTooltip('MCP disconnected'), findsOneWidget);
      expect(find.byTooltip('MCP awaiting discovery'), findsOneWidget);
      host.events.add(
        const RuntimeHostEvent(aleraRuntimeHostDisconnectedEvent, {}),
      );
      await tester.pump();
      await tester.pump();
      expect(find.byTooltip('MCP disconnected'), findsNothing);
      expect(find.byTooltip('MCP awaiting discovery'), findsNothing);
    },
  );

  testWidgets('terminal tab chips show agent status dots', (tester) async {
    final tab = _tab('tab-1', title: 'Terminal');

    await _pumpWorkbenchView(
      tester,
      tabs: <WorkspaceTabRecord>[tab],
      terminalRuntime: terminalRuntime,
      layout: .single(workspaceId: _workspaceId, tabIds: <String>[tab.id]),
      agentStatuses: <String, AgentStatusEntry>{
        tab.terminalSessionId: _agentStatus(tab, state: .waiting),
      },
      createdTabs: createdTabs,
      selectedTabs: selectedTabs,
      closedTabs: closedTabs,
      closedTabGroups: closedTabGroups,
      renamedTabs: renamedTabs,
      movedTabs: movedTabs,
      splitGroups: splitGroups,
      mergedGroups: mergedGroups,
      updatedRatios: updatedRatios,
    );

    expect(find.byType(AleraStatusDot), findsOneWidget);
  });

  testWidgets('editor tabs use file icons in the chip', (tester) async {
    final terminalTab = _tab('tab-1', title: 'Terminal');
    final editorTab = _tab(
      'tab-2',
      title: 'main.dart',
      kind: .editor,
      filePath: 'lib/main.dart',
    );

    await _pumpWorkbenchView(
      tester,
      tabs: <WorkspaceTabRecord>[terminalTab, editorTab],
      terminalRuntime: terminalRuntime,
      layout: WorkbenchLayout(
        workspaceId: _workspaceId,
        root: .leaf('group-a'),
        groups: <String, WorkbenchPaneGroup>{
          'group-a': WorkbenchPaneGroup(
            id: 'group-a',
            tabIds: <String>[terminalTab.id, editorTab.id],
            activeTabId: terminalTab.id,
          ),
        },
        activeGroupId: 'group-a',
      ),
      createdTabs: createdTabs,
      selectedTabs: selectedTabs,
      closedTabs: closedTabs,
      closedTabGroups: closedTabGroups,
      renamedTabs: renamedTabs,
      movedTabs: movedTabs,
      splitGroups: splitGroups,
      mergedGroups: mergedGroups,
      updatedRatios: updatedRatios,
    );

    final icon = tester.widget<AleraFileIcon>(find.byType(AleraFileIcon));
    expect(icon.kind, AleraFileIconKind.file);
    expect(icon.pathOrName, 'lib/main.dart');
  });

  testWidgets('markdown viewer tabs use file icons in the chip', (
    tester,
  ) async {
    final terminalTab = _tab('tab-1', title: 'Terminal');
    final viewerTab = _tab(
      'tab-2',
      title: 'readme.md',
      kind: .markdownViewer,
      filePath: 'docs/readme.md',
    );

    await _pumpWorkbenchView(
      tester,
      tabs: <WorkspaceTabRecord>[terminalTab, viewerTab],
      terminalRuntime: terminalRuntime,
      layout: WorkbenchLayout(
        workspaceId: _workspaceId,
        root: .leaf('group-a'),
        groups: <String, WorkbenchPaneGroup>{
          'group-a': WorkbenchPaneGroup(
            id: 'group-a',
            tabIds: <String>[terminalTab.id, viewerTab.id],
            activeTabId: terminalTab.id,
          ),
        },
        activeGroupId: 'group-a',
      ),
      createdTabs: createdTabs,
      selectedTabs: selectedTabs,
      closedTabs: closedTabs,
      closedTabGroups: closedTabGroups,
      renamedTabs: renamedTabs,
      movedTabs: movedTabs,
      splitGroups: splitGroups,
      mergedGroups: mergedGroups,
      updatedRatios: updatedRatios,
    );

    final fileIcons = tester.widgetList<AleraFileIcon>(
      find.byType(AleraFileIcon),
    );
    expect(fileIcons.single.kind, AleraFileIconKind.file);
    expect(fileIcons.single.pathOrName, 'docs/readme.md');
  });

  testWidgets('git diff tabs use source control icon and editor width', (
    tester,
  ) async {
    final terminalTab = _tab('tab-1', title: 'Terminal');
    final editorTab = _tab(
      'tab-2',
      title: 'very_long_editor_file_name.dart',
      kind: .editor,
      filePath: 'lib/very_long_editor_file_name.dart',
    );
    final gitDiffTab = _tab(
      'tab-3',
      title: 'very_long_git_diff_file_name.dart',
      kind: .gitDiff,
      filePath: 'lib/very_long_git_diff_file_name.dart',
    );

    await _pumpWorkbenchView(
      tester,
      tabs: <WorkspaceTabRecord>[terminalTab, editorTab, gitDiffTab],
      terminalRuntime: terminalRuntime,
      layout: WorkbenchLayout(
        workspaceId: _workspaceId,
        root: .leaf('group-a'),
        groups: <String, WorkbenchPaneGroup>{
          'group-a': WorkbenchPaneGroup(
            id: 'group-a',
            tabIds: <String>[terminalTab.id, editorTab.id, gitDiffTab.id],
            activeTabId: terminalTab.id,
          ),
        },
        activeGroupId: 'group-a',
      ),
      createdTabs: createdTabs,
      selectedTabs: selectedTabs,
      closedTabs: closedTabs,
      closedTabGroups: closedTabGroups,
      renamedTabs: renamedTabs,
      movedTabs: movedTabs,
      splitGroups: splitGroups,
      mergedGroups: mergedGroups,
      updatedRatios: updatedRatios,
      size: const Size(720, 280),
    );

    expect(find.byIcon(AleraIcons.gitBranch), findsOneWidget);
    expect(_tabTitleMaxWidth(tester, editorTab.title), 180);
    expect(_tabTitleMaxWidth(tester, gitDiffTab.title), 180);
  });

  testWidgets('commit graph tabs render the history surface', (tester) async {
    final backend = FakeGitBackend()
      ..gitHistoryResult = const GitHistoryResult(
        items: <GitHistoryItem>[
          GitHistoryItem(
            id: 'c1',
            parentIds: <String>[],
            subject: 'Initial Commit',
            message: 'Initial Commit',
          ),
        ],
        hasIncomingChanges: false,
        hasOutgoingChanges: false,
        hasMore: false,
        limit: 200,
      );
    final graphTab = _tab(
      'tab-graph',
      title: 'Commit Graph',
      kind: .gitHistory,
    );

    await _pumpWorkbenchView(
      tester,
      tabs: <WorkspaceTabRecord>[graphTab],
      terminalRuntime: terminalRuntime,
      layout: .single(
        workspaceId: _workspaceId,
        groupId: 'group-a',
        tabIds: <String>[graphTab.id],
      ),
      createdTabs: createdTabs,
      selectedTabs: selectedTabs,
      closedTabs: closedTabs,
      closedTabGroups: closedTabGroups,
      renamedTabs: renamedTabs,
      movedTabs: movedTabs,
      splitGroups: splitGroups,
      mergedGroups: mergedGroups,
      updatedRatios: updatedRatios,
      gitBackend: backend,
    );
    await tester.pumpAndSettle();

    expect(find.byIcon(AleraIcons.gitGraph), findsWidgets);
    expect(find.text('Commits'), findsOneWidget);
    expect(find.text('All Branches'), findsOneWidget);
    expect(find.text('Initial Commit'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}

double _tabTitleMaxWidth(WidgetTester tester, String title) {
  final box = tester.widget<ConstrainedBox>(
    find.ancestor(of: find.text(title), matching: find.byType(ConstrainedBox)),
  );
  return box.constraints.maxWidth;
}
