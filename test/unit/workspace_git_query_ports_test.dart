import 'package:alera/src/features/workbench/application/workspace_git_commit_compare_cache.dart';
import 'package:alera/src/features/workbench/application/workspace_git_history_loader.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_query_ports.dart';
import 'package:flutter_test/flutter_test.dart';

/// These fakes deliberately implement only a single query capability each.
/// A regression to the 66-method GitBackend dependency would fail to compile.
final class _HistoryQueryFake implements GitHistoryQueryPort {
  final List<String> paths = <String>[];

  @override
  Future<GitHistoryResult> history(
    String path, {
    int? limit,
    String? baseRef,
    bool includeAllRefs = false,
    List<String>? includedRefs,
    int? offset,
    String? filePath,
  }) async {
    paths.add(path);
    return GitHistoryResult(
      currentRef: const GitHistoryItemRef(
        id: 'refs/heads/main',
        name: 'main',
        revision: 'abc',
      ),
      hasIncomingChanges: false,
      hasOutgoingChanges: false,
      hasMore: false,
      limit: limit ?? 50,
      items: const <GitHistoryItem>[],
    );
  }
}

final class _CommitCompareQueryFake implements GitCommitCompareQueryPort {
  final List<String> paths = <String>[];

  @override
  Future<GitCommitCompareResult> commitCompare({
    required String path,
    required String commitId,
  }) async {
    paths.add(path);
    return GitCommitCompareResult(
      summary: GitCommitCompareSummary(
        commitOid: commitId,
        parentOid: null,
        compareRef: '',
        baseRef: '',
        changedFiles: 0,
        status: .ready,
      ),
      entries: const <GitCommitChangeEntry>[],
    );
  }
}

void main() {
  test(
    'history loader accepts only a narrow history query and rebinds',
    () async {
      final query = _HistoryQueryFake();
      final loader = WorkspaceGitHistoryLoader(
        backend: query,
        scopePath: '/first',
        onChanged: () {},
      );

      await loader.load();
      loader.rebind('/second');
      await loader.load();

      expect(query.paths, <String>['/first', '/second']);
    },
  );

  test(
    'compare cache accepts only a narrow query and isolates scope',
    () async {
      final query = _CommitCompareQueryFake();
      final cache = WorkspaceGitCommitCompareCache(
        backend: query,
        scopePath: '/first',
      );

      final first = await cache.compareFor('abc');
      expect(await cache.compareFor('abc'), same(first));
      cache.rebind('/second');
      await cache.compareFor('abc');

      expect(query.paths, <String>['/first', '/second']);
    },
  );
}
