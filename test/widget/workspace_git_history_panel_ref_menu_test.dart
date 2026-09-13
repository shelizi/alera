part of 'workspace_git_history_panel_test.dart';

void _registerHistoryRefMenuTests() {
  testWidgets('remote and tag refs expose their parity actions', (
    tester,
  ) async {
    final backend = _remoteAndTagPanelBackend();

    await _pumpPanel(tester, backend: backend, width: 420, height: 520);
    await tester.pumpAndSettle();
    await tester.tap(find.text('COMMITS'));
    await tester.pumpAndSettle();

    await tester.tap(find.text('origin/main'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    expect(find.text('Checkout Remote Branch'), findsOneWidget);
    expect(find.text('Delete Remote Branch'), findsOneWidget);
    expect(find.text('Pull into Current Branch'), findsOneWidget);
    expect(find.text('Copy Branch Name'), findsOneWidget);
    await tester.tap(find.text('Pull into Current Branch'));
    expect(backend.calls.where((call) => call.method == 'pull'), isEmpty);
    await tester.tapAt(const Offset(8, 8));
    await tester.pumpAndSettle();

    await tester.tap(find.text('v0.14.0'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    expect(find.text('Push Tag'), findsOneWidget);
    expect(find.text('Delete Tag'), findsOneWidget);
    expect(find.text('Create Archive...'), findsOneWidget);
    expect(find.text('Copy Tag Name'), findsOneWidget);
  });

  testWidgets('boundary rows expose uncommitted changes actions', (
    tester,
  ) async {
    final backend = _boundaryPanelBackend();

    await _pumpPanel(tester, backend: backend, width: 420, height: 520);
    await tester.pumpAndSettle();
    await tester.tap(find.text('COMMITS'));
    await tester.pumpAndSettle();

    await tester.tap(
      find.text('Outgoing Changes'),
      buttons: kSecondaryMouseButton,
    );
    await tester.pumpAndSettle();
    expect(find.text('Stash Changes'), findsOneWidget);
    expect(find.text('Discard All Changes'), findsOneWidget);
    expect(find.text('Commit Changes...'), findsOneWidget);
    await tester.tap(find.text('Stash Changes'));
    await tester.pumpAndSettle();

    expect(backend.calls.where((call) => call.method == 'stash'), hasLength(1));
  });
}

FakeGitBackend _boundaryPanelBackend() {
  return FakeGitBackend()
    ..gitHistoryResult = GitHistoryResult(
      currentRef: const GitHistoryItemRef(
        id: 'refs/heads/main',
        name: 'main',
        revision: 'head1',
      ),
      remoteRef: const GitHistoryItemRef(
        id: 'refs/remotes/origin/main',
        name: 'origin/main',
        revision: 'base1',
      ),
      mergeBase: 'base1',
      hasIncomingChanges: false,
      hasOutgoingChanges: true,
      hasMore: false,
      limit: 50,
      items: <GitHistoryItem>[
        GitHistoryItem(
          id: 'head1',
          parentIds: const <String>['base1'],
          subject: 'Local Commit',
          message: 'Local Commit',
        ),
        GitHistoryItem(
          id: 'base1',
          parentIds: const <String>[],
          subject: 'Base Commit',
          message: 'Base Commit',
        ),
      ],
    )
    ..gitRepositoryStateResult = const GitRepositoryState(
      branch: 'main',
      upstream: 'origin/main',
    );
}

FakeGitBackend _remoteAndTagPanelBackend() {
  return FakeGitBackend()
    ..gitRepositoryStateResult = const GitRepositoryState(
      branch: 'feat/settings-fullscreen-modal',
      upstream: 'origin/feat/settings-fullscreen-modal',
    )
    ..gitHistoryResult = GitHistoryResult(
      currentRef: const GitHistoryItemRef(
        id: 'refs/heads/feat/settings-fullscreen-modal',
        name: 'feat/settings-fullscreen-modal',
        revision: 'abc123456789',
      ),
      remoteRef: const GitHistoryItemRef(
        id: 'refs/remotes/origin/feat/settings-fullscreen-modal',
        name: 'origin/feat/settings-fullscreen-modal',
        revision: 'base123456789',
      ),
      hasIncomingChanges: false,
      hasOutgoingChanges: false,
      hasMore: false,
      limit: 50,
      items: <GitHistoryItem>[
        GitHistoryItem(
          id: 'abc123456789',
          parentIds: const <String>[],
          subject: 'Remote Ref Commit',
          message: 'Remote Ref Commit',
          references: const <GitHistoryItemRef>[
            GitHistoryItemRef(
              id: 'refs/remotes/origin/main',
              name: 'origin/main',
              category: GitHistoryRefCategory.remoteBranches,
            ),
          ],
        ),
        GitHistoryItem(
          id: 'base123456789',
          parentIds: const <String>[],
          subject: 'Tag Ref Commit',
          message: 'Tag Ref Commit',
          references: const <GitHistoryItemRef>[
            GitHistoryItemRef(
              id: 'refs/tags/v0.14.0',
              name: 'v0.14.0',
              category: GitHistoryRefCategory.tags,
            ),
          ],
        ),
      ],
    );
}
