import 'package:alera/src/design_system/feedback/alera_toast.dart';
import 'package:alera/src/design_system/menus/alera_dropdown_entry.dart';
import 'package:alera/src/features/workbench/presentation/workspace_git_history_ref_menus.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter_test/flutter_test.dart';

import '../unit/fake_git_backend.dart';
import 'workspace_git_history_surface_test_harness.dart';

void main() {
  testWidgets('local branch menu exposes parity actions', (tester) async {
    final backend = _refBackend();
    await _pumpRefSurface(tester, backend);

    await tester.tap(find.text('feature'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();

    expect(find.text('Switch to Branch'), findsOneWidget);
    expect(find.text('Rename Branch...'), findsOneWidget);
    expect(find.text('Delete Branch'), findsOneWidget);
    expect(find.text('Merge into Current Branch'), findsOneWidget);
    expect(find.text('Rebase Current Branch onto feature'), findsOneWidget);
    expect(find.text('Create Archive...'), findsOneWidget);
    expect(find.text('Copy Branch Name'), findsOneWidget);
    final actions = tester
        .widgetList<AleraDropdownEntry<GitHistoryRefMenuAction>>(
          find.byType(AleraDropdownEntry<GitHistoryRefMenuAction>),
        );
    expect(actions, isNotEmpty);
    expect(actions.every((entry) => entry.localizeLabel), isTrue);
  });

  testWidgets('current branch disables switch and delete', (tester) async {
    final backend = _refBackend();
    await _pumpRefSurface(tester, backend);

    await tester.tap(find.text('main'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();

    expect(find.text('Current Branch'), findsOneWidget);
    await tester.tap(find.text('Current Branch'));
    await tester.tap(find.text('Delete Branch'));
    await tester.pump();

    expect(
      backend.calls.where((call) => call.method == 'deleteBranch'),
      isEmpty,
    );
  });

  testWidgets('delete branch retries with force after an unmerged failure', (
    tester,
  ) async {
    final toasts = captureAleraToasts();
    final backend = _refBackend()..failingBranchDeletes.add('feature');
    await _pumpRefSurface(tester, backend);

    await tester.tap(find.text('feature'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Delete Branch'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Delete'));
    await tester.pumpAndSettle();

    expect(find.text('Force Delete Branch?'), findsOneWidget);
    expect(find.textContaining('was not fully merged'), findsOneWidget);
    backend.failingBranchDeletes.clear();
    await tester.tap(find.text('Delete Forcefully'));
    await tester.pumpAndSettle();

    final deletes = backend.calls
        .where((call) => call.method == 'deleteBranch')
        .toList();
    expect(deletes, hasLength(2));
    expect(deletes[0].args['force'], isFalse);
    expect(deletes[1].args['force'], isTrue);
    expect(toasts.last.tone, AleraToastTone.success);
  });

  testWidgets('boundary rows expose and run uncommitted changes actions', (
    tester,
  ) async {
    final backend = _boundaryBackend();
    await _pumpRefSurface(tester, backend);

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

    await tester.tap(
      find.text('Outgoing Changes'),
      buttons: kSecondaryMouseButton,
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Discard All Changes'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Discard'));
    await tester.pumpAndSettle();

    expect(
      backend.calls.where((call) => call.method == 'discard'),
      hasLength(1),
    );
    expect(
      backend.calls.where((call) => call.method == 'discardArea'),
      contains(
        isA<GitBackendCall>().having(
          (call) => call.args['area'],
          'area',
          GitChangeArea.staged,
        ),
      ),
    );
  });
}

Future<void> _pumpRefSurface(
  WidgetTester tester,
  FakeGitBackend backend,
) async {
  final repository = GitHistoryFakeWorkbenchRepository()
    ..tabs.add(gitHistoryTab());
  await pumpGitHistorySurface(tester, backend: backend, repository: repository);
  await tester.pumpAndSettle();
}

FakeGitBackend _refBackend() {
  const current = GitHistoryItemRef(
    id: 'refs/heads/main',
    name: 'main',
    revision: 'head1',
    category: GitHistoryRefCategory.branches,
  );
  return FakeGitBackend()
    ..gitHistoryResult = GitHistoryResult(
      currentRef: current,
      remoteRef: const GitHistoryItemRef(
        id: 'refs/remotes/origin/main',
        name: 'origin/main',
        revision: 'head1',
        category: GitHistoryRefCategory.remoteBranches,
      ),
      hasIncomingChanges: false,
      hasOutgoingChanges: false,
      hasMore: false,
      limit: 200,
      items: <GitHistoryItem>[
        GitHistoryItem(
          id: 'head1',
          parentIds: const <String>[],
          subject: 'Ref Commit',
          message: 'Ref Commit',
          references: const <GitHistoryItemRef>[
            current,
            GitHistoryItemRef(
              id: 'refs/heads/feature',
              name: 'feature',
              revision: 'head1',
              category: GitHistoryRefCategory.branches,
            ),
          ],
        ),
      ],
    )
    ..gitRepositoryStateResult = const GitRepositoryState(
      branch: 'main',
      upstream: 'origin/main',
    );
}

FakeGitBackend _boundaryBackend() {
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
      limit: 200,
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
