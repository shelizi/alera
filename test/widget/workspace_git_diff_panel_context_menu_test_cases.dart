part of 'workspace_git_diff_panel_test.dart';

void _registerWorkspaceGitDiffPanelContextMenuTests() {
  testWidgets('tree file context menu opens the file instead of the diff', (
    tester,
  ) async {
    final backend = FakeGitBackend()
      ..gitStatusResult = const GitStatusResult(
        entries: <GitChangeEntry>[
          GitChangeEntry(
            path: 'lib/src/dirty.dart',
            area: .unstaged,
            status: .modified,
          ),
        ],
      );
    final opened = <String>[];
    final launcher = _RecordingExternalEditorLauncher();

    await _pumpPanel(
      tester,
      backend: backend,
      viewMode: .tree,
      onOpenFile: opened.add,
      externalEditorLauncher: launcher,
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('dirty.dart'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    expect(find.text('Open File'), findsOneWidget);
    expect(find.text('Open in Zed'), findsOneWidget);
    expect(find.text('Reveal in Explorer'), findsOneWidget);
    expect(find.text('Add to .gitignore'), findsOneWidget);
    expect(find.text('Stage'), findsWidgets);
    expect(find.text('Discard'), findsWidgets);

    await tester.tap(find.text('Open File'));
    await tester.pumpAndSettle();

    expect(opened, <String>['lib/src/dirty.dart']);

    await tester.tap(find.text('dirty.dart'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Open in Zed'));
    await tester.pumpAndSettle();

    expect(launcher.fileRequests, hasLength(1));
    expect(launcher.fileRequests.single.workspacePath, '/tmp/project');
    expect(
      launcher.fileRequests.single.filePath,
      terminalAbsolutePath(
        rootPath: '/tmp/project',
        relativePath: 'lib/src/dirty.dart',
      ),
    );
  });

  testWidgets('deleted file context menu does not offer Open in Zed', (
    tester,
  ) async {
    final backend = FakeGitBackend()
      ..gitStatusResult = const GitStatusResult(
        entries: <GitChangeEntry>[
          GitChangeEntry(
            path: 'lib/src/deleted.dart',
            area: .unstaged,
            status: .deleted,
          ),
        ],
      );

    await _pumpPanel(
      tester,
      backend: backend,
      viewMode: .tree,
      onOpenFile: (_) {},
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('deleted.dart'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();

    expect(find.text('Open File'), findsNothing);
    expect(find.text('Open in Zed'), findsNothing);
    expect(find.text('Reveal in Explorer'), findsOneWidget);
  });

  testWidgets('tree folder context menu stages the folder path', (
    tester,
  ) async {
    final backend = FakeGitBackend()
      ..gitStatusResult = const GitStatusResult(
        entries: <GitChangeEntry>[
          GitChangeEntry(
            path: 'lib/src/dirty.dart',
            area: .unstaged,
            status: .modified,
          ),
        ],
      );

    await _pumpPanel(tester, backend: backend, viewMode: .tree);
    await tester.pumpAndSettle();

    await tester.tap(find.text('src'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    expect(find.text('Open File'), findsNothing);
    expect(find.text('Open in Zed'), findsNothing);
    expect(find.text('Reveal in Explorer'), findsOneWidget);

    await tester.tap(find.text('Stage').last);
    await tester.pumpAndSettle();

    expect(
      backend.calls.where((call) => call.method == 'stageArea').single.args,
      <String, Object?>{
        'path': '/tmp/project',
        'area': GitChangeArea.unstaged,
        'filePath': 'lib/src',
      },
    );
  });

  testWidgets(
    'file context menu adds the exact repo-relative file to .gitignore',
    (tester) async {
      final repo = await Directory.systemTemp.createTemp(
        'alera-gitignore-file-',
      );
      addTearDown(() => repo.delete(recursive: true));
      final backend = FakeGitBackend()
        ..gitStatusResult = const GitStatusResult(
          entries: <GitChangeEntry>[
            GitChangeEntry(
              path: 'lib/src/dirty file[1].dart',
              area: .untracked,
              status: .untracked,
            ),
          ],
        );
      final workspace = _workspace(path: repo.path);

      await _pumpPanel(
        tester,
        backend: backend,
        workspace: workspace,
        sourceControlScope: _sourceControlScope(workspace),
        viewMode: .tree,
      );
      await tester.pumpAndSettle();

      await tester.tap(
        find.text('dirty file[1].dart'),
        buttons: kSecondaryMouseButton,
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Add to .gitignore'));
      await tester.pumpAndSettle();

      final gitIgnore = File('${repo.path}${Platform.pathSeparator}.gitignore');
      expect(
        await gitIgnore.readAsString(),
        '/lib/src/dirty\\ file\\[1\\].dart\n',
      );

      await tester.tap(
        find.text('dirty file[1].dart'),
        buttons: kSecondaryMouseButton,
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Add to .gitignore'));
      await tester.pumpAndSettle();

      expect(
        await gitIgnore.readAsString(),
        '/lib/src/dirty\\ file\\[1\\].dart\n',
      );
    },
  );

  testWidgets(
    'folder context menu adds the repo-relative folder to .gitignore',
    (tester) async {
      final repo = await Directory.systemTemp.createTemp(
        'alera-gitignore-dir-',
      );
      addTearDown(() => repo.delete(recursive: true));
      final backend = FakeGitBackend()
        ..gitStatusResult = const GitStatusResult(
          entries: <GitChangeEntry>[
            GitChangeEntry(
              path: 'lib/src/dirty.dart',
              area: .untracked,
              status: .untracked,
            ),
          ],
        );
      final workspace = _workspace(path: repo.path);

      await _pumpPanel(
        tester,
        backend: backend,
        workspace: workspace,
        sourceControlScope: _sourceControlScope(workspace),
        viewMode: .tree,
      );
      await tester.pumpAndSettle();

      await tester.tap(find.text('src'), buttons: kSecondaryMouseButton);
      await tester.pumpAndSettle();
      await tester.tap(find.text('Add to .gitignore'));
      await tester.pumpAndSettle();

      final gitIgnore = File('${repo.path}${Platform.pathSeparator}.gitignore');
      expect(await gitIgnore.readAsString(), '/lib/src/\n');
    },
  );

  testWidgets('tree file context menu reveals the workspace-relative path', (
    tester,
  ) async {
    final backend = FakeGitBackend()
      ..gitStatusResult = const GitStatusResult(
        entries: <GitChangeEntry>[
          GitChangeEntry(
            path: 'lib/src/dirty.dart',
            area: .unstaged,
            status: .modified,
          ),
        ],
      );
    final revealed = <String>[];

    await _pumpPanel(
      tester,
      backend: backend,
      viewMode: .tree,
      sourceControlScope: const WorkspaceSourceControlScope(
        workspaceId: 'workspace-1',
        workspacePath: '/tmp/project',
        path: '/tmp/project/packages/app',
        relativeRoot: 'packages/app',
      ),
      onRevealInExplorer: revealed.add,
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('dirty.dart'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Reveal in Explorer'));
    await tester.pumpAndSettle();

    expect(revealed, <String>['packages/app/lib/src/dirty.dart']);
  });

  testWidgets(
    'tree file context menu reveals through the native file manager by default',
    (tester) async {
      final backend = FakeGitBackend()
        ..gitStatusResult = const GitStatusResult(
          entries: <GitChangeEntry>[
            GitChangeEntry(
              path: 'lib/src/dirty.dart',
              area: .unstaged,
              status: .modified,
            ),
          ],
        );
      final opener = _RecordingWorkspaceFolderOpener();

      await _pumpPanel(
        tester,
        backend: backend,
        workspace: _workspace(path: r'C:\repo\alera'),
        viewMode: .tree,
        workspaceFolderOpener: opener,
      );
      await tester.pumpAndSettle();

      await tester.tap(find.text('dirty.dart'), buttons: kSecondaryMouseButton);
      await tester.pumpAndSettle();
      await tester.tap(find.text('Reveal in Explorer'));
      await tester.pumpAndSettle();

      expect(opener.revealedPaths, <String>[
        terminalAbsolutePath(
          rootPath: r'C:\repo\alera',
          relativePath: 'lib/src/dirty.dart',
        ),
      ]);
    },
  );
}
