part of 'workspace_workbench_view_test.dart';

void _registerWorkspaceWorkbenchViewTabTests() {
  for (final kind in [WorkspaceTabKind.terminal]) {
    for (final mode in ['unsupported', 'new', 'generated', 'generating']) {
      testWidgets('title action respects $kind capability and $mode state', (
        tester,
      ) async {
        final tab = _tab('title-tab', title: 'Agent Task', kind: kind).copyWith(
          payload: <String, Object?>{
            if (mode == 'generated') 'agentTitleSource': 'generated',
            if (mode == 'generating') 'agentTitleStatus': 'generating',
          },
        );
        await _pumpWorkbenchView(
          tester,
          tabs: [
            tab,
            _tab('shell', title: 'Shell'),
          ],
          terminalRuntime: terminalRuntime,
          layout: .single(
            workspaceId: _workspaceId,
            groupId: 'group-a',
            tabIds: [tab.id, 'shell'],
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
          agentTitlesAvailable: mode != 'unsupported',
        );
        await tester.pumpAndSettle();
        await _openTabContextMenu(tester, 'Agent Task');
        final label = mode == 'generating'
            ? 'Generating title...'
            : mode == 'generated'
            ? 'Regenerate Title'
            : 'Generate Title';
        expect(
          find.text(label),
          mode == 'unsupported' ? findsNothing : findsOneWidget,
        );
        if (kind == WorkspaceTabKind.terminal && mode != 'unsupported') {
          expect(
            tester.getTopLeft(find.text('Change Title')).dy,
            lessThan(tester.getTopLeft(find.text(label)).dy),
          );
        }
        if (mode != 'unsupported') {
          final entry = tester.widget<AleraDropdownEntry<Object?>>(
            find.ancestor(
              of: find.text(label),
              matching: find.byWidgetPredicate(
                (widget) => widget is AleraDropdownEntry,
              ),
            ),
          );
          expect(entry.leading, isA<Icon>());
          expect((entry.leading! as Icon).size, 16);
          expect((entry.leading! as Icon).icon, AleraIcons.ai);
        }
        if (mode == 'generating') {
          expect(find.byTooltip('Generating title...'), findsOneWidget);
          final entry = tester.widget<AleraDropdownEntry<Object?>>(
            find.ancestor(
              of: find.text(label),
              matching: find.byWidgetPredicate(
                (widget) => widget is AleraDropdownEntry,
              ),
            ),
          );
          expect(entry.enabled, isFalse);
        }
      });
    }
  }

  testWidgets('tab context menu closes sibling tabs', (tester) async {
    final tabs = <WorkspaceTabRecord>[
      _tab('tab-1', title: 'Terminal 1'),
      _tab('tab-2', title: 'Terminal 2'),
      _tab('tab-3', title: 'Terminal 3'),
    ];

    await _pumpWorkbenchView(
      tester,
      tabs: tabs,
      terminalRuntime: terminalRuntime,
      layout: .single(
        workspaceId: _workspaceId,
        groupId: 'group-a',
        tabIds: tabs.map((tab) => tab.id).toList(),
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

    await _openTabContextMenu(tester, 'Terminal 2');
    await tester.tap(find.text('Close Others'));
    await tester.pumpAndSettle();

    expect(closedTabGroups, <List<String>>[
      <String>['tab-1', 'tab-3'],
    ]);

    await _openTabContextMenu(tester, 'Terminal 2');
    await tester.tap(find.text('Close Tabs to the Right'));
    await tester.pumpAndSettle();

    expect(closedTabGroups, <List<String>>[
      <String>['tab-1', 'tab-3'],
      <String>['tab-3'],
    ]);
  });

  testWidgets('terminal tab context menu opens an external terminal', (
    tester,
  ) async {
    final tab = _tab('tab-1', title: 'Terminal');
    final openedTabs = <String>[];

    await _pumpWorkbenchView(
      tester,
      tabs: <WorkspaceTabRecord>[tab],
      terminalRuntime: terminalRuntime,
      layout: .single(workspaceId: _workspaceId, tabIds: <String>[tab.id]),
      createdTabs: createdTabs,
      selectedTabs: selectedTabs,
      closedTabs: closedTabs,
      closedTabGroups: closedTabGroups,
      renamedTabs: renamedTabs,
      movedTabs: movedTabs,
      splitGroups: splitGroups,
      mergedGroups: mergedGroups,
      updatedRatios: updatedRatios,
      externalTerminalTabs: openedTabs,
    );

    await _openTabContextMenu(tester, 'Terminal');
    expect(find.text('Open In External Terminal'), findsOneWidget);

    await tester.tap(find.text('Open In External Terminal'));
    await tester.pumpAndSettle();

    expect(openedTabs, <String>['tab-1']);
  });

  testWidgets('tab context menu renames and splits the active group', (
    tester,
  ) async {
    final tabs = <WorkspaceTabRecord>[
      _tab('tab-1', title: 'Terminal 1'),
      _tab('tab-2', title: 'Terminal 2'),
    ];

    await _pumpWorkbenchView(
      tester,
      tabs: tabs,
      terminalRuntime: terminalRuntime,
      layout: .single(
        workspaceId: _workspaceId,
        groupId: 'group-a',
        tabIds: tabs.map((tab) => tab.id).toList(),
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

    await _openTabContextMenu(tester, 'Terminal 2');
    await tester.tap(find.text('Change Title'));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField).last, 'Renamed terminal');
    await tester.tap(find.text('Change Title').last);
    await tester.pumpAndSettle();

    expect(renamedTabs, <String>['Renamed terminal']);

    await _openTabContextMenu(tester, 'Terminal 2');
    await tester.tap(find.text('Split Right'));
    await tester.pumpAndSettle();

    expect(splitGroups, contains(const _SplitGroupAction('group-a', .right)));
  });

  testWidgets('tab context menu routes split up, down, left, and close', (
    tester,
  ) async {
    final tabs = <WorkspaceTabRecord>[
      _tab('tab-1', title: 'Terminal 1'),
      _tab('tab-2', title: 'Terminal 2'),
    ];

    await _pumpWorkbenchView(
      tester,
      tabs: tabs,
      terminalRuntime: terminalRuntime,
      layout: .single(
        workspaceId: _workspaceId,
        groupId: 'group-a',
        tabIds: tabs.map((tab) => tab.id).toList(),
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

    for (final label in <String>['Split Up', 'Split Down', 'Split Left']) {
      await _openTabContextMenu(tester, 'Terminal 2');
      await tester.tap(find.text(label));
      await tester.pumpAndSettle();
    }

    await _openTabContextMenu(tester, 'Terminal 2');
    await tester.tap(find.text('Close'));
    await tester.pumpAndSettle();

    expect(
      splitGroups,
      containsAll(<_SplitGroupAction>[
        const _SplitGroupAction('group-a', .up),
        const _SplitGroupAction('group-a', .down),
        const _SplitGroupAction('group-a', .left),
      ]),
    );
    expect(closedTabs, <String>['tab-2']);
  });

  testWidgets('active-tab fallback picks the first available tab', (
    tester,
  ) async {
    final tabs = <WorkspaceTabRecord>[
      _tab('tab-1', title: 'Terminal 1'),
      _tab('tab-2', title: 'Terminal 2'),
    ];

    await _pumpWorkbenchView(
      tester,
      tabs: tabs,
      terminalRuntime: terminalRuntime,
      layout: WorkbenchLayout(
        workspaceId: _workspaceId,
        root: .leaf('group-a'),
        groups: <String, WorkbenchPaneGroup>{
          'group-a': WorkbenchPaneGroup(
            id: 'group-a',
            tabIds: tabs.map((tab) => tab.id).toList(),
            activeTabId: 'missing-tab',
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

    expect(
      find.byKey(const ValueKey<String>('terminal-tab-1')),
      findsOneWidget,
    );
    expect(terminalRuntime.requestedTabIds, contains('tab-1'));
  });
}
