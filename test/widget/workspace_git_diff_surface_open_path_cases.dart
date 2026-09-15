part of 'workspace_git_diff_surface_test.dart';

void _registerWorkspaceGitDiffSurfaceOpenPathTests() {
  testWidgets('diff surface disables opening deleted files', (tester) async {
    final backend = FakeGitBackend()
      ..gitDiffResult = const GitDiffResult(
        files: <GitDiffFile>[
          GitDiffFile(
            path: 'lib/deleted.dart',
            area: .staged,
            status: .deleted,
            lines: <GitDiffLine>[GitDiffLine.deletion('-old')],
            added: 0,
            removed: 1,
          ),
        ],
      );

    await _pumpDiffSurface(
      tester,
      backend: backend,
      tab: _diffTab(
        filePath: 'lib/deleted.dart',
        title: 'deleted.dart staged',
        area: .staged,
      ),
    );
    await tester.pumpAndSettle();

    expect(_openFileButton(tester).onPressed, isNull);
    expect(
      find.byTooltip('File is not available in working tree'),
      findsOneWidget,
    );
  });

  testWidgets('diff surface disables opening gitlink diffs', (tester) async {
    final backend = FakeGitBackend()
      ..gitDiffResult = const GitDiffResult(
        files: <GitDiffFile>[
          GitDiffFile(
            path: 'modules/sample',
            area: .unstaged,
            status: .modified,
            lines: <GitDiffLine>[
              GitDiffLine.addition('+Subproject commit abc123'),
            ],
            added: 1,
            removed: 1,
            isGitlink: true,
          ),
        ],
      );

    await _pumpDiffSurface(
      tester,
      backend: backend,
      tab: _diffTab(
        filePath: 'modules/sample',
        title: 'sample unstaged',
        area: .unstaged,
      ),
    );
    await tester.pumpAndSettle();

    expect(_openFileButton(tester).onPressed, isNull);
    expect(
      find.byTooltip('File is not available in working tree'),
      findsOneWidget,
    );
  });

  testWidgets('diff surface enables opening modified files', (tester) async {
    final backend = FakeGitBackend()
      ..gitDiffResult = const GitDiffResult(
        files: <GitDiffFile>[
          GitDiffFile(
            path: 'lib/main.dart',
            area: .unstaged,
            status: .modified,
            lines: <GitDiffLine>[GitDiffLine.addition('+new')],
            added: 1,
            removed: 0,
          ),
        ],
      );

    await _pumpDiffSurface(tester, backend: backend);
    await tester.pumpAndSettle();

    expect(_openFileButton(tester).onPressed, isNotNull);
    expect(find.byTooltip('Open file'), findsOneWidget);
    expect(find.byTooltip('Generate Reading Diff'), findsOneWidget);
  });

  testWidgets('diff surface disables opening rename-out old paths', (
    tester,
  ) async {
    final backend = FakeGitBackend()
      ..gitDiffResult = const GitDiffResult(
        files: <GitDiffFile>[
          GitDiffFile(
            path: 'lib/foo.dart',
            oldPath: 'lib/foo.dart',
            area: .staged,
            status: .renamed,
            lines: <GitDiffLine>[
              GitDiffLine.header('rename from packages/app/lib/foo.dart'),
            ],
          ),
        ],
      );

    await _pumpDiffSurface(
      tester,
      backend: backend,
      tab: _diffTab(
        filePath: 'lib/foo.dart',
        title: 'foo.dart staged',
        area: .staged,
      ),
    );
    await tester.pumpAndSettle();

    expect(_openFileButton(tester).onPressed, isNull);
    expect(
      find.byTooltip('File is not available in working tree'),
      findsOneWidget,
    );
  });

  testWidgets('diff surface enables opening in-workspace renames', (
    tester,
  ) async {
    final backend = FakeGitBackend()
      ..gitDiffResult = const GitDiffResult(
        files: <GitDiffFile>[
          GitDiffFile(
            path: 'lib/new.dart',
            oldPath: 'lib/old.dart',
            area: .staged,
            status: .renamed,
            lines: <GitDiffLine>[GitDiffLine.header('rename to lib/new.dart')],
          ),
        ],
      );

    await _pumpDiffSurface(
      tester,
      backend: backend,
      tab: _diffTab(
        filePath: 'lib/new.dart',
        title: 'new.dart staged',
        area: .staged,
      ),
    );
    await tester.pumpAndSettle();

    expect(_openFileButton(tester).onPressed, isNotNull);
    expect(find.byTooltip('Open file'), findsOneWidget);
  });

  testWidgets('diff surface opens loaded rename target path', (tester) async {
    final backend = FakeGitBackend()
      ..gitDiffResult = const GitDiffResult(
        files: <GitDiffFile>[
          GitDiffFile(
            path: 'lib/new.dart',
            oldPath: 'lib/old.dart',
            area: .staged,
            status: .renamed,
            lines: <GitDiffLine>[GitDiffLine.header('rename to lib/new.dart')],
          ),
        ],
      );
    final controller = _GitDiffSurfaceTestController();

    await _pumpDiffSurface(
      tester,
      backend: backend,
      controller: controller,
      tab: _diffTab(
        filePath: 'lib/old.dart',
        title: 'old.dart staged',
        area: .staged,
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byTooltip('Open file'));
    await tester.pump();

    expect(controller.openedRelativePaths, <String>['lib/new.dart']);
  });

  testWidgets('diff surface scopes nested git roots and opens workspace path', (
    tester,
  ) async {
    final backend = FakeGitBackend()
      ..gitDiffResult = const GitDiffResult(
        files: <GitDiffFile>[
          GitDiffFile(
            path: 'lib/main.dart',
            area: .unstaged,
            status: .modified,
            lines: <GitDiffLine>[GitDiffLine.addition('+new')],
          ),
        ],
      );
    final controller = _GitDiffSurfaceTestController();

    await _pumpDiffSurface(
      tester,
      backend: backend,
      controller: controller,
      tab: _diffTab(
        filePath: 'packages/app/lib/main.dart',
        title: 'main.dart unstaged',
        area: .unstaged,
        gitDiffRoot: 'packages/app',
      ),
    );
    await tester.pumpAndSettle();

    expect(
      backend.calls.where((call) => call.method == 'diff').single.args,
      <String, Object?>{
        'path': p.join('/tmp/project', 'packages', 'app'),
        'filePath': 'lib/main.dart',
        'area': GitChangeArea.unstaged,
        'whitespaceMode': GitDiffWhitespaceMode.normal,
      },
    );

    await tester.tap(find.byTooltip('Open file'));
    await tester.pump();

    expect(controller.openedRelativePaths, <String>[
      'packages/app/lib/main.dart',
    ]);
  });

  testWidgets('diff surface keeps scoped file-all outside root empty', (
    tester,
  ) async {
    final backend = FakeGitBackend()
      ..gitDiffAllResult = const GitDiffResult(
        files: <GitDiffFile>[
          GitDiffFile(
            path: 'unrelated.dart',
            area: .unstaged,
            status: .modified,
            lines: <GitDiffLine>[GitDiffLine.addition('+unrelated')],
          ),
        ],
      );

    await _pumpDiffSurface(
      tester,
      backend: backend,
      tab: _diffTab(
        scope: .fileAll,
        filePath: 'docs/main.dart',
        title: 'main.dart changes',
        area: null,
        gitDiffRoot: 'packages/app',
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('No diff available.'), findsOneWidget);
    expect(backend.calls.where((call) => call.method == 'diffAll'), isEmpty);
  });
}
