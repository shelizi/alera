part of 'workspace_explorer_test.dart';

void _registerWorkspaceExplorerGitSnapshotTests() {
  testWidgets('overlapping folder refreshes use one fresh trailing Git scan', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = [_directory('src', hasChildrenHint: true)]
      ..childrenByDirectory['src'] = [_file('src/main.dart')];
    final git = _BlockingSnapshotGitBackend();
    await _pumpExplorer(tester, service, gitBackend: git);
    await tester.tap(find.text('src'));
    await tester.pumpAndSettle();
    int calls() => git.calls
        .where((call) => call.method == 'explorerStatusSnapshot')
        .length;
    final baseline = calls();
    final first = Completer<void>();
    final trailing = Completer<void>();
    addTearDown(() {
      if (!first.isCompleted) first.complete();
      if (!trailing.isCompleted) trailing.complete();
    });
    git.snapshotGates.addAll([first, trailing]);
    service.emitWatchBatch(['src']);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 130));
    expect(calls(), baseline + 1);
    for (var index = 0; index < 3; index++) {
      await tester.tap(find.text('src'), buttons: kSecondaryMouseButton);
      await tester.pumpAndSettle();
      await tester.tap(find.text('Refresh'));
      await tester.pumpAndSettle();
    }
    expect(calls(), baseline + 1);
    first.complete();
    await tester.pumpAndSettle();
    expect(calls(), baseline + 2);
    expect(find.text('A'), findsNothing);
    git.gitExplorerStatusSnapshot = GitExplorerStatusSnapshot({
      'src/main.dart': GitExplorerStatus.added,
    });
    trailing.complete();
    await tester.pumpAndSettle();
    expect(calls(), baseline + 2);
    expect(find.text('A'), findsOneWidget);
  });

  testWidgets('root refresh retires a blocked watcher Git snapshot', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = [_file('main.dart')];
    final git = _BlockingSnapshotGitBackend();
    await _pumpExplorer(tester, service, gitBackend: git);
    final gate = Completer<void>();
    addTearDown(() {
      if (!gate.isCompleted) gate.complete();
    });
    git.snapshotGates.add(gate);
    service.emitWatchBatch(['']);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 130));
    expect(
      git.calls.where((call) => call.method == 'explorerStatusSnapshot'),
      hasLength(2),
    );

    git.gitExplorerStatusSnapshot = GitExplorerStatusSnapshot({
      'main.dart': GitExplorerStatus.added,
    });
    await tester.tap(find.byTooltip('Refresh'));
    await tester.pumpAndSettle();
    expect(
      git.calls.where((call) => call.method == 'explorerStatusSnapshot'),
      hasLength(3),
    );
    expect(find.text('A'), findsOneWidget);
    final reads = service.listChildrenCalls.length;
    // The retired scan completes later with different status, which must
    // not overwrite the new projection or cause another directory read.
    git.gitExplorerStatusSnapshot = GitExplorerStatusSnapshot({
      'main.dart': GitExplorerStatus.modified,
    });
    gate.complete();
    await tester.pumpAndSettle();
    expect(find.text('A'), findsOneWidget);
    expect(find.text('M'), findsNothing);
    expect(service.listChildrenCalls, hasLength(reads));
  });

  testWidgets('rows show git status indicators', (tester) async {
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
        _file('added.dart', gitStatus: native.WorkspaceFileGitStatus.added),
        _file(
          'modified.dart',
          gitStatus: native.WorkspaceFileGitStatus.modified,
        ),
        _file(
          'untracked.dart',
          gitStatus: native.WorkspaceFileGitStatus.untracked,
        ),
      ];

    await _pumpExplorer(tester, service);

    expect(find.text('A'), findsOneWidget);
    expect(find.text('M'), findsOneWidget);
    expect(find.text('U'), findsOneWidget);
  });

  testWidgets('reuses one Git snapshot across expanded directories', (
    tester,
  ) async {
    final service = _FakeWorkspaceFileService()
      ..childrenByDirectory[''] = <native.WorkspaceFileEntry>[
        _directory('src', hasChildrenHint: true),
      ]
      ..childrenByDirectory['src'] = <native.WorkspaceFileEntry>[
        _file('src/main.dart'),
      ];
    final gitBackend = FakeGitBackend();

    await _pumpExplorer(tester, service, gitBackend: gitBackend);
    expect(
      gitBackend.calls.where((call) => call.method == 'explorerStatusSnapshot'),
      hasLength(1),
    );

    await tester.tap(find.text('src'));
    await tester.pumpAndSettle();

    expect(
      gitBackend.calls.where((call) => call.method == 'explorerStatusSnapshot'),
      hasLength(1),
    );

    service.emitWatchBatch(<String>['', 'src']);
    await _pumpWatchedRefresh(tester);

    expect(
      gitBackend.calls.where((call) => call.method == 'explorerStatusSnapshot'),
      hasLength(2),
    );
  });
}
