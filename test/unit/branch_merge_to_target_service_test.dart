import 'package:alera/src/features/shell/application/branch_merge_to_target_service.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_exception.dart';
import 'package:flutter_test/flutter_test.dart';

import 'fake_git_backend.dart';

void main() {
  late FakeGitBackend git;
  late BranchMergeToTargetService service;

  setUp(() {
    git = FakeGitBackend()
      ..sourceBranches = <String>['feature', 'main']
      ..headBranch = 'feature'
      ..currentBranchesByPath['/repo'] = 'feature'
      ..currentBranchesByPath['/worktrees/main'] = 'main'
      ..liveBranchByPath = <String, String>{'/repo': 'feature'};
    service = BranchMergeToTargetService(git);
  });

  Future<String> create(String branch) async {
    expect(branch, 'main');
    return '/worktrees/main';
  }

  test('merges current branch INTO target in newly created worktree', () async {
    final result = await service.merge(
      sourcePath: '/repo',
      sourceBranch: 'feature',
      targetBranch: 'main',
      idleWorktreePaths: <String>{},
      createWorktree: create,
    );
    expect(result.conflicted, isFalse);
    expect(result.createdWorktree, isTrue);
    expect(result.mergeCommitId, 'merge123');
    final merge = git.calls.where((call) => call.method == 'mergeRef').single;
    expect(merge.args, {'path': '/worktrees/main', 'ref': 'feature'});
  });

  test('reuses known idle worktree without creating one', () async {
    git.liveBranchByPath['/worktrees/main'] = 'main';
    final result = await service.merge(
      sourcePath: '/repo',
      sourceBranch: 'feature',
      targetBranch: 'main',
      idleWorktreePaths: {'/worktrees/main'},
      createWorktree: (_) => throw StateError('must reuse idle worktree'),
    );
    expect(result.createdWorktree, isFalse);
    expect(result.path, '/worktrees/main');
    expect(git.calls.where((call) => call.method == 'mergeRef'), hasLength(1));
  });

  test('refuses busy target worktree without touching a branch', () async {
    git.liveBranchByPath['/worktrees/main'] = 'main';
    await expectLater(
      service.merge(
        sourcePath: '/repo',
        sourceBranch: 'feature',
        targetBranch: 'main',
        idleWorktreePaths: <String>{},
        createWorktree: (_) => throw StateError('must not create'),
      ),
      throwsA(isA<StateError>()),
    );
    expect(git.calls.where((call) => call.method == 'mergeRef'), isEmpty);
  });

  test('refuses dirty target checkout and preserves its changes', () async {
    git.gitRepositoryStateResult = const GitRepositoryState(
      branch: 'main',
      hasConflicts: true,
    );
    await expectLater(
      service.merge(
        sourcePath: '/repo',
        sourceBranch: 'feature',
        targetBranch: 'main',
        idleWorktreePaths: <String>{},
        createWorktree: create,
      ),
      throwsA(isA<StateError>()),
    );
    expect(git.calls.where((call) => call.method == 'mergeRef'), isEmpty);
  });

  test('retains conflicted target worktree for in-app resolution', () async {
    git.mergeRefError = const GitConflictException('conflict');
    final result = await service.merge(
      sourcePath: '/repo',
      sourceBranch: 'feature',
      targetBranch: 'main',
      idleWorktreePaths: <String>{},
      createWorktree: create,
    );
    expect(result.conflicted, isTrue);
    expect(result.path, '/worktrees/main');
    expect(result.createdWorktree, isTrue);
    expect(git.calls.where((call) => call.method == 'mergeRef'), hasLength(1));
  });

  test('rejects stale source after branch changed', () async {
    git.currentBranchesByPath['/repo'] = 'another-branch';
    await expectLater(
      service.merge(
        sourcePath: '/repo',
        sourceBranch: 'feature',
        targetBranch: 'main',
        idleWorktreePaths: <String>{},
        createWorktree: create,
      ),
      throwsA(isA<StateError>()),
    );
    expect(git.calls.where((call) => call.method == 'mergeRef'), isEmpty);
  });
}
