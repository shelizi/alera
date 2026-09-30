part of 'workspace_explorer_test.dart';

void _registerWorkspaceExplorerContextSidebarTests() {
  testWidgets(
    'context sidebar keeps source control tabs and explains missing git scope',
    (tester) async {
      final service = _FakeWorkspaceFileService();

      await tester.pumpWidget(
        _withWorkspaceFiles(
          service,
          child: MaterialApp(
            home: Scaffold(
              body: WorkspaceContextSidebar(
                workspace: _workspace(),
                prefs: WorkbenchViewPrefs.defaults.copyWith(
                  activeContextPanelTab: .gitDiff,
                ),
                onToggleVisible: () {},
                onResize: (_) {},
                onSetContextPanelTab: (_) {},
                onSetExplorerMode: (_) {},
                onSetShowHiddenFiles: (_) {},
                onSetGitDiffViewMode: (_) {},
                onSetGitDiffGroupMode: (_) {},
                onOpenFile: (_) {},
                onOpenGitDiff: ({
                  relativePath,
                  area,
                  gitDiffRoot,
                  required scope,
                  bool preview = false,
                }) async {},
                onOpenGitCommitDiff: ({
                  relativePath,
                  oldPath,
                  required scope,
                  gitDiffRoot,
                  required commitOid,
                  parentOid,
                  required compareRef,
                  subject,
                  message,
                  bool preview = false,
                }) async {},
                onOpenSearchMatch: (_) {},
                onPathMoved: (_, _) async {},
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.byTooltip('Explorer'), findsOneWidget);
      expect(find.byTooltip('Search'), findsOneWidget);
      expect(find.byTooltip('References'), findsOneWidget);
      expect(find.byTooltip('Source Control'), findsOneWidget);
      expect(find.byTooltip('Pull Request'), findsOneWidget);
      expect(find.text('Source Control Unavailable'), findsOneWidget);
      expect(
        find.text(
          'This workspace is not connected to a Git repository, so there are no changes to show.',
        ),
        findsOneWidget,
      );
      expect(find.byType(WorkspaceExplorer), findsNothing);

      await tester.pumpWidget(
        _withWorkspaceFiles(
          service,
          child: MaterialApp(
            home: Scaffold(
              body: WorkspaceContextSidebar(
                workspace: _workspace(),
                prefs: WorkbenchViewPrefs.defaults.copyWith(
                  activeContextPanelTab: .pullRequests,
                  rightSidebarVisible: false,
                ),
                onToggleVisible: () {},
                onResize: (_) {},
                onSetContextPanelTab: (_) {},
                onSetExplorerMode: (_) {},
                onSetShowHiddenFiles: (_) {},
                onSetGitDiffViewMode: (_) {},
                onSetGitDiffGroupMode: (_) {},
                onOpenFile: (_) {},
                onOpenGitDiff: ({
                  relativePath,
                  area,
                  gitDiffRoot,
                  required scope,
                  bool preview = false,
                }) async {},
                onOpenGitCommitDiff: ({
                  relativePath,
                  oldPath,
                  required scope,
                  gitDiffRoot,
                  required commitOid,
                  parentOid,
                  required compareRef,
                  subject,
                  message,
                  bool preview = false,
                }) async {},
                onOpenSearchMatch: (_) {},
                onPathMoved: (_, _) async {},
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.byTooltip('Expand panel'), findsOneWidget);
      expect(find.byTooltip('References'), findsOneWidget);
      expect(find.byTooltip('Source Control'), findsOneWidget);
      expect(find.byTooltip('Pull Request'), findsOneWidget);

      await tester.pumpWidget(
        _withWorkspaceFiles(
          service,
          child: MaterialApp(
            home: Scaffold(
              body: WorkspaceContextSidebar(
                workspace: _workspace(),
                prefs: WorkbenchViewPrefs.defaults.copyWith(
                  activeContextPanelTab: .pullRequests,
                ),
                onToggleVisible: () {},
                onResize: (_) {},
                onSetContextPanelTab: (_) {},
                onSetExplorerMode: (_) {},
                onSetShowHiddenFiles: (_) {},
                onSetGitDiffViewMode: (_) {},
                onSetGitDiffGroupMode: (_) {},
                onOpenFile: (_) {},
                onOpenGitDiff: ({
                  relativePath,
                  area,
                  gitDiffRoot,
                  required scope,
                  bool preview = false,
                }) async {},
                onOpenGitCommitDiff: ({
                  relativePath,
                  oldPath,
                  required scope,
                  gitDiffRoot,
                  required commitOid,
                  parentOid,
                  required compareRef,
                  subject,
                  message,
                  bool preview = false,
                }) async {},
                onOpenSearchMatch: (_) {},
                onPathMoved: (_, _) async {},
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.byTooltip('Source Control'), findsOneWidget);
      expect(find.byTooltip('Pull Request'), findsOneWidget);
      expect(find.text('Pull Request Unavailable'), findsOneWidget);
      expect(
        find.text(
          'This workspace is not connected to a Git repository, so there are no Pull Requests to show.',
        ),
        findsOneWidget,
      );
    },
  );

  testWidgets('context sidebar hosts the references result surface', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService();
    await tester.pumpWidget(
      _withWorkspaceFiles(
        service,
        child: MaterialApp(
          home: Scaffold(
            body: SizedBox(
              width: 360,
              height: 520,
              child: WorkspaceContextSidebar(
                workspace: _workspace(),
                prefs: WorkbenchViewPrefs.defaults.copyWith(
                  activeContextPanelTab: .references,
                ),
                onToggleVisible: () {},
                onResize: (_) {},
                onSetContextPanelTab: (_) {},
                onSetExplorerMode: (_) {},
                onSetShowHiddenFiles: (_) {},
                onSetGitDiffViewMode: (_) {},
                onSetGitDiffGroupMode: (_) {},
                onOpenFile: (_) {},
                onOpenGitDiff: ({
                  relativePath,
                  area,
                  gitDiffRoot,
                  required scope,
                  bool preview = false,
                }) async {},
                onOpenGitCommitDiff: ({
                  relativePath,
                  oldPath,
                  required scope,
                  gitDiffRoot,
                  required commitOid,
                  parentOid,
                  required compareRef,
                  subject,
                  message,
                  bool preview = false,
                }) async {},
                onOpenSearchMatch: (_) {},
                onPathMoved: (_, _) async {},
              ),
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.byTooltip('References'), findsOneWidget);
    expect(find.text('Find References'), findsOneWidget);
    expect(
      find.text(
        'Run Find References from an editor to show project references here.',
      ),
      findsOneWidget,
    );
  });

  testWidgets('context sidebar preserves explorer state across tab switches', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
        _directory('src', hasChildrenHint: true),
        for (var index = 0; index < 30; index++) _file('file_$index.dart'),
      ]
      ..childrenByDirectory['src'] = <native.WorkspaceFileEntry>[
        _file('src/main.dart'),
      ];
    var activeTab = WorkbenchContextPanelTab.explorer;

    await tester.pumpWidget(
      _withWorkspaceFiles(
        service,
        child: MaterialApp(
          home: Scaffold(
            body: SizedBox(
              width: 360,
              height: 520,
              child: StatefulBuilder(
                builder: (context, setHarnessState) {
                  return WorkspaceContextSidebar(
                    workspace: _workspace(),
                    prefs: WorkbenchViewPrefs.defaults.copyWith(
                      activeContextPanelTab: activeTab,
                    ),
                    onToggleVisible: () {},
                    onResize: (_) {},
                    onSetContextPanelTab: (tab) {
                      setHarnessState(() => activeTab = tab);
                    },
                    onSetExplorerMode: (_) {},
                    onSetShowHiddenFiles: (_) {},
                    onSetGitDiffViewMode: (_) {},
                    onSetGitDiffGroupMode: (_) {},
                    onOpenFile: (_) {},
                    onOpenGitDiff: ({
                      relativePath,
                      area,
                      gitDiffRoot,
                      required scope,
                      bool preview = false,
                    }) async {},
                    onOpenGitCommitDiff: ({
                      relativePath,
                      oldPath,
                      required scope,
                      gitDiffRoot,
                      required commitOid,
                      parentOid,
                      required compareRef,
                      subject,
                      message,
                      bool preview = false,
                    }) async {},
                    onOpenSearchMatch: (_) {},
                    onPathMoved: (_, _) async {},
                  );
                },
              ),
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('src'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('main.dart'));
    await tester.pump();

    final explorerFinder = find.byType(WorkspaceExplorer);
    final explorerState = tester.state(explorerFinder);
    final scrollableFinder = find
        .descendant(of: explorerFinder, matching: find.byType(Scrollable))
        .last;
    await tester.drag(scrollableFinder, const Offset(0, -180));
    await tester.pumpAndSettle();
    final scrollBeforeSwitch = tester
        .state<ScrollableState>(scrollableFinder)
        .position
        .pixels;
    expect(scrollBeforeSwitch, greaterThan(0));

    await tester.tap(find.byTooltip('Search'));
    await tester.pumpAndSettle();

    final hiddenExplorerFinder = find.byType(
      WorkspaceExplorer,
      skipOffstage: false,
    );
    expect(find.byType(WorkspaceExplorer), findsNothing);
    expect(hiddenExplorerFinder, findsOneWidget);
    expect(tester.state(hiddenExplorerFinder), same(explorerState));

    await tester.tap(find.byTooltip('Explorer'));
    await tester.pumpAndSettle();

    final restoredExplorerFinder = find.byType(WorkspaceExplorer);
    expect(tester.state(restoredExplorerFinder), same(explorerState));
    final restoredScrollableFinder = find
        .descendant(
          of: restoredExplorerFinder,
          matching: find.byType(Scrollable),
        )
        .last;
    expect(
      tester.state<ScrollableState>(restoredScrollableFinder).position.pixels,
      closeTo(scrollBeforeSwitch, 0.1),
    );

    await tester.drag(
      restoredScrollableFinder,
      Offset(0, scrollBeforeSwitch + 32),
    );
    await tester.pumpAndSettle();
    expect(find.text('main.dart'), findsOneWidget);
  });

  testWidgets(
    'context sidebar loads the next workspace explorer without manual refresh',
    (tester) async {
      final stopGate = Completer<void>();
      final service = _FakeWorkspaceFileService(stopGate: stopGate)
        ..childrenByWorkspacePath['/repo/alera'] =
            <String, List<native.WorkspaceFileEntry>>{
              '': <native.WorkspaceFileEntry>[_file('main.dart')],
            }
        ..childrenByWorkspacePath['/repo/alera-feature'] =
            <String, List<native.WorkspaceFileEntry>>{
              '': <native.WorkspaceFileEntry>[_file('feature.dart')],
            };

      await tester.pumpWidget(
        _withWorkspaceFiles(
          service,
          child: MaterialApp(
            home: Scaffold(
              body: SizedBox(
                width: 360,
                height: 520,
                child: _workspaceContextSidebar(_workspace()),
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('main.dart'), findsOneWidget);
      expect(find.text('feature.dart'), findsNothing);

      await tester.pumpWidget(
        _withWorkspaceFiles(
          service,
          child: MaterialApp(
            home: Scaffold(
              body: SizedBox(
                width: 360,
                height: 520,
                child: _workspaceContextSidebar(
                  _workspace(
                    id: 'workspace-2',
                    name: 'Feature',
                    path: '/repo/alera-feature',
                  ),
                ),
              ),
            ),
          ),
        ),
      );
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 50));
      await tester.pump();

      expect(find.text('main.dart'), findsNothing);
      expect(find.text('feature.dart'), findsOneWidget);

      stopGate.complete();
    },
  );

  testWidgets(
    'context sidebar restores explorer state after switching workspaces',
    (tester) async {
      final service = _FakeWorkspaceFileService()
        ..childrenByWorkspacePath['/repo/alera'] =
            <String, List<native.WorkspaceFileEntry>>{
              '': <native.WorkspaceFileEntry>[
                for (var index = 0; index < 24; index += 1)
                  _file('file_' + index.toString().padLeft(2, '0') + '.dart'),
                _directory('src', hasChildrenHint: true),
              ],
              'src': <native.WorkspaceFileEntry>[_file('src/main.dart')],
            }
        ..childrenByWorkspacePath['/repo/alera-feature'] =
            <String, List<native.WorkspaceFileEntry>>{
              '': <native.WorkspaceFileEntry>[_file('feature.dart')],
            };

      Widget sidebar(Workspace workspace) => _withWorkspaceFiles(
        service,
        child: MaterialApp(
          home: Scaffold(
            body: SizedBox(
              width: 360,
              height: 520,
              child: _workspaceContextSidebar(workspace),
            ),
          ),
        ),
      );

      final mainWorkspace = _workspace();
      final featureWorkspace = _workspace(
        id: 'workspace-2',
        name: 'Feature',
        path: '/repo/alera-feature',
      );

      await tester.pumpWidget(sidebar(mainWorkspace));
      await tester.pumpAndSettle();

      var explorer = find.byType(WorkspaceExplorer);
      var scrollable = find
          .descendant(of: explorer, matching: find.byType(Scrollable))
          .last;
      await tester.scrollUntilVisible(
        find.text('src'),
        240,
        scrollable: scrollable,
      );
      await tester.tap(find.text('src'));
      await tester.pumpAndSettle();
      await tester.drag(scrollable, const Offset(0, -120));
      await tester.pumpAndSettle();
      await tester.tap(find.text('main.dart'));
      await tester.pump();

      final beforePixels = tester
          .state<ScrollableState>(scrollable)
          .position
          .pixels;
      expect(beforePixels, greaterThan(0));

      await tester.pumpWidget(sidebar(featureWorkspace));
      await tester.pumpAndSettle();
      expect(find.text('feature.dart'), findsOneWidget);
      expect(find.text('main.dart'), findsNothing);

      await tester.pumpWidget(sidebar(mainWorkspace));
      await tester.pumpAndSettle();

      expect(find.text('main.dart'), findsOneWidget);
      explorer = find.byType(WorkspaceExplorer);
      scrollable = find
          .descendant(of: explorer, matching: find.byType(Scrollable))
          .last;
      expect(
        tester.state<ScrollableState>(scrollable).position.pixels,
        closeTo(beforePixels, 0.1),
      );
      expect(
        find.ancestor(
          of: find.text('main.dart'),
          matching: find.byWidgetPredicate(
            (widget) =>
                widget is DecoratedBox &&
                widget.decoration is BoxDecoration &&
                (widget.decoration as BoxDecoration).color ==
                    AleraTokens.surfaceElevated,
          ),
        ),
        findsOneWidget,
      );
    },
  );
  testWidgets(
    'context sidebar preserves explorer expansion selection and scroll position across tabs',
    (tester) async {
      final service = _FakeWorkspaceFileService()
        ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
          for (var index = 0; index < 24; index += 1)
            _file('file_${index.toString().padLeft(2, '0')}.dart'),
          _directory('src', hasChildrenHint: true),
        ]
        ..childrenByDirectory['src'] = <native.WorkspaceFileEntry>[
          _file('src/main.dart'),
        ];

      Widget buildSidebar(
        WorkbenchContextPanelTab activeTab, {
        bool visible = true,
      }) {
        return _withWorkspaceFiles(
          service,
          child: MaterialApp(
            home: Scaffold(
              body: SizedBox(
                width: 360,
                height: 520,
                child: WorkspaceContextSidebar(
                  workspace: _workspace(),
                  prefs: WorkbenchViewPrefs.defaults.copyWith(
                    activeContextPanelTab: activeTab,
                    rightSidebarVisible: visible,
                  ),
                  onToggleVisible: () {},
                  onResize: (_) {},
                  onSetContextPanelTab: (_) {},
                  onSetExplorerMode: (_) {},
                  onSetShowHiddenFiles: (_) {},
                  onSetGitDiffViewMode: (_) {},
                  onSetGitDiffGroupMode: (_) {},
                  onOpenFile: (_) {},
                  onOpenGitDiff: ({
                    relativePath,
                    area,
                    gitDiffRoot,
                    required scope,
                    bool preview = false,
                  }) async {},
                  onOpenGitCommitDiff: ({
                    relativePath,
                    oldPath,
                    required scope,
                    gitDiffRoot,
                    required commitOid,
                    parentOid,
                    required compareRef,
                    subject,
                    message,
                    bool preview = false,
                  }) async {},
                  onOpenSearchMatch: (_) {},
                  onPathMoved: (_, _) async {},
                ),
              ),
            ),
          ),
        );
      }

      await tester.pumpWidget(buildSidebar(.explorer));
      await tester.pumpAndSettle();

      final explorerScrollable = find.byType(Scrollable).first;
      await tester.scrollUntilVisible(
        find.text('src'),
        240,
        scrollable: explorerScrollable,
      );
      await tester.tap(find.text('src'));
      await tester.pumpAndSettle();
      await tester.ensureVisible(find.text('main.dart'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('main.dart'));
      await tester.pump();

      final beforePixels = tester
          .state<ScrollableState>(explorerScrollable)
          .position
          .pixels;
      expect(beforePixels, greaterThan(0));
      final beforeTop = tester.getTopLeft(find.text('main.dart')).dy;
      expect(
        find.ancestor(
          of: find.text('main.dart'),
          matching: find.byWidgetPredicate(
            (widget) =>
                widget is DecoratedBox &&
                widget.decoration is BoxDecoration &&
                (widget.decoration as BoxDecoration).color ==
                    AleraTokens.surfaceElevated,
          ),
        ),
        findsOneWidget,
      );

      await tester.pumpWidget(buildSidebar(.search));
      await tester.pumpAndSettle();
      expect(find.text('main.dart'), findsNothing);

      await tester.pumpWidget(buildSidebar(.explorer));
      await tester.pumpAndSettle();

      expect(find.text('main.dart'), findsOneWidget);
      final restoredScrollable = find.byType(Scrollable).first;
      final afterPixels = tester
          .state<ScrollableState>(restoredScrollable)
          .position
          .pixels;
      final afterTop = tester.getTopLeft(find.text('main.dart')).dy;
      expect(afterPixels, closeTo(beforePixels, 0.01));
      expect(afterTop, closeTo(beforeTop, 0.01));
      expect(
        find.ancestor(
          of: find.text('main.dart'),
          matching: find.byWidgetPredicate(
            (widget) =>
                widget is DecoratedBox &&
                widget.decoration is BoxDecoration &&
                (widget.decoration as BoxDecoration).color ==
                    AleraTokens.surfaceElevated,
          ),
        ),
        findsOneWidget,
      );
      await tester.pumpWidget(buildSidebar(.explorer, visible: false));
      await tester.pumpAndSettle();
      expect(find.text('main.dart'), findsNothing);

      await tester.pumpWidget(buildSidebar(.explorer));
      await tester.pumpAndSettle();

      expect(find.text('main.dart'), findsOneWidget);
      final afterCollapseScrollable = find.byType(Scrollable).first;
      expect(
        tester.state<ScrollableState>(afterCollapseScrollable).position.pixels,
        closeTo(beforePixels, 0.01),
      );
      expect(
        tester.getTopLeft(find.text('main.dart')).dy,
        closeTo(beforeTop, 0.01),
      );
      expect(
        find.ancestor(
          of: find.text('main.dart'),
          matching: find.byWidgetPredicate(
            (widget) =>
                widget is DecoratedBox &&
                widget.decoration is BoxDecoration &&
                (widget.decoration as BoxDecoration).color ==
                    AleraTokens.surfaceElevated,
          ),
        ),
        findsOneWidget,
      );
    },
  );

  testWidgets('context sidebar uses rail only while collapsed', (tester) async {
    final service = _FakeWorkspaceFileService();

    await tester.pumpWidget(
      _withWorkspaceFiles(
        service,
        child: MaterialApp(
          home: Scaffold(
            body: WorkspaceContextSidebar(
              workspace: _workspace(),
              prefs: WorkbenchViewPrefs.defaults.copyWith(
                rightSidebarVisible: false,
              ),
              onToggleVisible: () {},
              onResize: (_) {},
              onSetContextPanelTab: (_) {},
              onSetExplorerMode: (_) {},
              onSetShowHiddenFiles: (_) {},
              onSetGitDiffViewMode: (_) {},
              onSetGitDiffGroupMode: (_) {},
              onOpenFile: (_) {},
              onOpenGitDiff: ({
                relativePath,
                area,
                gitDiffRoot,
                required scope,
                bool preview = false,
              }) async {},
              onOpenGitCommitDiff: ({
                relativePath,
                oldPath,
                required scope,
                gitDiffRoot,
                required commitOid,
                parentOid,
                required compareRef,
                subject,
                message,
                bool preview = false,
              }) async {},
              onOpenSearchMatch: (_) {},
              onPathMoved: (_, _) async {},
            ),
          ),
        ),
      ),
    );

    expect(find.byTooltip('Expand panel'), findsOneWidget);
    expect(find.byTooltip('Explorer'), findsOneWidget);
    expect(find.byTooltip('Search'), findsOneWidget);
    expect(find.byTooltip('References'), findsOneWidget);
    expect(find.byTooltip('Source Control'), findsOneWidget);
    expect(find.byIcon(AleraIcons.gitBranch), findsOneWidget);
    expect(
      tester.getTopLeft(find.byTooltip('Explorer')).dy,
      lessThan(tester.getTopLeft(find.byTooltip('Search')).dy),
    );
    expect(
      tester.getTopLeft(find.byTooltip('Search')).dy,
      lessThan(tester.getTopLeft(find.byTooltip('References')).dy),
    );
    expect(
      tester.getTopLeft(find.byTooltip('References')).dy,
      lessThan(tester.getTopLeft(find.byTooltip('Source Control')).dy),
    );
    expect(find.byType(WorkspaceExplorer), findsNothing);

    await tester.pumpWidget(
      _withWorkspaceFiles(
        service,
        child: MaterialApp(
          home: Scaffold(
            body: WorkspaceContextSidebar(
              workspace: _workspace(),
              prefs: .defaults,
              onToggleVisible: () {},
              onResize: (_) {},
              onSetContextPanelTab: (_) {},
              onSetExplorerMode: (_) {},
              onSetShowHiddenFiles: (_) {},
              onSetGitDiffViewMode: (_) {},
              onSetGitDiffGroupMode: (_) {},
              onOpenFile: (_) {},
              onOpenGitDiff: ({
                relativePath,
                area,
                gitDiffRoot,
                required scope,
                bool preview = false,
              }) async {},
              onOpenGitCommitDiff: ({
                relativePath,
                oldPath,
                required scope,
                gitDiffRoot,
                required commitOid,
                parentOid,
                required compareRef,
                subject,
                message,
                bool preview = false,
              }) async {},
              onOpenSearchMatch: (_) {},
              onPathMoved: (_, _) async {},
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.byTooltip('Expand panel'), findsNothing);
    expect(find.byTooltip('Collapse panel'), findsOneWidget);
    expect(find.byIcon(AleraIcons.gitBranch), findsOneWidget);
    expect(find.byType(WorkspaceExplorer), findsOneWidget);
  });
}
