part of 'workspace_workbench_view_test.dart';

void _registerWorkspaceWorkbenchViewTabI18nTests() {
  testWidgets('Traditional Chinese localizes Terminal Tab right-click menu', (
    tester,
  ) async {
    final tabs = <WorkspaceTabRecord>[
      _tab('terminal-one', title: 'Terminal 1'),
      _tab('terminal-two', title: 'Terminal 2'),
    ];
    await _pumpWorkbenchView(
      tester,
      tabs: tabs,
      layout: _singleTabStripLayout(tabs, activeTabId: 'terminal-one'),
      terminalRuntime: terminalRuntime,
      createdTabs: createdTabs,
      selectedTabs: selectedTabs,
      closedTabs: closedTabs,
      closedTabGroups: closedTabGroups,
      renamedTabs: renamedTabs,
      movedTabs: movedTabs,
      splitGroups: splitGroups,
      mergedGroups: mergedGroups,
      updatedRatios: updatedRatios,
      externalTerminalTabs: <String>[],
      agentTitlesAvailable: true,
      locale: const Locale('zh', 'TW'),
    );
    await tester.pumpAndSettle();
    await _openTabContextMenu(tester, 'Terminal 1');

    for (final label in <String>[
      '向上分割',
      '向左分割',
      '以外部終端機開啟',
      '釘選分頁',
      '關閉其他分頁',
      '關閉右側分頁',
      '變更標題',
      '產生標題',
    ]) {
      expect(find.text(label), findsOneWidget, reason: label);
    }
    expect(find.text('Split Up'), findsNothing);

    await tester.tap(find.text('變更標題'));
    await tester.pumpAndSettle();
    expect(find.text('變更終端機標題'), findsOneWidget);
    expect(find.text('終端機標題'), findsOneWidget);
    await tester.enterText(find.byType(TextField).last, '新的標題');
    await tester.tap(find.text('變更標題').last);
    await tester.pumpAndSettle();
    expect(renamedTabs, <String>['新的標題']);
  });

  testWidgets('Simplified Chinese localizes file Tab and Explorer placeholder', (
    tester,
  ) async {
    final tab = _tab(
      'editor-one',
      title: 'README.md',
      kind: .editor,
      filePath: 'README.md',
    );
    // Avoid mounting the native CodeForge editor; only its Tab menu is tested.
    final terminal = _tab('terminal-one', title: 'Terminal');
    final tabs = <WorkspaceTabRecord>[terminal, tab];
    await _pumpWorkbenchView(
      tester,
      tabs: tabs,
      layout: _singleTabStripLayout(tabs, activeTabId: terminal.id),
      terminalRuntime: terminalRuntime,
      createdTabs: createdTabs,
      selectedTabs: selectedTabs,
      closedTabs: closedTabs,
      closedTabGroups: closedTabGroups,
      renamedTabs: renamedTabs,
      movedTabs: movedTabs,
      splitGroups: splitGroups,
      mergedGroups: mergedGroups,
      updatedRatios: updatedRatios,
      workspaceFolderOpener: _RecordingWorkspaceFolderOpener(),
      locale: const Locale('zh', 'CN'),
    );
    await _openTabContextMenu(tester, 'README.md');

    for (final label in <String>[
      '重新载入文档',
      '使用默认应用程序打开',
      '在 Explorer 中显示',
      '查看文件历史记录',
      '与最新 Git 版本比较',
      '与分支比较…',
      '关闭其他标签页',
    ]) {
      expect(
        find.text(label),
        findsOneWidget,
        reason:
            '$label; visible: '
            '${tester.widgetList<Text>(find.byType(Text)).map((item) => item.data).toList()}',
      );
    }
    expect(find.text('Reveal in Explorer'), findsNothing);
  });

  testWidgets('Japanese localizes pinned and preview Tab actions', (
    tester,
  ) async {
    final tab = _tab(
      'preview-one',
      title: 'Preview',
      preview: true,
      pinned: true,
    );
    await _pumpWorkbenchView(
      tester,
      tabs: <WorkspaceTabRecord>[tab],
      layout: .single(workspaceId: _workspaceId, tabIds: <String>[tab.id]),
      terminalRuntime: terminalRuntime,
      createdTabs: createdTabs,
      selectedTabs: selectedTabs,
      closedTabs: closedTabs,
      closedTabGroups: closedTabGroups,
      renamedTabs: renamedTabs,
      movedTabs: movedTabs,
      splitGroups: splitGroups,
      mergedGroups: mergedGroups,
      updatedRatios: updatedRatios,
      keptPreviewTabs: <String>[],
      locale: const Locale('ja', 'JP'),
    );
    await _openTabContextMenu(tester, 'Preview');
    expect(find.text('タブの固定を解除'), findsOneWidget);
    expect(find.text('開いたままにする'), findsOneWidget);
    expect(find.text('他のタブを閉じる'), findsOneWidget);
    expect(find.text('上に分割'), findsOneWidget);
  });

  testWidgets(
    'detached Tab uses the active locale and retains window actions',
    (tester) async {
      debugDefaultTargetPlatformOverride = TargetPlatform.windows;
      addTearDown(() => debugDefaultTargetPlatformOverride = null);
      final tab = _tab('history-one', title: 'History', kind: .gitHistory);
      var movedToMain = false;
      await tester.pumpWidget(
        ProviderScope(
          child: MaterialApp(
            locale: const Locale('zh', 'TW'),
            supportedLocales: supportedAleraLocales,
            localizationsDelegates: const <LocalizationsDelegate<dynamic>>[
              AleraLocalizationsDelegate(),
              ...GlobalMaterialLocalizations.delegates,
            ],
            home: Scaffold(
              body: Consumer(
                builder: (context, ref, child) => ElevatedButton(
                  onPressed: () => showWorkspaceTabContextMenu(
                    context,
                    globalPosition: const Offset(20, 20),
                    ref: ref,
                    workspace: _workspace(),
                    sourceControlScope: null,
                    tab: tab,
                    terminalSession: null,
                    groupTabs: <WorkspaceTabRecord>[tab],
                    onClose: () {},
                    onCloseTabs: (_) {},
                    onRename: (_) {},
                    onSplit: null,
                    detachedActions: WorkspaceTabDetachedMenuActions(
                      onMoveToMainWindow: () async {
                        movedToMain = true;
                        return true;
                      },
                      onMoveToNewWindow: () async => true,
                    ),
                  ),
                  child: const Text('Open Menu'),
                ),
              ),
            ),
          ),
        ),
      );
      await tester.tap(find.text('Open Menu'));
      await tester.pumpAndSettle();
      expect(find.text('移回主視窗'), findsOneWidget);
      expect(find.text('移至新視窗'), findsOneWidget);
      await tester.tap(find.text('移回主視窗'));
      await tester.pumpAndSettle();
      expect(movedToMain, isTrue);
      debugDefaultTargetPlatformOverride = null;
    },
  );
}
