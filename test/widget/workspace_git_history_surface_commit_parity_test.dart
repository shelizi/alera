import 'package:alera/src/design_system/menus/alera_dropdown_entry.dart';
import 'package:alera/src/design_system/feedback/alera_toast.dart';
import 'package:alera/src/features/workbench/presentation/workspace_git_history_actions.dart';
import 'package:alera/src/shared/infra/git/git_commit_ops_models.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_exception.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../unit/fake_git_backend.dart';
import 'workspace_git_history_surface_test_harness.dart';

void main() {
  testWidgets('commit menu exposes the Git Graph parity actions', (
    tester,
  ) async {
    final item = gitHistoryCommit(
      'commit123456',
      parents: <String>['parent123'],
      subject: 'Parity Commit',
    );
    final backend = _historyBackend(item);

    await _pumpSurface(tester, backend);
    await _openMenu(tester, item.subject);

    for (final label in <String>[
      'Copy Commit Hash',
      'Copy Commit Subject',
      'Add Tag…',
      'Create Branch Here…',
      'Checkout Commit',
      'Cherry Pick',
      'Revert Commit',
      'Drop Commit',
      'Merge Into Current Branch',
      'Rebase Current Branch Onto This Commit',
      'Reset Current Branch Here (Soft)',
      'Reset Current Branch Here (Mixed)',
      'Reset Current Branch Here (Hard)',
      'Create Archive…',
    ]) {
      expect(find.text(label), findsOneWidget, reason: label);
    }
  });

  testWidgets('drop commit is disabled for a root commit', (tester) async {
    final item = gitHistoryCommit(
      'root123456',
      parents: <String>[],
      subject: 'Root Commit',
    );
    final backend = _historyBackend(item);

    await _pumpSurface(tester, backend);
    await _openMenu(tester, item.subject);

    final entry = tester.widget<AleraDropdownEntry<GitHistoryCommitMenuAction>>(
      find.byWidgetPredicate(
        (widget) =>
            widget is AleraDropdownEntry<GitHistoryCommitMenuAction> &&
            widget.value == GitHistoryCommitMenuAction.dropCommit,
      ),
    );
    expect(entry.enabled, isFalse);
  });

  testWidgets('cherry pick passes the merge mainline when needed', (
    tester,
  ) async {
    final item = gitHistoryCommit(
      'merge123456',
      parents: <String>['parent1', 'parent2'],
      subject: 'Merge Commit',
    );
    final backend = _historyBackend(item);

    await _pumpSurface(tester, backend);
    await _openMenu(tester, item.subject);
    await tester.tap(find.text('Cherry Pick'));
    await tester.pumpAndSettle();

    final call = _lastCall(backend, 'cherryPickCommit');
    expect(call.args['path'], '/tmp/project');
    expect(call.args['commitId'], item.id);
    expect(call.args['mainlineParent'], 1);
  });

  testWidgets('drop commit confirms and invokes the backend', (tester) async {
    final item = gitHistoryCommit(
      'drop123456',
      parents: <String>['parent123'],
      subject: 'Drop Target',
    );
    final backend = _historyBackend(item);

    await _pumpSurface(tester, backend);
    await _openMenu(tester, item.subject);
    await tester.tap(find.text('Drop Commit'));
    await tester.pumpAndSettle();

    expect(find.text('Drop Commit?'), findsOneWidget);
    expect(find.textContaining('This cannot be undone.'), findsOneWidget);
    await tester.tap(find.text('Drop'));
    await tester.pumpAndSettle();

    final call = _lastCall(backend, 'dropCommit');
    expect(call.args['path'], '/tmp/project');
    expect(call.args['commitId'], item.id);
  });

  testWidgets('merge confirms and reports a fast forward', (tester) async {
    final toasts = captureAleraToasts();
    final item = gitHistoryCommit(
      'merge123456',
      parents: <String>['parent123'],
      subject: 'Merge Commit',
    );
    final backend = _historyBackend(item)..mergeRefResult = null;

    await _pumpSurface(tester, backend);
    await _openMenu(tester, item.subject);
    await tester.tap(find.text('Merge Into Current Branch'));
    await tester.pumpAndSettle();

    expect(find.text('Merge Commit?'), findsOneWidget);
    expect(find.text('Merge merge12 into main?'), findsOneWidget);
    await tester.tap(find.text('Merge'));
    await tester.pumpAndSettle();

    final call = _lastCall(backend, 'mergeRef');
    expect(call.args['path'], '/tmp/project');
    expect(call.args['ref'], item.id);
    expect(toasts.last.message, contains('fast-forwarded'));
  });

  testWidgets('rebase confirms and invokes the backend', (tester) async {
    final item = gitHistoryCommit(
      'rebase123456',
      parents: <String>['parent123'],
      subject: 'Rebase Commit',
    );
    final backend = _historyBackend(item);

    await _pumpSurface(tester, backend);
    await _openMenu(tester, item.subject);
    await tester.tap(find.text('Rebase Current Branch Onto This Commit'));
    await tester.pumpAndSettle();

    expect(find.text('Rebase Current Branch?'), findsOneWidget);
    await tester.tap(find.text('Rebase'));
    await tester.pumpAndSettle();

    final call = _lastCall(backend, 'rebaseOnto');
    expect(call.args['path'], '/tmp/project');
    expect(call.args['ontoRef'], item.id);
  });

  testWidgets('conflicted cherry pick reports an aborted operation', (
    tester,
  ) async {
    final toasts = captureAleraToasts();
    final item = gitHistoryCommit(
      'cherry123456',
      parents: <String>['parent123'],
      subject: 'Cherry Commit',
    );
    final backend = _historyBackend(item)
      ..cherryPickCommitError = const GitConflictException('conflict');

    await _pumpSurface(tester, backend);
    await _openMenu(tester, item.subject);
    await tester.tap(find.text('Cherry Pick'));
    await tester.pumpAndSettle();

    expect(toasts.last.tone, AleraToastTone.error);
    expect(
      toasts.last.message,
      'Cherry pick was aborted because of conflicts.',
    );
  });

  testWidgets('create branch here sends the entered branch name', (
    tester,
  ) async {
    final item = gitHistoryCommit(
      'branch123456',
      parents: <String>['parent123'],
      subject: 'Branch Commit',
    );
    final backend = _historyBackend(item);

    await _pumpSurface(tester, backend);
    await _openMenu(tester, item.subject);
    await tester.tap(find.text('Create Branch Here…'));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField), 'feature/new');
    await tester.tap(find.text('Create Branch'));
    await tester.pumpAndSettle();

    final call = _lastCall(backend, 'createBranchAtCommit');
    expect(call.args['path'], '/tmp/project');
    expect(call.args['commitId'], item.id);
    expect(call.args['branch'], 'feature/new');
    expect(call.args['checkout'], isFalse);
  });

  testWidgets('add tag sends the name and optional annotation', (tester) async {
    final item = gitHistoryCommit(
      'tag123456',
      parents: <String>['parent123'],
      subject: 'Tag Commit',
    );
    final backend = _historyBackend(item);

    await _pumpSurface(tester, backend);
    await _openMenu(tester, item.subject);
    await tester.tap(find.text('Add Tag…'));
    await tester.pumpAndSettle();
    final fields = find.byType(TextField);
    await tester.enterText(fields.first, 'v1.2.3');
    await tester.enterText(fields.last, 'Release annotation');
    await tester.tap(find.widgetWithText(FilledButton, 'Add Tag'));
    await tester.pumpAndSettle();

    final call = _lastCall(backend, 'createTag');
    expect(call.args['path'], '/tmp/project');
    expect(call.args['commitId'], item.id);
    expect(call.args['name'], 'v1.2.3');
    expect(call.args['message'], 'Release annotation');
  });

  testWidgets('create archive infers tar format from the entered path', (
    tester,
  ) async {
    final item = gitHistoryCommit(
      'archive123456',
      parents: <String>['parent123'],
      subject: 'Archive Commit',
    );
    final backend = _historyBackend(item);

    await _pumpSurface(tester, backend);
    await _openMenu(tester, item.subject);
    final archiveAction = find.text('Create Archive…');
    await tester.ensureVisible(archiveAction);
    await tester.tap(archiveAction);
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField), '/tmp/project/release.tar');
    await tester.tap(find.widgetWithText(FilledButton, 'Create Archive'));
    await tester.pumpAndSettle();

    final call = _lastCall(backend, 'createArchive');
    expect(call.args['path'], '/tmp/project');
    expect(call.args['ref'], item.id);
    expect(call.args['outputPath'], '/tmp/project/release.tar');
    expect(call.args['format'], GitArchiveFormat.tar);
  });
}

FakeGitBackend _historyBackend(GitHistoryItem item) {
  return gitHistoryBackend(<GitHistoryItem>[item])
    ..gitHistoryResult = GitHistoryResult(
      items: <GitHistoryItem>[item],
      hasIncomingChanges: false,
      hasOutgoingChanges: false,
      hasMore: false,
      limit: 200,
      currentRef: GitHistoryItemRef(
        id: 'refs/heads/main',
        name: 'main',
        revision: item.id,
        category: GitHistoryRefCategory.branches,
      ),
    );
}

Future<void> _pumpSurface(WidgetTester tester, FakeGitBackend backend) async {
  final repository = GitHistoryFakeWorkbenchRepository()
    ..tabs.add(gitHistoryTab());
  await pumpGitHistorySurface(tester, backend: backend, repository: repository);
  await tester.pumpAndSettle();
}

Future<void> _openMenu(WidgetTester tester, String subject) async {
  await tester.tap(find.text(subject), buttons: kSecondaryMouseButton);
  await tester.pumpAndSettle();
}

GitBackendCall _lastCall(FakeGitBackend backend, String method) {
  return backend.calls.lastWhere((call) => call.method == method);
}
