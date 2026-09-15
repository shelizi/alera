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
    expect(find.byTooltip('Source Control'), findsOneWidget);
    expect(find.byIcon(AleraIcons.gitBranch), findsOneWidget);
    expect(
      tester.getTopLeft(find.byTooltip('Explorer')).dy,
      lessThan(tester.getTopLeft(find.byTooltip('Search')).dy),
    );
    expect(
      tester.getTopLeft(find.byTooltip('Search')).dy,
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
