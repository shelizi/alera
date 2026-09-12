part of 'workspace_git_history_panel_test.dart';

void _registerHistoryScopeTests() {
  testWidgets('late history from an old repository scope is ignored', (
    tester,
  ) async {
    final firstHistory = Completer<GitHistoryResult>();
    final workspace = _workspace();
    final watcher = FakeSourceControlWatcher();
    final backend = FakeGitBackend()
      ..gitHistoryResultQueue.add(firstHistory.future);

    await _pumpPanel(
      tester,
      backend: backend,
      workspace: workspace,
      watcher: watcher,
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('COMMITS'));
    await tester.pump();

    backend.gitHistoryResult = _historyWithCommit(
      id: 'repo-b-commit',
      subject: 'Repo B Commit',
    );
    final nestedScope = _nestedScope(workspace);
    await _pumpPanel(
      tester,
      backend: backend,
      workspace: workspace,
      sourceControlScope: nestedScope,
      watcher: watcher,
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('COMMITS'));
    await tester.pumpAndSettle();

    expect(find.text('Repo B Commit'), findsOneWidget);

    firstHistory.complete(
      _historyWithCommit(id: 'repo-a-commit', subject: 'Repo A Commit'),
    );
    await tester.pumpAndSettle();

    expect(find.text('Repo B Commit'), findsOneWidget);
    expect(find.text('Repo A Commit'), findsNothing);
    final historyCalls = backend.calls
        .where((call) => call.method == 'history')
        .toList();
    expect(
      historyCalls.map((call) => call.args['path']),
      containsAll(<String>[workspace.path, nestedScope.path]),
    );
  });

  testWidgets('compare cache is scoped and reused within one repository', (
    tester,
  ) async {
    final workspace = _workspace();
    final watcher = FakeSourceControlWatcher();
    final backend = FakeGitBackend()
      ..gitHistoryResult = _historyWithCommit(
        id: 'same-commit',
        subject: 'Shared Commit',
      );

    await _pumpPanel(
      tester,
      backend: backend,
      workspace: workspace,
      watcher: watcher,
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('COMMITS'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Shared Commit'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Shared Commit'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Shared Commit'));
    await tester.pumpAndSettle();

    List<GitBackendCall> compareCalls() =>
        backend.calls.where((call) => call.method == 'commitCompare').toList();
    expect(compareCalls(), hasLength(1));
    expect(compareCalls().single.args['path'], workspace.path);

    final nestedScope = _nestedScope(workspace);
    await _pumpPanel(
      tester,
      backend: backend,
      workspace: workspace,
      sourceControlScope: nestedScope,
      watcher: watcher,
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('COMMITS'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Shared Commit'));
    await tester.pumpAndSettle();

    expect(compareCalls(), hasLength(2));
    expect(compareCalls().last.args['path'], nestedScope.path);
  });

  testWidgets('nested repository root scopes history and commit compare', (
    tester,
  ) async {
    final workspace = _workspace();
    final nestedScope = _nestedScope(workspace);
    final backend = FakeGitBackend()
      ..gitHistoryResult = _historyWithCommit(
        id: 'nested-commit',
        subject: 'Nested Commit',
      );

    await _pumpPanel(
      tester,
      backend: backend,
      workspace: workspace,
      sourceControlScope: nestedScope,
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('COMMITS'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Nested Commit'));
    await tester.pumpAndSettle();

    expect(
      backend.calls.where((call) => call.method == 'history').single.args,
      <String, Object?>{
        'path': nestedScope.path,
        'limit': 50,
        'baseRef': null,
        'includeAllRefs': false,
        'offset': null,
      },
    );
    expect(
      backend.calls.where((call) => call.method == 'commitCompare').single.args,
      <String, Object?>{'path': nestedScope.path, 'commitId': 'nested-commit'},
    );
  });
}
