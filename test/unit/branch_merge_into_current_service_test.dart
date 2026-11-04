import 'package:alera/src/features/shell/application/branch_merge_into_current_service.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_exception.dart';
import 'package:flutter_test/flutter_test.dart';

import 'fake_git_backend.dart';

void main() {
  late FakeGitBackend git;
  late BranchMergeIntoCurrentService service;

  setUp(() {
    git = FakeGitBackend()
      ..sourceBranches = <String>['main', 'feature']
      ..headBranch = 'main'
      ..currentBranchesByPath['/repo'] = 'main';
    service = BranchMergeIntoCurrentService(git);
  });

  Future<BranchMergeIntoCurrentResult> merge({
    String path = '/repo',
    String source = 'feature',
    String current = 'main',
  }) => service.merge(
    path: path,
    sourceBranch: source,
    expectedCurrentBranch: current,
  );

  test('merges selected source INTO current workspace checkout', () async {
    final result = await merge();
    expect(result.conflicted, isFalse);
    expect(result.mergeCommitId, 'merge123');
    expect(git.calls.where((call) => call.method == 'mergeRef').single.args, {
      'path': '/repo',
      'ref': 'feature',
    });
    expect(
      git.calls.where(
        (call) =>
            call.method == 'createWorktree' ||
            call.method == 'listWorktrees' ||
            call.method == 'checkoutBranch',
      ),
      isEmpty,
    );
  });

  test('selected source may already be checked out elsewhere', () async {
    git.liveBranchByPath['/other-worktree'] = 'feature';
    final result = await merge();
    expect(result.conflicted, isFalse);
    expect(git.calls.where((call) => call.method == 'mergeRef'), hasLength(1));
    expect(git.calls.where((call) => call.method == 'listWorktrees'), isEmpty);
  });

  test('merges within selected nested Git repository only', () async {
    git.currentBranchesByPath['/repo/nested'] = 'main';
    await merge(path: '/repo/nested');
    expect(git.calls.where((call) => call.method == 'mergeRef').single.args, {
      'path': '/repo/nested',
      'ref': 'feature',
    });
  });

  test('refuses dirty current checkout without modifying it', () async {
    git.gitStatusResult = const GitStatusResult(
      entries: [
        GitChangeEntry(
          path: 'pending.dart',
          area: GitChangeArea.unstaged,
          status: GitChangeStatus.modified,
        ),
      ],
    );
    await expectLater(merge(), throwsA(isA<StateError>()));
    expect(git.calls.where((call) => call.method == 'mergeRef'), isEmpty);
  });

  test('refuses existing conflicts in current workspace', () async {
    git.gitRepositoryStateResult = const GitRepositoryState(
      branch: 'main',
      hasConflicts: true,
    );
    await expectLater(merge(), throwsA(isA<StateError>()));
    expect(git.calls.where((call) => call.method == 'mergeRef'), isEmpty);
  });

  test('leaves merge conflicts in current checkout for resolution', () async {
    git.mergeRefError = const GitConflictException('conflict');
    final result = await merge();
    expect(result.conflicted, isTrue);
    expect(result.mergeCommitId, isNull);
    expect(git.calls.where((call) => call.method == 'mergeRef').single.args, {
      'path': '/repo',
      'ref': 'feature',
    });
  });

  test('rejects stale checkout branch after menu opened', () async {
    git.currentBranchesByPath['/repo'] = 'another';
    await expectLater(merge(), throwsA(isA<StateError>()));
    expect(git.calls.where((call) => call.method == 'mergeRef'), isEmpty);
  });

  test('rejects a source branch that disappeared', () async {
    git.sourceBranches = <String>['main'];
    await expectLater(merge(), throwsA(isA<StateError>()));
    expect(git.calls.where((call) => call.method == 'mergeRef'), isEmpty);
  });

  test('refuses detached checkout and self-merge', () async {
    await expectLater(merge(current: 'HEAD'), throwsA(isA<StateError>()));
    await expectLater(merge(source: 'main'), throwsA(isA<StateError>()));
    expect(git.calls.where((call) => call.method == 'mergeRef'), isEmpty);
  });
}
