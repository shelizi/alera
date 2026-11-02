import 'package:alera/src/features/workbench/application/branch_compare_current_service.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_exception.dart';
import 'package:flutter_test/flutter_test.dart';

import 'fake_git_backend.dart';

void main() {
  late FakeGitBackend backend;
  late BranchCompareCurrentService service;

  setUp(() {
    backend = FakeGitBackend()
      ..headBranch = 'feature'
      ..sourceBranches = <String>['feature', 'main']
      ..compareRangeResult = const GitCommitCompareResult(
        summary: GitCommitCompareSummary(
          commitOid: 'main-tip-oid',
          parentOid: 'merge-base-oid',
          baseRef: 'feature',
          compareRef: 'main',
          changedFiles: 2,
          status: GitCommitCompareStatus.ready,
        ),
        entries: <GitCommitChangeEntry>[],
      );
    service = BranchCompareCurrentService(backend);
  });

  test(
    'compares selected branch against live current branch without checkout',
    () async {
      final snapshot = await service.prepare(
        path: '/repo/submodule',
        compareBranch: 'main',
      );
      expect(snapshot.baseRef, 'feature');
      expect(snapshot.headRef, 'main');
      expect(snapshot.headOid, 'main-tip-oid');
      expect(snapshot.mergeBaseOid, 'merge-base-oid');
      expect(
        backend.calls
            .where((call) => call.method == 'compareRange')
            .single
            .args,
        {'path': '/repo/submodule', 'baseRef': 'feature', 'headRef': 'main'},
      );
      expect(
        backend.calls.where((call) => call.method == 'checkoutBranch'),
        isEmpty,
      );
      expect(
        backend.calls.where((call) => call.method == 'createWorktree'),
        isEmpty,
      );
    },
  );

  test('refuses to compare a branch with itself', () async {
    await expectLater(
      service.prepare(path: '/repo', compareBranch: 'feature'),
      throwsA(isA<StateError>()),
    );
    expect(
      backend.calls.where((call) => call.method == 'compareRange'),
      isEmpty,
    );
  });

  test('refuses detached HEAD', () async {
    backend.headBranch = 'HEAD';
    await expectLater(
      service.prepare(path: '/repo', compareBranch: 'main'),
      throwsA(isA<DetachedHeadException>()),
    );
    expect(
      backend.calls.where((call) => call.method == 'compareRange'),
      isEmpty,
    );
  });

  test('refuses stale or non-local target', () async {
    await expectLater(
      service.prepare(path: '/repo', compareBranch: 'deleted-branch'),
      throwsA(isA<BranchNotFoundException>()),
    );
    expect(
      backend.calls.where((call) => call.method == 'compareRange'),
      isEmpty,
    );
  });

  test('does not open a comparison without common ancestry', () async {
    backend.compareRangeResult = const GitCommitCompareResult(
      summary: GitCommitCompareSummary(
        commitOid: 'main-tip-oid',
        parentOid: null,
        baseRef: 'feature',
        compareRef: 'main',
        changedFiles: 0,
        status: GitCommitCompareStatus.error,
        errorMessage: 'no common ancestor',
      ),
      entries: <GitCommitChangeEntry>[],
    );
    await expectLater(
      service.prepare(path: '/repo', compareBranch: 'main'),
      throwsA(isA<GitInternalException>()),
    );
  });

  test('does not create a tab when comparison backend fails', () async {
    backend.compareRangeError = const GitInternalException('failed');
    await expectLater(
      service.prepare(path: '/repo', compareBranch: 'main'),
      throwsA(isA<GitInternalException>()),
    );
  });
}
