import 'package:alera/src/design_system/feedback/alera_toast.dart';
import 'package:alera/src/shared/infra/git/git_commit_ops_models.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_exception.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter_test/flutter_test.dart';

import 'workspace_git_history_surface_test_harness.dart';

void main() {
  testWidgets('revert commit confirms and runs through source control', (
    tester,
  ) async {
    final toasts = captureAleraToasts();
    final backend = gitHistoryBackend(<GitHistoryItem>[
      gitHistoryCommit(
        'abc123def',
        parents: <String>['c0'],
        subject: 'Only Commit',
      ),
    ]);
    final repository = GitHistoryFakeWorkbenchRepository()
      ..tabs.add(gitHistoryTab());

    await pumpGitHistorySurface(
      tester,
      backend: backend,
      repository: repository,
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Only Commit'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Revert Commit'));
    await tester.pumpAndSettle();

    expect(find.text('Revert Commit?'), findsOneWidget);
    await tester.tap(find.text('Revert'));
    await tester.pumpAndSettle();

    final reverts = backend.calls
        .where((call) => call.method == 'revertCommit')
        .toList();
    expect(reverts, hasLength(1));
    expect(reverts.single.args['commitId'], 'abc123def');
    expect(reverts.single.args['mainlineParent'], isNull);
    expect(reverts.single.args['path'], '/tmp/project');
    expect(toasts.last.message, 'Reverted abc123d');
    expect(toasts.last.tone, AleraToastTone.success);
    expect(
      backend.calls.where((call) => call.method == 'history'),
      hasLength(2),
    );
  });
  testWidgets('reverting a merge commit passes the first-parent mainline', (
    tester,
  ) async {
    final backend = gitHistoryBackend(<GitHistoryItem>[
      gitHistoryCommit(
        'merge001',
        parents: <String>['p1', 'p2'],
        subject: 'Merge Commit',
      ),
    ]);
    final repository = GitHistoryFakeWorkbenchRepository()
      ..tabs.add(gitHistoryTab());

    await pumpGitHistorySurface(
      tester,
      backend: backend,
      repository: repository,
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Merge Commit'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Revert Commit'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Revert'));
    await tester.pumpAndSettle();

    final reverts = backend.calls
        .where((call) => call.method == 'revertCommit')
        .toList();
    expect(reverts.single.args['mainlineParent'], 1);
  });
  testWidgets('cancelling revert leaves the backend untouched', (tester) async {
    final backend = gitHistoryBackend(<GitHistoryItem>[
      gitHistoryCommit(
        'abc123',
        parents: <String>['c0'],
        subject: 'Only Commit',
      ),
    ]);
    final repository = GitHistoryFakeWorkbenchRepository()
      ..tabs.add(gitHistoryTab());

    await pumpGitHistorySurface(
      tester,
      backend: backend,
      repository: repository,
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Only Commit'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Revert Commit'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Cancel'));
    await tester.pumpAndSettle();

    expect(
      backend.calls.where((call) => call.method == 'revertCommit'),
      isEmpty,
    );
  });
  testWidgets('hard reset marks the dialog destructive and records the mode', (
    tester,
  ) async {
    final toasts = captureAleraToasts();
    final backend = gitHistoryBackend(<GitHistoryItem>[
      gitHistoryCommit(
        'abc123def',
        parents: <String>['c0'],
        subject: 'Only Commit',
      ),
    ]);
    final repository = GitHistoryFakeWorkbenchRepository()
      ..tabs.add(gitHistoryTab());

    await pumpGitHistorySurface(
      tester,
      backend: backend,
      repository: repository,
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Only Commit'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Reset Current Branch Here (Hard)'));
    await tester.pumpAndSettle();

    expect(find.text('Reset Current Branch?'), findsOneWidget);
    expect(find.textContaining('This cannot be undone.'), findsOneWidget);
    await tester.tap(find.text('Reset'));
    await tester.pumpAndSettle();

    final resets = backend.calls
        .where((call) => call.method == 'resetToCommit')
        .toList();
    expect(resets, hasLength(1));
    expect(resets.single.args['commitId'], 'abc123def');
    expect(resets.single.args['mode'], GitResetMode.hard);
    expect(toasts.last.message, 'Reset to abc123d');
  });
  testWidgets('revert failure surfaces an error toast', (tester) async {
    final toasts = captureAleraToasts();
    final backend = gitHistoryBackend(<GitHistoryItem>[
      gitHistoryCommit(
        'abc123',
        parents: <String>['c0'],
        subject: 'Only Commit',
      ),
    ])..revertCommitError = const GitConflictException('conflicting changes');
    final repository = GitHistoryFakeWorkbenchRepository()
      ..tabs.add(gitHistoryTab());

    await pumpGitHistorySurface(
      tester,
      backend: backend,
      repository: repository,
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Only Commit'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Revert Commit'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Revert'));
    await tester.pumpAndSettle();

    expect(toasts.last.tone, AleraToastTone.error);
    expect(toasts.last.message, contains('conflicting changes'));
  });
}
