import 'package:alera/src/features/workbench/application/workspace_git_commit_compare_cache.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_exception.dart';
import 'package:flutter_test/flutter_test.dart';

import 'fake_git_backend.dart';

void main() {
  test('reuses a compare within a scope and clears it explicitly', () async {
    final backend = FakeGitBackend();
    final cache = WorkspaceGitCommitCompareCache(
      backend: backend,
      scopePath: '/tmp/project',
    );

    final first = await cache.compareFor('same-commit');
    final second = await cache.compareFor('same-commit');
    expect(identical(first, second), isTrue);
    expect(
      backend.calls.where((call) => call.method == 'commitCompare'),
      hasLength(1),
    );

    cache.clear();
    await cache.compareFor('same-commit');
    expect(
      backend.calls.where((call) => call.method == 'commitCompare'),
      hasLength(2),
    );
  });

  test(
    'rebind prevents a commit id from leaking across repository scopes',
    () async {
      final backend = FakeGitBackend();
      final cache = WorkspaceGitCommitCompareCache(
        backend: backend,
        scopePath: '/tmp/project',
      );

      await cache.compareFor('same-commit');
      cache.rebind('/tmp/project/packages/app');
      await cache.compareFor('same-commit');

      expect(
        backend.calls
            .where((call) => call.method == 'commitCompare')
            .map((call) => call.args['path']),
        <Object?>['/tmp/project', '/tmp/project/packages/app'],
      );
    },
  );

  test('rejects compare results that are not ready', () async {
    final backend = FakeGitBackend()
      ..gitCommitCompareResult = const GitCommitCompareResult(
        summary: GitCommitCompareSummary(
          commitOid: '',
          parentOid: null,
          compareRef: '',
          baseRef: '',
          changedFiles: 0,
          status: .error,
          errorMessage: 'compare failed',
        ),
        entries: <GitCommitChangeEntry>[],
      );
    final cache = WorkspaceGitCommitCompareCache(
      backend: backend,
      scopePath: '/tmp/project',
    );

    await expectLater(
      cache.compareFor('broken-commit'),
      throwsA(
        isA<GitInternalException>().having(
          (error) => error.context,
          'context',
          'compare failed',
        ),
      ),
    );
    await expectLater(
      cache.compareFor('broken-commit'),
      throwsA(isA<GitInternalException>()),
    );
    expect(
      backend.calls.where((call) => call.method == 'commitCompare'),
      hasLength(2),
    );
  });
}
