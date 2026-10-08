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

  testWidgets('editor tab context menu exposes reload document', (
    tester,
  ) async {
    final terminalTab = _tab('terminal', title: 'Terminal');
    final editorTab = _tab(
      'editor',
      title: 'README.md',
      kind: WorkspaceTabKind.editor,
      filePath: 'README.md',
    );
    final opener = _RecordingWorkspaceFolderOpener();

    await _pumpWorkbenchView(
      tester,
      tabs: <WorkspaceTabRecord>[terminalTab, editorTab],
      terminalRuntime: terminalRuntime,
      layout: .single(
        workspaceId: _workspaceId,
        groupId: 'group-a',
        tabIds: <String>[editorTab.id, terminalTab.id],
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
      workspaceFolderOpener: opener,
    );

    await _openTabContextMenu(tester, 'README.md');
    expect(find.text('Reload Document'), findsOneWidget);
    expect(find.text('View File History'), findsOneWidget);
    expect(find.text('Open with Default Application'), findsOneWidget);
    expect(find.text('Reveal in Explorer'), findsOneWidget);

    await tester.tap(find.text('Open with Default Application'));
    await tester.pumpAndSettle();
    expect(opener.defaultOpenedPaths, <String>[
      terminalAbsolutePath(rootPath: '/tmp/alera', relativePath: 'README.md'),
    ]);

    await _openTabContextMenu(tester, 'README.md');
    await tester.tap(find.text('Reveal in Explorer'));
    await tester.pumpAndSettle();
    expect(opener.revealedPaths, <String>[
      terminalAbsolutePath(rootPath: '/tmp/alera', relativePath: 'README.md'),
    ]);
  });

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

  testWidgets('overflowing tabs expose horizontal scroll controls', (
    tester,
  ) async {
    final tabs = _overflowingWorkbenchTabs();
    await _pumpWorkbenchView(
      tester,
      tabs: tabs,
      terminalRuntime: terminalRuntime,
      layout: _singleTabStripLayout(tabs, activeTabId: tabs.first.id),
      createdTabs: createdTabs,
      selectedTabs: selectedTabs,
      closedTabs: closedTabs,
      closedTabGroups: closedTabGroups,
      renamedTabs: renamedTabs,
      movedTabs: movedTabs,
      splitGroups: splitGroups,
      mergedGroups: mergedGroups,
      updatedRatios: updatedRatios,
      size: const Size(360, 280),
    );
    await tester.pumpAndSettle();

    final left = find.byKey(
      const ValueKey<String>('workspace-tab-scroll-left:group-a'),
    );
    final right = find.byKey(
      const ValueKey<String>('workspace-tab-scroll-right:group-a'),
    );
    expect(left, findsOneWidget);
    expect(right, findsOneWidget);

    final controller = _tabStripScrollController(tester);
    expect(controller.position.maxScrollExtent, greaterThan(0));
    expect(controller.position.pixels, closeTo(0, 0.1));
    expect(
      tester
          .widget<IconButton>(
            find.descendant(of: left, matching: find.byType(IconButton)),
          )
          .onPressed,
      isNull,
    );

    await tester.tap(right);
    await tester.pumpAndSettle();

    final afterRight = controller.position.pixels;
    expect(afterRight, greaterThan(0));
    expect(
      tester
          .widget<IconButton>(
            find.descendant(of: left, matching: find.byType(IconButton)),
          )
          .onPressed,
      isNotNull,
    );

    await tester.tap(left);
    await tester.pumpAndSettle();
    expect(controller.position.pixels, lessThan(afterRight));
  });

  testWidgets('mouse wheel scrolls overflowing tabs horizontally', (
    tester,
  ) async {
    final tabs = _overflowingWorkbenchTabs();
    await _pumpWorkbenchView(
      tester,
      tabs: tabs,
      terminalRuntime: terminalRuntime,
      layout: _singleTabStripLayout(tabs, activeTabId: tabs.first.id),
      createdTabs: createdTabs,
      selectedTabs: selectedTabs,
      closedTabs: closedTabs,
      closedTabGroups: closedTabGroups,
      renamedTabs: renamedTabs,
      movedTabs: movedTabs,
      splitGroups: splitGroups,
      mergedGroups: mergedGroups,
      updatedRatios: updatedRatios,
      size: const Size(360, 280),
    );
    await tester.pumpAndSettle();

    final scrollView = find.byKey(
      const ValueKey<String>('workspace-tab-scroll:group-a'),
    );
    final controller = _tabStripScrollController(tester);
    expect(controller.position.pixels, closeTo(0, 0.1));

    await tester.sendEventToBinding(
      PointerScrollEvent(
        position: tester.getCenter(scrollView),
        scrollDelta: const Offset(0, 120),
        kind: PointerDeviceKind.mouse,
      ),
    );
    await tester.pump();

    expect(controller.position.pixels, greaterThan(0));
    final afterForwardWheel = controller.position.pixels;

    await tester.sendEventToBinding(
      PointerScrollEvent(
        position: tester.getCenter(scrollView),
        scrollDelta: const Offset(0, -120),
        kind: PointerDeviceKind.mouse,
      ),
    );
    await tester.pump();

    expect(controller.position.pixels, lessThan(afterForwardWheel));
  });

  testWidgets('pinned tabs stay left and fixed while other tabs scroll', (
    tester,
  ) async {
    final tabs = <WorkspaceTabRecord>[
      _tab('regular-first', title: 'Regular first'),
      _tab('pinned-tab', title: 'Pinned tab', pinned: true),
      ..._overflowingWorkbenchTabs(),
    ];
    await _pumpWorkbenchView(
      tester,
      tabs: tabs,
      terminalRuntime: terminalRuntime,
      layout: _singleTabStripLayout(tabs, activeTabId: 'regular-first'),
      createdTabs: createdTabs,
      selectedTabs: selectedTabs,
      closedTabs: closedTabs,
      closedTabGroups: closedTabGroups,
      renamedTabs: renamedTabs,
      movedTabs: movedTabs,
      splitGroups: splitGroups,
      mergedGroups: mergedGroups,
      updatedRatios: updatedRatios,
      size: const Size(420, 280),
    );
    await tester.pumpAndSettle();

    final pinned = find.byKey(
      const ValueKey<String>('workspace-tab-chip:pinned-tab'),
    );
    final regular = find.byKey(
      const ValueKey<String>('workspace-tab-chip:regular-first'),
    );
    final scrollView = find.byKey(
      const ValueKey<String>('workspace-tab-scroll:group-a'),
    );
    expect(
      tester.getTopLeft(pinned).dx,
      lessThan(tester.getTopLeft(regular).dx),
    );
    final pinnedLeft = tester.getTopLeft(pinned).dx;

    await tester.sendEventToBinding(
      PointerScrollEvent(
        position: tester.getCenter(scrollView),
        scrollDelta: const Offset(0, 160),
        kind: PointerDeviceKind.mouse,
      ),
    );
    await tester.pump();

    expect(tester.getTopLeft(pinned).dx, closeTo(pinnedLeft, 0.1));
    expect(_tabStripScrollController(tester).position.pixels, greaterThan(0));
  });

  testWidgets('active overflow tab is revealed with trailing breathing room', (
    tester,
  ) async {
    final tabs = _overflowingWorkbenchTabs();
    await _pumpWorkbenchView(
      tester,
      tabs: tabs,
      terminalRuntime: terminalRuntime,
      layout: _singleTabStripLayout(tabs, activeTabId: tabs.first.id),
      createdTabs: createdTabs,
      selectedTabs: selectedTabs,
      closedTabs: closedTabs,
      closedTabGroups: closedTabGroups,
      renamedTabs: renamedTabs,
      movedTabs: movedTabs,
      splitGroups: splitGroups,
      mergedGroups: mergedGroups,
      updatedRatios: updatedRatios,
      size: const Size(360, 280),
    );
    await tester.pumpAndSettle();
    expect(_tabStripScrollController(tester).position.pixels, closeTo(0, 0.1));

    await _pumpWorkbenchView(
      tester,
      tabs: tabs,
      terminalRuntime: terminalRuntime,
      layout: _singleTabStripLayout(tabs, activeTabId: tabs.last.id),
      createdTabs: createdTabs,
      selectedTabs: selectedTabs,
      closedTabs: closedTabs,
      closedTabGroups: closedTabGroups,
      renamedTabs: renamedTabs,
      movedTabs: movedTabs,
      splitGroups: splitGroups,
      mergedGroups: mergedGroups,
      updatedRatios: updatedRatios,
      size: const Size(360, 280),
    );
    await tester.pumpAndSettle();

    final controller = _tabStripScrollController(tester);
    expect(controller.position.pixels, greaterThan(0));

    final scrollRect = tester.getRect(
      find.byKey(const ValueKey<String>('workspace-tab-scroll:group-a')),
    );
    final activeTabRect = tester.getRect(
      find.byKey(ValueKey<String>('workspace-tab-chip:${tabs.last.id}')),
    );
    expect(activeTabRect.left, greaterThanOrEqualTo(scrollRect.left));
    expect(activeTabRect.right, lessThan(scrollRect.right - 4));
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

List<WorkspaceTabRecord> _overflowingWorkbenchTabs() {
  return <WorkspaceTabRecord>[
    for (var index = 0; index < 8; index += 1)
      _tab('overflow-tab-$index', title: 'Long terminal tab $index'),
  ];
}

WorkbenchLayout _singleTabStripLayout(
  List<WorkspaceTabRecord> tabs, {
  required String activeTabId,
}) {
  return WorkbenchLayout(
    workspaceId: _workspaceId,
    root: .leaf('group-a'),
    groups: <String, WorkbenchPaneGroup>{
      'group-a': WorkbenchPaneGroup(
        id: 'group-a',
        tabIds: <String>[for (final tab in tabs) tab.id],
        activeTabId: activeTabId,
      ),
    },
    activeGroupId: 'group-a',
  );
}

ScrollController _tabStripScrollController(WidgetTester tester) {
  return tester
      .widget<SingleChildScrollView>(
        find.byKey(const ValueKey<String>('workspace-tab-scroll:group-a')),
      )
      .controller!;
}
