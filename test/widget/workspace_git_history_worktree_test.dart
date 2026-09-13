import 'package:alera/src/design_system/menus/alera_dropdown_entry.dart';
import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/presentation/workspace_git_history_actions.dart';
import 'package:alera/src/features/workbench/presentation/workspace_git_history_ref_menus.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../unit/fake_git_backend.dart';
import 'workspace_git_history_surface_test_harness.dart';

void main() {
  testWidgets('commit menu creates a worktree on a new branch', (tester) async {
    final item = gitHistoryCommit(
      'commit123456',
      parents: <String>['parent123'],
      subject: 'Worktree Commit',
    );
    final backend = _historyBackend(item);
    final controller = _controller();

    await _pumpSurface(tester, backend, controller);
    await tester.tap(find.text(item.subject), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Create Worktree Here…'));
    await tester.pumpAndSettle();

    expect(find.text('Create Worktree'), findsWidgets);
    await tester.enterText(find.byType(TextField), 'wt-branch');
    await tester.tap(find.widgetWithText(FilledButton, 'Create Worktree'));
    await tester.pumpAndSettle();

    final branch = backend.calls.lastWhere(
      (call) => call.method == 'createBranchAtCommit',
    );
    expect(branch.args['commitId'], item.id);
    expect(branch.args['branch'], 'wt-branch');

    expect(controller.worktrees, hasLength(1));
    expect(controller.worktrees.single.sourceBranch, 'wt-branch');
    expect(controller.worktrees.single.newBranchName, 'wt-branch');
    expect(controller.worktrees.single.reuseExistingBranch, isTrue);
    expect(controller.worktrees.single.project.id, 'project-1');
  });

  testWidgets('local branch opens a worktree without a dialog', (tester) async {
    final backend = _refBackend();
    final controller = _controller();
    await _pumpRefSurface(tester, backend, controller);

    await tester.tap(find.text('feature'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Open in New Worktree'));
    await tester.pumpAndSettle();

    expect(
      backend.calls.where((call) => call.method == 'createBranchAtCommit'),
      isEmpty,
    );
    expect(controller.worktrees, hasLength(1));
    expect(controller.worktrees.single.sourceBranch, 'feature');
    expect(controller.worktrees.single.newBranchName, 'feature');
    expect(controller.worktrees.single.reuseExistingBranch, isTrue);
  });

  testWidgets('current branch cannot be opened in a worktree', (tester) async {
    final backend = _refBackend();
    await _pumpRefSurface(tester, backend, _controller());

    await tester.tap(find.text('main'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();

    final entry = tester.widget<AleraDropdownEntry<GitHistoryRefMenuAction>>(
      find.byWidgetPredicate(
        (widget) =>
            widget is AleraDropdownEntry<GitHistoryRefMenuAction> &&
            widget.value == GitHistoryRefMenuAction.openInWorktree,
      ),
    );
    expect(entry.enabled, isFalse);
  });

  testWidgets('remote branch worktree keeps the remote source for tracking', (
    tester,
  ) async {
    final backend = _refBackend();
    final controller = _controller();
    await _pumpRefSurface(tester, backend, controller);

    await tester.tap(find.text('origin/main'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Open in New Worktree...'));
    await tester.pumpAndSettle();

    final field = tester.widget<TextField>(find.byType(TextField));
    expect(field.controller?.text, 'main');
    await tester.tap(find.widgetWithText(FilledButton, 'Create Worktree'));
    await tester.pumpAndSettle();

    expect(controller.worktrees, hasLength(1));
    expect(controller.worktrees.single.sourceBranch, 'origin/main');
    expect(controller.worktrees.single.newBranchName, 'main');
    expect(controller.worktrees.single.reuseExistingBranch, isFalse);
  });

  testWidgets('tag worktree creates a branch at the tag first', (tester) async {
    final backend = _refBackend(withTag: true);
    final controller = _controller();
    await _pumpRefSurface(tester, backend, controller);

    await tester.tap(find.text('v1.0.0'), buttons: kSecondaryMouseButton);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Open in New Worktree...'));
    await tester.pumpAndSettle();

    final field = tester.widget<TextField>(find.byType(TextField));
    expect(field.controller?.text, 'v1.0.0');
    await tester.enterText(find.byType(TextField), 'release-1.0');
    await tester.tap(find.widgetWithText(FilledButton, 'Create Worktree'));
    await tester.pumpAndSettle();

    final branch = backend.calls.lastWhere(
      (call) => call.method == 'createBranchAtCommit',
    );
    expect(branch.args['commitId'], 'v1.0.0');
    expect(branch.args['branch'], 'release-1.0');

    expect(controller.worktrees, hasLength(1));
    expect(controller.worktrees.single.newBranchName, 'release-1.0');
    expect(controller.worktrees.single.reuseExistingBranch, isTrue);
  });
}

GitHistorySurfaceTestWorkbenchController _controller() =>
    GitHistorySurfaceTestWorkbenchController(
      WorkbenchState(projects: <Project>[gitHistoryProject()]),
    );

Future<void> _pumpSurface(
  WidgetTester tester,
  FakeGitBackend backend,
  GitHistorySurfaceTestWorkbenchController controller,
) async {
  final repository = GitHistoryFakeWorkbenchRepository()
    ..tabs.add(gitHistoryTab());
  await pumpGitHistorySurface(
    tester,
    backend: backend,
    repository: repository,
    controller: controller,
  );
  await tester.pumpAndSettle();
}

Future<void> _pumpRefSurface(
  WidgetTester tester,
  FakeGitBackend backend,
  GitHistorySurfaceTestWorkbenchController controller,
) => _pumpSurface(tester, backend, controller);

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

FakeGitBackend _refBackend({bool withTag = false}) {
  const current = GitHistoryItemRef(
    id: 'refs/heads/main',
    name: 'main',
    revision: 'head1',
    category: GitHistoryRefCategory.branches,
  );
  return FakeGitBackend()
    ..gitHistoryResult = GitHistoryResult(
      currentRef: current,
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
          references: <GitHistoryItemRef>[
            current,
            const GitHistoryItemRef(
              id: 'refs/heads/feature',
              name: 'feature',
              revision: 'head1',
              category: GitHistoryRefCategory.branches,
            ),
            const GitHistoryItemRef(
              id: 'refs/remotes/origin/main',
              name: 'origin/main',
              revision: 'head1',
              category: GitHistoryRefCategory.remoteBranches,
            ),
            if (withTag)
              const GitHistoryItemRef(
                id: 'refs/tags/v1.0.0',
                name: 'v1.0.0',
                revision: 'head1',
                category: GitHistoryRefCategory.tags,
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
